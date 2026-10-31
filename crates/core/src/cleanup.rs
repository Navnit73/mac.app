//! Rule-driven discovery of safely reclaimable space (M6).
//!
//! Rules live in `rules/cleanup.toml`, embedded at build time. Each pattern has a literal root
//! (everything before its first wildcard) and items are found by walking the scan index below
//! that root. The scanner never follows symlinks, so a match is always a real descendant of the
//! root; symlinks themselves are never listed, and a root that resolves somewhere else yields
//! nothing. Right before anything moves, each item is checked again against its root on disk
//! (`verify_confined`) and then by `trash::preflight`.
//!
//! Rules run in file order. A node claimed by an earlier rule is never claimed again, and a match
//! that would contain an already claimed node is dropped, so every byte is counted once.

use std::collections::{HashMap, HashSet};
use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::path::{Component, Path, PathBuf};

use globset::{GlobBuilder, GlobMatcher, GlobSet, GlobSetBuilder};
use serde::Deserialize;

use crate::errors::AppError;
use crate::index::{flags, NodeId, NodeKind, Tree};
use crate::models::{
    CategoryId, CleanupCategory, CleanupItem, CleanupItemsPage, CleanupSummary, Risk, SelectMode,
};
use crate::trash::{self, Candidate, Context, Expected, MoveOutcome, TrashBackend};

const RULES_TOML: &str = include_str!("../rules/cleanup.toml");
const RULES_VERSION: u32 = 1;
pub const ITEMS_PAGE: usize = 100;
const DAY: i64 = 86_400;

/// Categories in display order, with what the user is told about each.
pub const CATEGORIES: &[(CategoryId, &str, &str, Risk)] = &[
    (
        CategoryId::Developer,
        "Developer files",
        "Build output and package caches. Your tools rebuild or download them again.",
        Risk::Safe,
    ),
    (
        CategoryId::Caches,
        "Caches",
        "Files apps keep to work faster. They grow back as you use your apps.",
        Risk::Safe,
    ),
    (
        CategoryId::Logs,
        "Logs",
        "Log files and crash reports. New ones are written when needed.",
        Risk::Safe,
    ),
    (
        CategoryId::OldDownloads,
        "Old downloads",
        "Downloads and installers you have not touched for 30 days. They do not come back.",
        Risk::Review,
    ),
    (
        CategoryId::Mail,
        "Mail & messaging",
        "Attachments Mail saved when you opened them. The originals stay in your mail.",
        Risk::Review,
    ),
    (
        CategoryId::Leftovers,
        "Leftovers",
        "Settings and data of apps that are no longer installed.",
        Risk::Review,
    ),
];

// ---------------------------------------------------------------------------------------------
// Rules
// ---------------------------------------------------------------------------------------------

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RuleFile {
    version: u32,
    rule: Vec<RawRule>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawRule {
    id: String,
    category: CategoryId,
    name: String,
    description: String,
    paths: Vec<String>,
    #[serde(default)]
    exclude: Vec<String>,
    #[serde(default)]
    kind: MatchKind,
    sibling: Option<String>,
    risk: RawRisk,
    #[serde(default)]
    regenerates: bool,
    requires_app_closed: Option<String>,
    #[serde(default)]
    app_from_name: bool,
    min_age_days: Option<u32>,
    min_size_mb: Option<u64>,
    #[serde(default)]
    report_only: bool,
    guidance: Option<String>,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
enum RawRisk {
    Safe,
    Review,
    Careful,
}

impl From<RawRisk> for Risk {
    fn from(r: RawRisk) -> Self {
        match r {
            RawRisk::Safe => Risk::Safe,
            RawRisk::Review => Risk::Review,
            RawRisk::Careful => Risk::Careful,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MatchKind {
    #[default]
    Any,
    Dir,
    File,
}

/// One path pattern: a literal root and an optional wildcard part below it.
#[derive(Debug)]
pub struct Pattern {
    pub root: PathBuf,
    rest: Option<GlobMatcher>,
    /// Folder levels below the root a match can be at; `None` for `**`.
    depth: Option<usize>,
}

#[derive(Debug)]
pub struct Rule {
    pub id: String,
    pub category: CategoryId,
    pub name: String,
    pub description: String,
    pub patterns: Vec<Pattern>,
    exclude: GlobSet,
    pub kind: MatchKind,
    pub sibling: Option<String>,
    pub risk: Risk,
    pub regenerates: bool,
    pub requires_app_closed: Option<String>,
    pub app_from_name: bool,
    pub min_age_days: Option<u32>,
    pub min_size: Option<u64>,
    pub report_only: bool,
    pub guidance: Option<String>,
}

#[derive(Debug)]
pub struct Rules {
    pub rules: Vec<Rule>,
}

fn has_wildcard(s: &str) -> bool {
    s.contains(['*', '?', '[', '{'])
}

/// `~/x` → `<home>/x`; absolute paths stay. Anything else, or any `.`/`..`, is rejected.
fn expand(pattern: &str, home: &Path) -> Result<PathBuf, String> {
    let p = if let Some(rest) = pattern.strip_prefix("~/") {
        home.join(rest)
    } else if pattern.starts_with('/') {
        PathBuf::from(pattern)
    } else {
        return Err(format!("pattern must start with ~/ or /: {pattern}"));
    };
    let raw = p.as_os_str().as_bytes();
    if raw.windows(2).any(|w| w == b"//")
        || p.components()
            .any(|c| matches!(c, Component::ParentDir | Component::CurDir))
        || raw
            .split(|b| *b == b'/')
            .any(|seg| seg == b"." || seg == b"..")
    {
        return Err(format!("pattern may not contain . or .. or //: {pattern}"));
    }
    Ok(p)
}

fn glob(pattern: &str) -> Result<GlobMatcher, String> {
    GlobBuilder::new(pattern)
        .literal_separator(true)
        .case_insensitive(true)
        .build()
        .map(|g| g.compile_matcher())
        .map_err(|e| format!("bad pattern {pattern}: {e}"))
}

fn parse_pattern(pattern: &str, home: &Path) -> Result<Pattern, String> {
    let full = expand(pattern, home)?;
    let mut root = PathBuf::from("/");
    let mut rest: Vec<String> = Vec::new();
    for c in full.components() {
        let Component::Normal(name) = c else { continue };
        let s = name.to_string_lossy();
        if rest.is_empty() && !has_wildcard(&s) {
            root.push(name);
        } else {
            rest.push(s.into_owned());
        }
    }
    if root.components().count() < 3 {
        return Err(format!("root too broad: {pattern}"));
    }
    // Items are strictly inside the root, so check what a child of it would be.
    if let Some(why) = trash::protected_reason(&root.join("item")) {
        return Err(format!("root is protected ({why}): {pattern}"));
    }
    let depth = (!rest.iter().any(|s| s.contains("**"))).then_some(rest.len());
    let rest = if rest.is_empty() {
        None
    } else {
        Some(glob(&rest.join("/"))?)
    };
    Ok(Pattern { root, rest, depth })
}

impl Rules {
    /// The rules shipped with the app.
    pub fn bundled(home: &Path) -> Result<Self, AppError> {
        Self::parse(RULES_TOML, home)
    }

    pub fn parse(text: &str, home: &Path) -> Result<Self, AppError> {
        let bad = |e: String| AppError::Internal(format!("cleanup rules: {e}"));
        let file: RuleFile = toml::from_str(text).map_err(|e| bad(e.to_string()))?;
        if file.version != RULES_VERSION {
            return Err(bad(format!("unsupported version {}", file.version)));
        }
        let mut ids = HashSet::new();
        let mut rules = Vec::new();
        for r in file.rule {
            if !ids.insert(r.id.clone()) {
                return Err(bad(format!("duplicate id {}", r.id)));
            }
            if r.category == CategoryId::Duplicates || r.category == CategoryId::LargeOld {
                return Err(bad(format!("{}: category not allowed in rules", r.id)));
            }
            let patterns = r
                .paths
                .iter()
                .map(|p| parse_pattern(p, home))
                .collect::<Result<Vec<_>, _>>()
                .map_err(bad)?;
            let mut ex = GlobSetBuilder::new();
            for e in &r.exclude {
                let full = expand(e, home).map_err(bad)?;
                let g = GlobBuilder::new(&full.to_string_lossy())
                    .literal_separator(true)
                    .case_insensitive(true)
                    .build()
                    .map_err(|e| bad(e.to_string()))?;
                ex.add(g);
            }
            rules.push(Rule {
                id: r.id,
                category: r.category,
                name: r.name,
                description: r.description,
                patterns,
                exclude: ex.build().map_err(|e| bad(e.to_string()))?,
                kind: r.kind,
                sibling: r.sibling,
                risk: r.risk.into(),
                regenerates: r.regenerates,
                requires_app_closed: r.requires_app_closed,
                app_from_name: r.app_from_name,
                min_age_days: r.min_age_days,
                min_size: r.min_size_mb.map(|m| m * 1_000_000),
                report_only: r.report_only,
                guidance: r.guidance,
            });
        }
        Ok(Self { rules })
    }
}

/// True when `path` is strictly inside `root` (component-wise, no `.`/`..` tricks).
pub fn within_root(path: &Path, root: &Path) -> bool {
    path != root
        && path.starts_with(root)
        && !path
            .components()
            .any(|c| matches!(c, Component::ParentDir | Component::CurDir))
}

/// On-disk check right before a move: the root is a real folder (not a symlink to somewhere
/// else) and the item's real location is still inside it.
pub fn verify_confined(path: &Path, root: &Path) -> Result<(), String> {
    const OUT: &str = "It is outside the folder this rule may clean, so it is not moved.";
    if !within_root(path, root) {
        return Err(OUT.into());
    }
    let real_root = std::fs::canonicalize(root).map_err(|_| OUT.to_string())?;
    if real_root != root {
        return Err("The rule's folder is a link to somewhere else, so it is not cleaned.".into());
    }
    let (Some(parent), Some(name)) = (path.parent(), path.file_name()) else {
        return Err(OUT.into());
    };
    let real = std::fs::canonicalize(parent)
        .map_err(|_| "The folder holding this item no longer exists.".to_string())?
        .join(name);
    if !within_root(&real, &real_root) {
        return Err(OUT.into());
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// Analysis
// ---------------------------------------------------------------------------------------------

/// An app that is open right now.
#[derive(Debug, Clone)]
pub struct RunningApp {
    pub bundle_id: String,
    pub name: String,
    pub pid: i32,
}

/// Where an item came from: a rule in the file, or the leftovers detector (with its risk).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Rule(usize),
    Leftover(Risk),
}

#[derive(Debug, Clone)]
pub struct Item {
    pub node: NodeId,
    pub source: Source,
    pub category: CategoryId,
    pub path: PathBuf,
    /// The folder this item must stay inside.
    pub root: PathBuf,
    pub bytes: u64,
    pub files: u32,
    pub mtime: i64,
    pub is_dir: bool,
    pub expected: Expected,
    /// Bundle id of the app this belongs to, when known.
    pub app: Option<String>,
    /// Name of that app if it is running.
    pub running: Option<String>,
}

/// Nodes already taken by a rule, plus every ancestor of those (to detect containment fast).
#[derive(Default)]
pub struct Claims {
    claimed: HashSet<NodeId>,
    ancestors: HashSet<NodeId>,
}

impl Claims {
    pub fn is_claimed_or_inside(&self, tree: &Tree, id: NodeId) -> bool {
        let mut cur = id;
        loop {
            if self.claimed.contains(&cur) {
                return true;
            }
            match tree.node(cur) {
                Some(n) if n.parent != cur && tree.node(n.parent).is_some() => cur = n.parent,
                _ => return false,
            }
        }
    }

    pub fn contains_claimed(&self, id: NodeId) -> bool {
        self.ancestors.contains(&id)
    }

    fn claim(&mut self, tree: &Tree, id: NodeId) {
        self.claimed.insert(id);
        let mut cur = id;
        while let Some(n) = tree.node(cur) {
            if n.parent == cur || !self.ancestors.insert(n.parent) {
                break;
            }
            cur = n.parent;
        }
    }
}

fn name_of(tree: &Tree, id: NodeId) -> &OsStr {
    OsStr::from_bytes(tree.name(id))
}

/// Newest modification time of a node and everything below it.
fn newest_mtime(tree: &Tree, id: NodeId) -> i64 {
    let mut newest = tree.node(id).map_or(0, |n| n.mtime);
    let mut stack = vec![id];
    while let Some(cur) = stack.pop() {
        for c in tree.children(cur) {
            if let Some(n) = tree.node(c) {
                newest = newest.max(n.mtime);
                if n.is_dir() {
                    stack.push(c);
                }
            }
        }
    }
    newest
}

fn has_sibling_file(tree: &Tree, id: NodeId, name: &str) -> bool {
    let Some(parent) = tree.node(id).map(|n| n.parent) else {
        return false;
    };
    tree.children(parent).any(|c| {
        tree.node(c).is_some_and(|n| n.kind == NodeKind::File)
            && name_of(tree, c)
                .to_string_lossy()
                .eq_ignore_ascii_case(name)
    })
}

fn skip_node(n: &crate::index::Node) -> bool {
    n.kind == NodeKind::Symlink || n.has(flags::DELETED | flags::UNREADABLE)
}

fn running_for(app: &str, from_name: bool, running: &[RunningApp]) -> Option<String> {
    running
        .iter()
        .find(|r| {
            r.bundle_id.eq_ignore_ascii_case(app)
                || (from_name
                    && app
                        .to_ascii_lowercase()
                        .starts_with(&format!("{}.", r.bundle_id.to_ascii_lowercase())))
        })
        .map(|r| r.name.clone())
}

#[allow(clippy::too_many_arguments)]
fn make_item(
    tree: &Tree,
    id: NodeId,
    path: PathBuf,
    root: &Path,
    source: Source,
    category: CategoryId,
    app: Option<String>,
    from_name: bool,
    running: &[RunningApp],
) -> Option<Item> {
    let n = tree.node(id)?;
    let running = app
        .as_deref()
        .and_then(|a| running_for(a, from_name, running));
    Some(Item {
        node: id,
        source,
        category,
        path,
        root: root.to_path_buf(),
        bytes: n.total_alloc,
        files: n.file_count,
        mtime: n.mtime,
        is_dir: n.is_dir(),
        expected: Expected {
            dev: n.dev,
            inode: n.inode,
            size: n.size_logical,
            mtime: n.mtime,
            is_dir: n.is_dir(),
        },
        app,
        running,
    })
}

/// Items of one rule pattern.
#[allow(clippy::too_many_arguments)]
fn match_pattern(
    tree: &Tree,
    idx: usize,
    rule: &Rule,
    pat: &Pattern,
    now: i64,
    running: &[RunningApp],
    claims: &mut Claims,
    out: &mut Vec<Item>,
) {
    let Some(root_id) = tree.find_path(&pat.root) else {
        return;
    };
    // A root that is a link (or reached through one) on disk is not trusted.
    if std::fs::canonicalize(&pat.root).is_ok_and(|real| real != pat.root) {
        return;
    }
    if claims.is_claimed_or_inside(tree, root_id) {
        return;
    }
    let mut accept = |claims: &mut Claims, id: NodeId, path: PathBuf| {
        let Some(n) = tree.node(id) else { return };
        let kind_ok = match rule.kind {
            MatchKind::Any => n.kind != NodeKind::Symlink,
            MatchKind::Dir => n.is_dir(),
            MatchKind::File => n.kind == NodeKind::File,
        };
        if !kind_ok
            || skip_node(n)
            || n.total_alloc == 0
            || claims.contains_claimed(id)
            || rule.min_size.is_some_and(|m| n.total_alloc < m)
            || rule
                .sibling
                .as_deref()
                .is_some_and(|s| !has_sibling_file(tree, id, s))
        {
            return;
        }
        if let Some(days) = rule.min_age_days {
            let newest = if n.is_dir() {
                newest_mtime(tree, id)
            } else {
                n.mtime
            };
            if now.saturating_sub(newest) < i64::from(days) * DAY {
                return;
            }
        }
        let app = rule.requires_app_closed.clone().or_else(|| {
            rule.app_from_name
                .then(|| name_of(tree, id).to_string_lossy().into_owned())
        });
        if let Some(item) = make_item(
            tree,
            id,
            path,
            &pat.root,
            Source::Rule(idx),
            rule.category,
            app,
            rule.app_from_name,
            running,
        ) {
            claims.claim(tree, id);
            out.push(item);
        }
    };

    let Some(rest) = &pat.rest else {
        accept(claims, root_id, pat.root.clone());
        return;
    };
    // Depth-first below the root, carrying the path relative to it.
    let mut stack: Vec<(NodeId, PathBuf, usize)> = vec![(root_id, PathBuf::new(), 0)];
    while let Some((cur, rel, depth)) = stack.pop() {
        if pat.depth.is_some_and(|d| depth >= d) {
            continue;
        }
        for c in tree.children(cur) {
            let Some(n) = tree.node(c) else { continue };
            if skip_node(n) || claims.claimed.contains(&c) {
                continue;
            }
            let child_rel = rel.join(name_of(tree, c));
            let abs = pat.root.join(&child_rel);
            if rule.exclude.is_match(&abs) {
                continue;
            }
            if rest.is_match(&child_rel) {
                accept(claims, c, abs);
                // Never look inside a match, accepted or not.
                continue;
            }
            if n.is_dir() && !n.has(flags::PACKAGE) {
                stack.push((c, child_rel, depth + 1));
            }
        }
    }
}

/// Everything the rules (and the leftovers detector) found in a scan.
#[derive(Debug, Default)]
pub struct Analysis {
    pub items: Vec<Item>,
    pub trash_bytes: Option<u64>,
}

/// Run every rule over the scan, then the leftovers detector on what is left.
pub fn analyze(
    tree: &Tree,
    rules: &Rules,
    home: &Path,
    now: i64,
    running: &[RunningApp],
    installed: &HashSet<String>,
) -> Analysis {
    let mut claims = Claims::default();
    let mut items = Vec::new();
    for (idx, rule) in rules.rules.iter().enumerate() {
        for pat in &rule.patterns {
            match_pattern(tree, idx, rule, pat, now, running, &mut claims, &mut items);
        }
    }
    for l in crate::apps::leftover_nodes(tree, home, installed, &claims) {
        let risk = if l.holds_user_data() {
            Risk::Careful
        } else {
            Risk::Review
        };
        if let Some(item) = make_item(
            tree,
            l.node,
            l.path,
            &l.root,
            Source::Leftover(risk),
            CategoryId::Leftovers,
            Some(l.bundle_id),
            false,
            running,
        ) {
            claims.claim(tree, l.node);
            items.push(item);
        }
    }
    let trash_bytes = tree.find_path(&home.join(".Trash")).and_then(|id| {
        let n = tree.node(id)?;
        (!n.has(flags::UNREADABLE)).then_some(n.total_alloc)
    });
    Analysis { items, trash_bytes }
}

impl Analysis {
    /// (bytes, item count) per category, report-only items left out. Overview uses this too,
    /// so both screens always show the same totals.
    pub fn totals(&self, rules: &Rules) -> HashMap<CategoryId, (u64, u32)> {
        let mut t: HashMap<CategoryId, (u64, u32)> = HashMap::new();
        for it in self
            .items
            .iter()
            .filter(|i| !is_report_only(rules, i.source))
        {
            let e = t.entry(it.category).or_default();
            e.0 += it.bytes;
            e.1 += 1;
        }
        t
    }

    pub fn is_claimed(&self, tree: &Tree, id: NodeId) -> bool {
        let set: HashSet<NodeId> = self.items.iter().map(|i| i.node).collect();
        let mut cur = id;
        loop {
            if set.contains(&cur) {
                return true;
            }
            match tree.node(cur) {
                Some(n) if n.parent != cur && tree.node(n.parent).is_some() => cur = n.parent,
                _ => return false,
            }
        }
    }
}

fn rule_of(rules: &Rules, s: Source) -> Option<&Rule> {
    match s {
        Source::Rule(i) => rules.rules.get(i),
        Source::Leftover(_) => None,
    }
}

fn is_report_only(rules: &Rules, s: Source) -> bool {
    rule_of(rules, s).is_some_and(|r| r.report_only)
}

fn risk_of(rules: &Rules, s: Source) -> Risk {
    match s {
        Source::Rule(_) => rule_of(rules, s).map_or(Risk::Review, |r| r.risk),
        Source::Leftover(risk) => risk,
    }
}

// ---------------------------------------------------------------------------------------------
// Session: analysis plus the user's selection
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Default)]
pub struct Session {
    pub analysis: Analysis,
    selected: HashSet<NodeId>,
    /// Choices the user made, kept when the analysis is refreshed.
    touched: HashMap<NodeId, bool>,
}

impl Session {
    /// Start from the defaults: only safe items of apps that are not running.
    pub fn new(analysis: Analysis, rules: &Rules, previous: Option<&Session>) -> Self {
        let touched = previous.map(|p| p.touched.clone()).unwrap_or_default();
        let mut s = Self {
            analysis,
            selected: HashSet::new(),
            touched,
        };
        let present: HashSet<NodeId> = s.analysis.items.iter().map(|i| i.node).collect();
        s.touched.retain(|id, _| present.contains(id));
        for it in &s.analysis.items {
            let on = match s.touched.get(&it.node) {
                Some(on) => *on && selectable(rules, it),
                None => default_selected(rules, it),
            };
            if on {
                s.selected.insert(it.node);
            }
        }
        s
    }

    fn item(&self, id: NodeId) -> Option<&Item> {
        self.analysis.items.iter().find(|i| i.node == id)
    }

    /// Change single items. Careful items need `confirmed` and must be changed one at a time.
    pub fn set_selected(
        &mut self,
        rules: &Rules,
        ids: &[NodeId],
        on: bool,
        confirmed: bool,
    ) -> Result<(), AppError> {
        for id in ids {
            let it = self
                .item(*id)
                .ok_or_else(|| AppError::NotFound("not a cleanup item".into()))?;
            if on && !selectable(rules, it) {
                return Err(AppError::InvalidInput(
                    "MyMacStorage does not remove this; follow the advice shown instead.".into(),
                ));
            }
            if on && risk_of(rules, it.source) == Risk::Careful && (ids.len() != 1 || !confirmed) {
                return Err(AppError::InvalidInput(
                    "Careful items must be confirmed one at a time.".into(),
                ));
            }
        }
        for id in ids {
            self.touched.insert(*id, on);
            if on {
                self.selected.insert(*id);
            } else {
                self.selected.remove(id);
            }
        }
        Ok(())
    }

    /// Select or clear a whole category (careful and report-only items are never batch-selected).
    pub fn select_category(&mut self, rules: &Rules, cat: Option<CategoryId>, mode: SelectMode) {
        for it in &self.analysis.items {
            if cat.is_some_and(|c| c != it.category) {
                continue;
            }
            let on = match mode {
                SelectMode::All => {
                    selectable(rules, it) && risk_of(rules, it.source) != Risk::Careful
                }
                SelectMode::None => false,
                SelectMode::Defaults => default_selected(rules, it),
            };
            if mode == SelectMode::Defaults {
                self.touched.remove(&it.node);
            } else {
                self.touched.insert(it.node, on);
            }
            if on {
                self.selected.insert(it.node);
            } else {
                self.selected.remove(&it.node);
            }
        }
    }

    /// The risk of the rule that found this node, if it is an item.
    pub fn risk_of_node(&self, rules: &Rules, node: NodeId) -> Option<Risk> {
        self.item(node).map(|i| risk_of(rules, i.source))
    }

    pub fn selected_items(&self) -> Vec<&Item> {
        self.analysis
            .items
            .iter()
            .filter(|i| self.selected.contains(&i.node))
            .collect()
    }

    pub fn summary(&self, rules: &Rules) -> CleanupSummary {
        let totals = self.analysis.totals(rules);
        let mut sel: HashMap<CategoryId, (u64, u32)> = HashMap::new();
        let mut running = Vec::new();
        for it in self.selected_items() {
            let e = sel.entry(it.category).or_default();
            e.0 += it.bytes;
            e.1 += 1;
            if let Some(r) = &it.running {
                if !running.contains(r) {
                    running.push(r.clone());
                }
            }
        }
        let categories: Vec<CleanupCategory> = CATEGORIES
            .iter()
            .map(|(id, name, blurb, risk)| {
                let (bytes, items) = totals.get(id).copied().unwrap_or_default();
                let (sb, si) = sel.get(id).copied().unwrap_or_default();
                CleanupCategory {
                    id: *id,
                    name: (*name).into(),
                    blurb: (*blurb).into(),
                    risk: *risk,
                    bytes,
                    items,
                    selected_bytes: sb,
                    selected_items: si,
                }
            })
            .collect();
        CleanupSummary {
            selected_bytes: categories.iter().map(|c| c.selected_bytes).sum(),
            selected_items: categories.iter().map(|c| c.selected_items).sum(),
            categories,
            running_selected: running,
            trash_bytes: self.analysis.trash_bytes,
        }
    }

    /// One page of a category's items, largest first. Report-only items come last.
    pub fn page(&self, rules: &Rules, cat: CategoryId, page: u32) -> CleanupItemsPage {
        let mut its: Vec<&Item> = self
            .analysis
            .items
            .iter()
            .filter(|i| i.category == cat)
            .collect();
        its.sort_by_key(|i| {
            (
                is_report_only(rules, i.source),
                std::cmp::Reverse(i.bytes),
                i.node,
            )
        });
        let start = page as usize * ITEMS_PAGE;
        let end = (start + ITEMS_PAGE).min(its.len());
        CleanupItemsPage {
            items: its
                .get(start..end)
                .unwrap_or(&[])
                .iter()
                .map(|i| self.to_model(rules, i))
                .collect(),
            total: its.len() as u32,
            next_page: (end < its.len()).then_some(page + 1),
        }
    }

    fn to_model(&self, rules: &Rules, i: &Item) -> CleanupItem {
        let rule = rule_of(rules, i.source);
        CleanupItem {
            node_id: i.node,
            name: i
                .path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            path: i.path.to_string_lossy().into_owned(),
            category: i.category,
            rule_id: rule.map_or_else(|| "leftovers".into(), |r| r.id.clone()),
            rule_name: rule.map_or_else(
                || match &i.app {
                    Some(a) => format!("Left by {a}"),
                    None => "Leftover".into(),
                },
                |r| r.name.clone(),
            ),
            risk: risk_of(rules, i.source),
            regenerates: rule.is_some_and(|r| r.regenerates),
            bytes: i.bytes,
            files: i.files,
            mtime: i.mtime,
            is_dir: i.is_dir,
            running_app: i.running.clone(),
            report_only: rule.is_some_and(|r| r.report_only),
            guidance: rule.and_then(|r| r.guidance.clone()),
            selected: self.selected.contains(&i.node),
        }
    }

    /// Unselect everything the user did not see in the Review sheet. The analysis can be rebuilt
    /// between review and execution (the scan changed), adding items nobody looked at.
    pub fn limit_to_reviewed(&mut self, reviewed: &HashSet<NodeId>) {
        self.selected.retain(|id| reviewed.contains(id));
    }

    /// Move the selection to the Trash. Each item is confined to its root, its app must not be
    /// running, and `trash::preflight` checks it once more. Returns the outcome and moved nodes.
    pub fn execute(
        &self,
        rules: &Rules,
        ctx: &Context,
        backend: &dyn TrashBackend,
        still_running: &dyn Fn(&Item) -> bool,
    ) -> (MoveOutcome, Vec<NodeId>) {
        let mut skipped = Vec::new();
        let mut ok: Vec<(Candidate, u64, NodeId)> = Vec::new();
        for it in self.selected_items() {
            if !selectable(rules, it) {
                continue;
            }
            if let Err(why) = verify_confined(&it.path, &it.root) {
                skipped.push((it.path.clone(), it.bytes, why));
                continue;
            }
            if it.running.is_some() && still_running(it) {
                let app = it.running.as_deref().unwrap_or("Its app");
                skipped.push((
                    it.path.clone(),
                    it.bytes,
                    format!("{app} is open. Quit it, then try again."),
                ));
                continue;
            }
            ok.push((
                Candidate {
                    path: it.path.clone(),
                    expected: it.expected.clone(),
                },
                it.bytes,
                it.node,
            ));
        }
        let cands: Vec<(Candidate, u64)> = ok.iter().map(|(c, b, _)| (c.clone(), *b)).collect();
        let mut out = trash::move_items(&cands, ctx, backend);
        let moved: HashSet<&Path> = out.moved.iter().map(|m| m.original.as_path()).collect();
        let nodes = ok
            .iter()
            .filter(|(c, _, _)| moved.contains(c.path.as_path()))
            .map(|(_, _, n)| *n)
            .collect();
        out.skipped.extend(skipped);
        (out, nodes)
    }
}

fn selectable(rules: &Rules, it: &Item) -> bool {
    !is_report_only(rules, it.source)
}

fn default_selected(rules: &Rules, it: &Item) -> bool {
    selectable(rules, it) && risk_of(rules, it.source) == Risk::Safe && it.running.is_none()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::{NewNode, ROOT};
    use crate::scanner::{scan, std_reader::StdReader};
    use crate::trash::FakeTrash;
    use std::sync::atomic::AtomicBool;

    const NOW: i64 = 1_000 * DAY;

    fn add(
        t: &mut Tree,
        parent: NodeId,
        name: &str,
        kind: NodeKind,
        size: u64,
        mtime: i64,
    ) -> NodeId {
        t.add(
            parent,
            NewNode {
                name: name.as_bytes(),
                kind,
                flags: 0,
                dev: 1,
                inode: 0,
                mtime,
                atime: 0,
                size_logical: size,
                size_alloc: size,
            },
        )
        .unwrap()
    }

    fn dir(t: &mut Tree, parent: NodeId, name: &str) -> NodeId {
        add(t, parent, name, NodeKind::Dir, 0, NOW)
    }

    fn file(t: &mut Tree, parent: NodeId, name: &str, size: u64, mtime: i64) -> NodeId {
        add(t, parent, name, NodeKind::File, size, mtime)
    }

    fn home() -> PathBuf {
        PathBuf::from("/Users/me")
    }

    /// A home folder covering most rules.
    fn tree() -> Tree {
        let mut t = Tree::new();
        let h = add(&mut t, ROOT, "/Users/me", NodeKind::Dir, 0, NOW);
        let lib = dir(&mut t, h, "Library");
        let caches = dir(&mut t, lib, "Caches");
        let brew = dir(&mut t, caches, "Homebrew");
        file(&mut t, brew, "bottle.tar.gz", 5000, NOW);
        let slack = dir(&mut t, caches, "com.tinyspeck.slackmacgap");
        file(&mut t, slack, "blob", 3000, NOW);
        let notes = dir(&mut t, caches, "com.example.notes");
        file(&mut t, notes, "db", 1000, NOW);
        let apple = dir(&mut t, caches, "com.apple.Music");
        file(&mut t, apple, "art", 400, NOW);
        let logs = dir(&mut t, lib, "Logs");
        let diag = dir(&mut t, logs, "DiagnosticReports");
        file(&mut t, diag, "x.ips", 70, NOW);
        file(&mut t, logs, "app.log", 30, NOW);
        let dl = dir(&mut t, h, "Downloads");
        file(&mut t, dl, "old.dmg", 9000, 0);
        file(&mut t, dl, "new.zip", 2000, NOW);
        let code = dir(&mut t, h, "code");
        let rust = dir(&mut t, code, "tool");
        file(&mut t, rust, "Cargo.toml", 10, NOW);
        let target = dir(&mut t, rust, "target");
        file(&mut t, target, "bin", 8000, NOW);
        let fake = dir(&mut t, code, "notrust");
        let ftarget = dir(&mut t, fake, "target");
        file(&mut t, ftarget, "keep", 100, NOW);
        let web = dir(&mut t, code, "site");
        file(&mut t, web, "package.json", 10, 0);
        let nm = add(&mut t, web, "node_modules", NodeKind::Dir, 0, 0);
        file(&mut t, nm, "lodash.js", 600, 0);
        let fresh = dir(&mut t, code, "fresh");
        file(&mut t, fresh, "package.json", 10, NOW);
        let nm2 = dir(&mut t, fresh, "node_modules");
        file(&mut t, nm2, "react.js", 700, NOW);
        file(&mut t, code, "Installer.pkg", 300, 0);
        let desk = dir(&mut t, h, "Desktop");
        file(&mut t, desk, "Setup.pkg", 300, 0);
        let docs = dir(&mut t, h, "Documents");
        file(&mut t, docs, "Vault.dmg", 300, 0);
        t.aggregate();
        t
    }

    fn rules() -> Rules {
        Rules::bundled(&home()).unwrap()
    }

    fn run(t: &Tree, running: &[RunningApp]) -> Analysis {
        analyze(t, &rules(), &home(), NOW, running, &HashSet::new())
    }

    fn by_rule<'a>(a: &'a Analysis, r: &Rules, id: &str) -> Vec<&'a Item> {
        a.items
            .iter()
            .filter(|i| rule_of(r, i.source).is_some_and(|x| x.id == id))
            .collect()
    }

    #[test]
    fn bundled_rules_load() {
        let r = rules();
        assert!(r.rules.len() >= 15);
        assert!(r.rules.iter().all(|x| !x.patterns.is_empty()));
    }

    #[test]
    fn rules_find_the_expected_items_once() {
        let t = tree();
        let r = rules();
        let a = run(&t, &[]);
        let names = |id: &str| -> Vec<String> {
            by_rule(&a, &r, id)
                .iter()
                .map(|i| i.path.to_string_lossy().into_owned())
                .collect()
        };
        assert_eq!(
            names("package-caches"),
            ["/Users/me/Library/Caches/Homebrew"]
        );
        let caches = names("app-caches");
        assert!(caches.contains(&"/Users/me/Library/Caches/com.tinyspeck.slackmacgap".into()));
        assert!(
            !caches.iter().any(|c| c.ends_with("Homebrew")),
            "claimed by an earlier rule"
        );
        assert_eq!(
            names("system-caches"),
            ["/Users/me/Library/Caches/com.apple.Music"]
        );
        assert_eq!(
            names("crash-reports"),
            ["/Users/me/Library/Logs/DiagnosticReports/x.ips"]
        );
        assert_eq!(names("user-logs"), ["/Users/me/Library/Logs/app.log"]);
        assert_eq!(names("old-downloads"), ["/Users/me/Downloads/old.dmg"]);
        // Only Downloads and Desktop: a disk image elsewhere may be a vault, never an installer.
        assert_eq!(names("old-installers"), ["/Users/me/Desktop/Setup.pkg"]);
        assert_eq!(names("cargo-target"), ["/Users/me/code/tool/target"]);
        assert_eq!(names("node-modules"), ["/Users/me/code/site/node_modules"]);

        // No node is listed twice and none sits inside another.
        let ids: Vec<NodeId> = a.items.iter().map(|i| i.node).collect();
        let set: HashSet<NodeId> = ids.iter().copied().collect();
        assert_eq!(ids.len(), set.len());
        for it in &a.items {
            let mut p = t.node(it.node).unwrap().parent;
            while let Some(n) = t.node(p) {
                assert!(!set.contains(&p), "{} is nested", it.path.display());
                if n.parent == p {
                    break;
                }
                p = n.parent;
            }
        }
    }

    #[test]
    fn every_match_stays_inside_its_root() {
        let t = tree();
        let a = run(&t, &[]);
        for it in &a.items {
            assert!(
                within_root(&it.path, &it.root) || it.path == it.root,
                "{it:?}"
            );
            assert!(it.path.starts_with("/Users/me"));
        }
    }

    #[test]
    fn traversal_and_broad_patterns_are_rejected() {
        let h = home();
        for bad in [
            "~/Library/Caches/../../Documents/*",
            "~/Library/./Caches/*",
            "~/Library//Caches/*",
            "Library/Caches/*",
            "/*",
            "/Users/*",
            "/System/Library/Caches/*",
            "/usr/lib/*",
        ] {
            let toml = format!(
                "version = 1\n[[rule]]\nid='x'\ncategory='Caches'\nname='x'\ndescription='x'\npaths=['{bad}']\nrisk='safe'\n"
            );
            assert!(Rules::parse(&toml, &h).is_err(), "{bad} should be rejected");
        }
        let unknown =
            "version = 1\n[[rule]]\nid='x'\ncategory='Caches'\nname='x'\ndescription='x'\npaths=['~/a/*']\nrisk='safe'\nfollow_links=true\n";
        assert!(
            Rules::parse(unknown, &h).is_err(),
            "unknown fields are refused"
        );
    }

    #[test]
    fn within_root_is_component_wise() {
        let root = Path::new("/Users/me/Library/Caches");
        assert!(within_root(Path::new("/Users/me/Library/Caches/a"), root));
        assert!(!within_root(root, root));
        assert!(!within_root(Path::new("/Users/me/Library/CachesX/a"), root));
        assert!(!within_root(
            Path::new("/Users/me/Library/Caches/../Mail"),
            root
        ));
    }

    #[test]
    fn only_safe_items_of_closed_apps_are_preselected() {
        let t = tree();
        let r = rules();
        let slack = RunningApp {
            bundle_id: "com.tinyspeck.slackmacgap".into(),
            name: "Slack".into(),
            pid: 42,
        };
        let s = Session::new(run(&t, &[slack]), &r, None);
        let sel: Vec<String> = s
            .selected_items()
            .iter()
            .map(|i| i.path.to_string_lossy().into_owned())
            .collect();
        assert!(sel.contains(&"/Users/me/Library/Caches/Homebrew".into()));
        assert!(sel.contains(&"/Users/me/Library/Caches/com.example.notes".into()));
        assert!(
            !sel.iter().any(|p| p.contains("slackmacgap")),
            "Slack is running"
        );
        assert!(
            !sel.iter().any(|p| p.contains("Downloads")),
            "review is not preselected"
        );
        assert!(
            !sel.iter().any(|p| p.contains("com.apple")),
            "system caches are review"
        );
        let sum = s.summary(&r);
        assert!(sum.running_selected.is_empty());
        let slack_item = s
            .analysis
            .items
            .iter()
            .find(|i| i.path.ends_with("com.tinyspeck.slackmacgap"))
            .unwrap();
        assert_eq!(slack_item.running.as_deref(), Some("Slack"));
    }

    #[test]
    fn careful_and_report_only_rules() {
        let mut t = Tree::new();
        let h = add(&mut t, ROOT, "/Users/me", NodeKind::Dir, 0, NOW);
        let lib = dir(&mut t, h, "Library");
        let dev = dir(&mut t, lib, "Developer");
        let cs = dir(&mut t, dev, "CoreSimulator");
        let devices = dir(&mut t, cs, "Devices");
        file(&mut t, devices, "sim.img", 5000, NOW);
        t.aggregate();
        let r = rules();
        let mut s = Session::new(run(&t, &[]), &r, None);
        assert!(s.selected_items().is_empty());
        let id = s.analysis.items[0].node;
        assert!(
            s.set_selected(&r, &[id], true, true).is_err(),
            "report-only"
        );
        s.select_category(&r, None, SelectMode::All);
        assert!(s.selected_items().is_empty());
        assert_eq!(
            s.summary(&r).categories[0].bytes,
            0,
            "not counted as clearable"
        );
    }

    #[test]
    fn careful_items_need_confirmation_one_at_a_time() {
        let t = tree();
        let toml = "version = 1\n[[rule]]\nid='dl'\ncategory='OldDownloads'\nname='x'\ndescription='x'\npaths=['~/Downloads/*']\nrisk='careful'\n";
        let r = Rules::parse(toml, &home()).unwrap();
        let a = analyze(&t, &r, &home(), NOW, &[], &HashSet::new());
        let mut s = Session::new(a, &r, None);
        let ids: Vec<NodeId> = s
            .analysis
            .items
            .iter()
            .filter(|i| i.category == CategoryId::OldDownloads)
            .map(|i| i.node)
            .collect();
        assert_eq!(ids.len(), 2);
        s.select_category(&r, Some(CategoryId::OldDownloads), SelectMode::All);
        assert!(
            !s.selected_items()
                .iter()
                .any(|i| i.category == CategoryId::OldDownloads),
            "never batch-selected"
        );
        assert!(
            s.set_selected(&r, &ids, true, true).is_err(),
            "not in a batch"
        );
        assert!(
            s.set_selected(&r, &ids[..1], true, false).is_err(),
            "not unconfirmed"
        );
        s.set_selected(&r, &ids[..1], true, true).unwrap();
        assert!(s.selected_items().iter().any(|i| i.node == ids[0]));
    }

    #[test]
    fn choices_survive_a_refresh() {
        let t = tree();
        let r = rules();
        let mut s = Session::new(run(&t, &[]), &r, None);
        let brew = s
            .analysis
            .items
            .iter()
            .find(|i| i.path.ends_with("Homebrew"))
            .unwrap()
            .node;
        s.set_selected(&r, &[brew], false, false).unwrap();
        let s2 = Session::new(run(&t, &[]), &r, Some(&s));
        assert!(!s2.selected_items().iter().any(|i| i.node == brew));
        let mut s3 = s2;
        s3.select_category(&r, None, SelectMode::Defaults);
        assert!(s3.selected_items().iter().any(|i| i.node == brew));
    }

    #[test]
    fn only_reviewed_items_stay_selected() {
        let t = tree();
        let r = rules();
        let mut s = Session::new(run(&t, &[]), &r, None);
        let before: Vec<NodeId> = s.selected_items().iter().map(|i| i.node).collect();
        assert!(before.len() >= 2);
        let reviewed: HashSet<NodeId> = before.iter().skip(1).copied().collect();
        s.limit_to_reviewed(&reviewed);
        let after: HashSet<NodeId> = s.selected_items().iter().map(|i| i.node).collect();
        assert_eq!(after, reviewed);
    }

    #[test]
    fn leftover_containers_are_careful_and_never_batch_selected() {
        let mut t = Tree::new();
        let h = add(&mut t, ROOT, "/Users/me", NodeKind::Dir, 0, NOW);
        let lib = dir(&mut t, h, "Library");
        let cont = dir(&mut t, lib, "Containers");
        let journal = dir(&mut t, cont, "com.gone.journal");
        file(&mut t, journal, "entries.db", 5000, 0);
        let prefs = dir(&mut t, lib, "Preferences");
        file(&mut t, prefs, "com.gone.journal.plist", 10, 0);
        t.aggregate();
        let r = rules();
        let mut s = Session::new(run(&t, &[]), &r, None);
        let risk = |s: &Session, name: &str| {
            let it = s
                .analysis
                .items
                .iter()
                .find(|i| i.path.ends_with(name))
                .unwrap();
            risk_of(&r, it.source)
        };
        assert_eq!(risk(&s, "com.gone.journal"), Risk::Careful);
        assert_eq!(risk(&s, "com.gone.journal.plist"), Risk::Review);
        s.select_category(&r, Some(CategoryId::Leftovers), SelectMode::All);
        let picked: Vec<_> = s.selected_items().iter().map(|i| i.path.clone()).collect();
        assert_eq!(
            picked,
            [PathBuf::from(
                "/Users/me/Library/Preferences/com.gone.journal.plist"
            )]
        );
    }

    #[test]
    fn totals_match_items_and_selection() {
        let t = tree();
        let r = rules();
        let s = Session::new(run(&t, &[]), &r, None);
        let sum = s.summary(&r);
        let item_bytes: u64 = s.analysis.items.iter().map(|i| i.bytes).sum();
        let cat_bytes: u64 = sum.categories.iter().map(|c| c.bytes).sum();
        assert_eq!(item_bytes, cat_bytes);
        let sel: u64 = s.selected_items().iter().map(|i| i.bytes).sum();
        assert_eq!(sum.selected_bytes, sel);
    }

    /// Real files: a cache folder that is a symlink to a folder outside, and a rule root that is
    /// a symlink. Neither may yield anything, and the outside files must survive execution.
    #[test]
    fn symlinks_never_lead_outside() {
        use std::os::unix::fs::symlink;
        let tmp = tempfile::tempdir().unwrap();
        let base = tmp.path().canonicalize().unwrap();
        let h = base.join("home");
        let outside = base.join("outside");
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("precious.txt"), vec![1u8; 9000]).unwrap();
        let caches = h.join("Library/Caches");
        std::fs::create_dir_all(caches.join("com.real.app")).unwrap();
        std::fs::write(caches.join("com.real.app/blob"), vec![2u8; 9000]).unwrap();
        symlink(&outside, caches.join("com.evil.link")).unwrap();
        // Logs is a symlink to the outside folder: the whole rule root is untrusted.
        symlink(&outside, h.join("Library/Logs")).unwrap();

        let out = scan(
            &StdReader,
            std::slice::from_ref(&h),
            &Default::default(),
            &AtomicBool::new(false),
            &mut |_| {},
        )
        .unwrap();
        let r = Rules::bundled(&h).unwrap();
        let a = analyze(&out.tree, &r, &h, NOW, &[], &HashSet::new());
        let paths: Vec<&Path> = a.items.iter().map(|i| i.path.as_path()).collect();
        assert_eq!(paths, [caches.join("com.real.app").as_path()]);

        // Swap the real cache folder for a symlink after the scan: execution must refuse it.
        let mut s = Session::new(a, &r, None);
        s.select_category(&r, None, SelectMode::All);
        std::fs::remove_dir_all(caches.join("com.real.app")).unwrap();
        symlink(&outside, caches.join("com.real.app")).unwrap();
        let trash = FakeTrash::new(&base).unwrap();
        let (o, moved) = s.execute(&r, &Context::new(vec![h.clone()]), &trash, &|_| false);
        assert!(moved.is_empty());
        assert_eq!(o.skipped.len(), 1);
        assert!(outside.join("precious.txt").exists());
    }

    #[test]
    fn verify_confined_rejects_redirected_roots() {
        use std::os::unix::fs::symlink;
        let tmp = tempfile::tempdir().unwrap();
        let base = tmp.path().canonicalize().unwrap();
        std::fs::create_dir_all(base.join("real/a")).unwrap();
        symlink(base.join("real"), base.join("link")).unwrap();
        assert!(verify_confined(&base.join("real/a"), &base.join("real")).is_ok());
        assert!(verify_confined(&base.join("link/a"), &base.join("link")).is_err());
        assert!(verify_confined(&base.join("real"), &base.join("real")).is_err());
        assert!(verify_confined(&base.join("other/a"), &base.join("real")).is_err());
    }

    #[test]
    fn execute_moves_and_undo_restores_a_thousand_files() {
        let tmp = tempfile::tempdir().unwrap();
        let base = tmp.path().canonicalize().unwrap();
        let h = base.join("home");
        let caches = h.join("Library/Caches");
        let mut originals = Vec::new();
        for app in 0..10 {
            let d = caches.join(format!("com.test.app{app}"));
            std::fs::create_dir_all(&d).unwrap();
            for f in 0..100 {
                let p = d.join(format!("f{f}"));
                let body = format!("{app}-{f}");
                std::fs::write(&p, &body).unwrap();
                originals.push((p, body));
            }
        }
        let out = scan(
            &StdReader,
            std::slice::from_ref(&h),
            &Default::default(),
            &AtomicBool::new(false),
            &mut |_| {},
        )
        .unwrap();
        let r = Rules::bundled(&h).unwrap();
        let a = analyze(&out.tree, &r, &h, NOW, &[], &HashSet::new());
        let s = Session::new(a, &r, None);
        assert_eq!(s.selected_items().len(), 10);
        let trash = FakeTrash::new(&base).unwrap();
        let (o, moved) = s.execute(&r, &Context::new(vec![h.clone()]), &trash, &|_| false);
        assert_eq!(moved.len(), 10);
        assert!(o.skipped.is_empty(), "{:?}", o.skipped);
        assert!(originals.iter().all(|(p, _)| !p.exists()));
        for m in &o.moved {
            assert!(matches!(
                trash::restore(&m.trashed, &m.original, false),
                trash::Restore::Restored(_)
            ));
        }
        for (p, body) in &originals {
            assert_eq!(&std::fs::read_to_string(p).unwrap(), body);
        }
    }

    #[test]
    fn running_app_is_skipped_at_execution() {
        let tmp = tempfile::tempdir().unwrap();
        let base = tmp.path().canonicalize().unwrap();
        let h = base.join("home");
        let d = h.join("Library/Caches/com.busy.app");
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("x"), "x").unwrap();
        let out = scan(
            &StdReader,
            std::slice::from_ref(&h),
            &Default::default(),
            &AtomicBool::new(false),
            &mut |_| {},
        )
        .unwrap();
        let r = Rules::bundled(&h).unwrap();
        let busy = RunningApp {
            bundle_id: "com.busy.app".into(),
            name: "Busy".into(),
            pid: 1,
        };
        let a = analyze(&out.tree, &r, &h, NOW, &[busy], &HashSet::new());
        let mut s = Session::new(a, &r, None);
        assert!(s.selected_items().is_empty());
        let id = s.analysis.items[0].node;
        s.set_selected(&r, &[id], true, false).unwrap();
        assert_eq!(s.summary(&r).running_selected, ["Busy"]);
        let trash = FakeTrash::new(&base).unwrap();
        let (o, moved) = s.execute(&r, &Context::new(vec![h.clone()]), &trash, &|_| true);
        assert!(moved.is_empty());
        assert!(o.skipped[0].2.contains("Busy is open"));
        assert!(d.exists());
    }
}
