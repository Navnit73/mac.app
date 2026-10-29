//! Instant search over the scan index (M3).
//!
//! Names are stored lower-cased and NFC-normalized in one byte buffer, each followed by a NUL, so
//! a substring query is a single `memmem` pass over contiguous memory. Both the query and the index
//! are normalized, so `café` typed with a combining accent still finds a precomposed file name
//! (macOS stores names in either form).

use std::collections::HashSet;

use globset::GlobBuilder;
use memchr::memmem;
use regex::RegexBuilder;
use unicode_normalization::UnicodeNormalization;

use crate::index::{flags, NodeId, NodeKind, Tree, ROOT};
use crate::models::{
    HiddenMode, NodeRow, SearchFilters, SearchMode, SearchQuery, SearchResult, SearchSort,
};
use crate::space::{kind_of_name, row};

pub const PAGE_SIZE: usize = 200;
/// Most matches kept for listing; the true count is still reported.
pub const MAX_LISTED: usize = 10_000;

/// Lower-case NFC form used for both the index and queries.
pub fn normalize(name: &[u8]) -> String {
    if name.is_ascii() {
        // Fast path: no allocation beyond the output string.
        let mut s = String::from_utf8_lossy(name).into_owned();
        s.make_ascii_lowercase();
        return s;
    }
    String::from_utf8_lossy(name)
        .nfc()
        .collect::<String>()
        .to_lowercase()
}

#[derive(Debug)]
pub struct SearchIndex {
    /// Every node's normalized name followed by NUL, in id order (id 0 is the empty root name).
    text: Vec<u8>,
    /// `starts[id]` is where node `id`'s name begins in `text`.
    starts: Vec<u32>,
}

impl SearchIndex {
    /// Build from a tree. Deleted nodes get an empty name so they never match.
    pub fn build(tree: &Tree) -> Self {
        let mut text = Vec::with_capacity(tree.len() * 20);
        let mut starts = Vec::with_capacity(tree.len() + 1);
        starts.push(0);
        text.push(0);
        for id in tree.ids() {
            starts.push(text.len() as u32);
            let deleted = tree.node(id).is_none_or(|n| n.has(flags::DELETED));
            if !deleted {
                text.extend_from_slice(normalize(tree.name_for_index(id)).as_bytes());
            }
            text.push(0);
        }
        Self { text, starts }
    }

    fn name(&self, id: NodeId) -> &str {
        let start = self.starts[id as usize] as usize;
        let end = self.text[start..]
            .iter()
            .position(|b| *b == 0)
            .map_or(self.text.len(), |p| start + p);
        std::str::from_utf8(&self.text[start..end]).unwrap_or("")
    }

    fn id_at(&self, pos: usize) -> NodeId {
        (self.starts.partition_point(|s| (*s as usize) <= pos) - 1) as NodeId
    }

    pub fn len(&self) -> usize {
        self.starts.len().saturating_sub(1)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Outcome of matching names, before sorting.
struct Matched {
    /// Matching ids with a relevance score (lower is better).
    hits: Vec<(NodeId, u8)>,
}

fn match_names(idx: &SearchIndex, q: &SearchQuery) -> Result<Matched, String> {
    let text = normalize(q.text.as_bytes());
    let mut hits: Vec<(NodeId, u8)> = Vec::new();

    if text.is_empty() {
        hits.extend(
            idx.starts
                .iter()
                .enumerate()
                .skip(1)
                .map(|(i, _)| (i as NodeId, 3)),
        );
        return Ok(Matched { hits });
    }

    match q.mode {
        SearchMode::Substring | SearchMode::Prefix | SearchMode::Exact => {
            if text.contains('\0') {
                return Ok(Matched { hits });
            }
            let finder = memmem::Finder::new(text.as_bytes());
            let mut last: Option<NodeId> = None;
            for pos in finder.find_iter(&idx.text) {
                let id = idx.id_at(pos);
                if id == ROOT || last == Some(id) {
                    continue;
                }
                let start = idx.starts[id as usize] as usize;
                let at_start = pos == start;
                let at_end = idx.text.get(pos + text.len()) == Some(&0);
                let score = match (at_start, at_end) {
                    (true, true) => 0,
                    (true, false) => 1,
                    _ => {
                        let prev = idx.text.get(pos.wrapping_sub(1)).copied().unwrap_or(0);
                        if matches!(prev, b'.' | b'-' | b'_' | b' ') {
                            2
                        } else {
                            3
                        }
                    }
                };
                let keep = match q.mode {
                    SearchMode::Substring => true,
                    SearchMode::Prefix => at_start,
                    _ => at_start && at_end,
                };
                if keep {
                    hits.push((id, score));
                    last = Some(id);
                }
            }
        }
        SearchMode::Glob => {
            let glob = GlobBuilder::new(&text)
                .case_insensitive(true)
                .literal_separator(false)
                .build()
                .map_err(|e| e.to_string())?
                .compile_matcher();
            for id in 1..idx.starts.len() as NodeId {
                let name = idx.name(id);
                if !name.is_empty() && glob.is_match(name) {
                    hits.push((id, 3));
                }
            }
        }
        SearchMode::Regex => {
            let re = RegexBuilder::new(&text)
                .case_insensitive(true)
                .size_limit(1 << 20)
                .build()
                .map_err(|e| e.to_string())?;
            for id in 1..idx.starts.len() as NodeId {
                let name = idx.name(id);
                if !name.is_empty() && re.is_match(name) {
                    hits.push((id, 3));
                }
            }
        }
    }
    Ok(Matched { hits })
}

fn passes(tree: &Tree, id: NodeId, f: &SearchFilters, scope_ok: &dyn Fn(NodeId) -> bool) -> bool {
    let Some(n) = tree.node(id) else { return false };
    if n.has(flags::DELETED) {
        return false;
    }
    if let Some(min) = f.min_size {
        if n.total_alloc < min {
            return false;
        }
    }
    if let Some(max) = f.max_size {
        if n.total_alloc > max {
            return false;
        }
    }
    if let Some(after) = f.modified_after {
        if n.mtime < after {
            return false;
        }
    }
    if let Some(before) = f.modified_before {
        if n.mtime >= before {
            return false;
        }
    }
    if let Some(kind) = f.kind {
        if kind_of_name(
            tree.name(id),
            n.kind == NodeKind::Dir,
            n.has(flags::PACKAGE),
        ) != kind
        {
            return false;
        }
    }
    match f.hidden {
        HiddenMode::Include => {}
        HiddenMode::Exclude => {
            if n.has(flags::HIDDEN) || tree.has_flagged_ancestor(id, flags::HIDDEN) {
                return false;
            }
        }
        HiddenMode::Only => {
            if !n.has(flags::HIDDEN) && !tree.has_flagged_ancestor(id, flags::HIDDEN) {
                return false;
            }
        }
    }
    scope_ok(id)
}

/// Ids of every match, best first, capped at [`MAX_LISTED`]; also the true match count.
pub fn search_ids(
    tree: &Tree,
    idx: &SearchIndex,
    q: &SearchQuery,
) -> Result<(Vec<NodeId>, u32), String> {
    let matched = match_names(idx, q)?;
    let scope = q.filters.scope.filter(|s| tree.node(*s).is_some());
    let scope_ok = |id: NodeId| scope.is_none_or(|s| tree.is_within(id, s));
    let f = &q.filters;
    let unfiltered = f.kind.is_none()
        && f.min_size.is_none()
        && f.max_size.is_none()
        && f.modified_after.is_none()
        && f.modified_before.is_none()
        && f.hidden == HiddenMode::Include
        && scope.is_none();
    // Deleted nodes have empty names and cannot match, so with no filters every hit is valid.
    let mut hits: Vec<(NodeId, u8)> = if unfiltered {
        matched.hits
    } else {
        matched
            .hits
            .into_iter()
            .filter(|(id, _)| passes(tree, *id, f, &scope_ok))
            .collect()
    };
    let total = hits.len() as u32;

    let name_len = |id: NodeId| idx.name(id).len();
    type Key = (u64, u64, u32);
    let key = |&(id, score): &(NodeId, u8)| -> Key {
        let n = tree.node(id);
        match q.sort {
            SearchSort::Relevance => (u64::from(score), name_len(id) as u64, id),
            SearchSort::Size => (n.map_or(0, |n| n.total_alloc), 0, id),
            SearchSort::Modified => (n.map_or(0, |n| n.mtime.max(0) as u64), 0, id),
            SearchSort::Name => (0, 0, id),
        }
    };
    let by_name =
        |a: &(NodeId, u8), b: &(NodeId, u8)| idx.name(a.0).cmp(idx.name(b.0)).then(a.0.cmp(&b.0));
    // Natural order: relevance and name ascending, size and modified largest/newest first.
    // `descending` reverses the natural order.
    let natural_desc = matches!(q.sort, SearchSort::Size | SearchSort::Modified);
    let reverse = if q.sort == SearchSort::Relevance {
        q.descending
    } else {
        q.descending != natural_desc
    };
    let cmp = |a: &(NodeId, u8), b: &(NodeId, u8)| {
        let ord = if q.sort == SearchSort::Name {
            by_name(a, b)
        } else {
            key(a).cmp(&key(b))
        };
        if reverse {
            ord.reverse()
        } else {
            ord
        }
    };

    if hits.len() > MAX_LISTED {
        hits.select_nth_unstable_by(MAX_LISTED, cmp);
        hits.truncate(MAX_LISTED);
    }
    hits.sort_unstable_by(cmp);
    Ok((hits.into_iter().map(|(id, _)| id).collect(), total))
}

/// Turn a page of ids into rows. Ids that vanished from the tree are skipped.
pub fn rows_for(tree: &Tree, ids: &[NodeId]) -> Vec<NodeRow> {
    ids.iter()
        .filter_map(|id| tree.node(*id).map(|n| row(tree, *id, n, 0, true)))
        .collect()
}

/// Full search with one page of rows.
pub fn search(tree: &Tree, idx: &SearchIndex, q: &SearchQuery, cursor: u32) -> SearchResult {
    match search_ids(tree, idx, q) {
        Ok((ids, total)) => page(tree, &ids, total, cursor),
        Err(e) => SearchResult {
            items: Vec::new(),
            total: 0,
            next_cursor: None,
            truncated: false,
            pattern_error: Some(e),
        },
    }
}

/// One page out of an already computed id list.
pub fn page(tree: &Tree, ids: &[NodeId], total: u32, cursor: u32) -> SearchResult {
    let start = (cursor as usize).min(ids.len());
    let end = (start + PAGE_SIZE).min(ids.len());
    SearchResult {
        items: rows_for(tree, &ids[start..end]),
        total,
        next_cursor: (end < ids.len()).then_some(end as u32),
        truncated: total as usize > ids.len(),
        pattern_error: None,
    }
}

/// Distinct ids helper for tests and callers that merge result sets.
pub fn unique(ids: &[NodeId]) -> usize {
    ids.iter().collect::<HashSet<_>>().len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::NewNode;

    fn add(
        t: &mut Tree,
        parent: NodeId,
        name: &str,
        kind: NodeKind,
        size: u64,
        mtime: i64,
        f: u8,
    ) -> NodeId {
        t.add(
            parent,
            NewNode {
                name: name.as_bytes(),
                kind,
                flags: f,
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

    fn sample() -> (Tree, SearchIndex) {
        let mut t = Tree::new();
        let r = add(&mut t, ROOT, "/r", NodeKind::Dir, 0, 0, 0);
        add(&mut t, r, "Report.pdf", NodeKind::File, 5000, 100, 0);
        add(
            &mut t,
            r,
            "annual-report-2024.docx",
            NodeKind::File,
            9000,
            200,
            0,
        );
        add(&mut t, r, "reporter.txt", NodeKind::File, 100, 300, 0);
        add(&mut t, r, "notes.md", NodeKind::File, 50, 400, 0);
        add(&mut t, r, "caf\u{e9}.txt", NodeKind::File, 10, 500, 0); // precomposed
        add(&mut t, r, "cafe\u{301}-nfd.txt", NodeKind::File, 10, 500, 0); // decomposed
        let h = add(&mut t, r, ".config", NodeKind::Dir, 0, 0, flags::HIDDEN);
        add(
            &mut t,
            h,
            "report-settings.json",
            NodeKind::File,
            20,
            600,
            0,
        );
        t.aggregate();
        let idx = SearchIndex::build(&t);
        (t, idx)
    }

    fn q(text: &str, mode: SearchMode) -> SearchQuery {
        SearchQuery {
            text: text.into(),
            mode,
            ..SearchQuery::default()
        }
    }

    fn names(t: &Tree, ids: &[NodeId]) -> Vec<String> {
        ids.iter()
            .map(|i| String::from_utf8_lossy(t.name(*i)).into_owned())
            .collect()
    }

    #[test]
    fn substring_is_case_insensitive_and_ranked() {
        let (t, idx) = sample();
        let (ids, total) = search_ids(&t, &idx, &q("REPORT", SearchMode::Substring)).unwrap();
        assert_eq!(total, 4);
        let n = names(&t, &ids);
        assert_eq!(
            n[0], "Report.pdf",
            "prefix match with shorter name ranks first"
        );
        assert!(n.contains(&"annual-report-2024.docx".to_string()));
    }

    #[test]
    fn prefix_and_exact_modes() {
        let (t, idx) = sample();
        let (ids, _) = search_ids(&t, &idx, &q("report", SearchMode::Prefix)).unwrap();
        let n = names(&t, &ids);
        assert!(n.contains(&"Report.pdf".into()) && n.contains(&"reporter.txt".into()));
        assert!(!n.contains(&"annual-report-2024.docx".into()));
        let (ids, _) = search_ids(&t, &idx, &q("notes.md", SearchMode::Exact)).unwrap();
        assert_eq!(names(&t, &ids), ["notes.md"]);
        let (ids, _) = search_ids(&t, &idx, &q("notes", SearchMode::Exact)).unwrap();
        assert!(ids.is_empty());
    }

    #[test]
    fn glob_and_regex_modes() {
        let (t, idx) = sample();
        let (ids, _) = search_ids(&t, &idx, &q("*.pdf", SearchMode::Glob)).unwrap();
        assert_eq!(names(&t, &ids), ["Report.pdf"]);
        let (ids, _) =
            search_ids(&t, &idx, &q(r"^report.*\.(pdf|txt)$", SearchMode::Regex)).unwrap();
        assert_eq!(unique(&ids), 2);
    }

    #[test]
    fn invalid_patterns_are_reported_not_panicked() {
        let (t, idx) = sample();
        let r = search(&t, &idx, &q("(unclosed", SearchMode::Regex), 0);
        assert!(r.pattern_error.is_some());
        assert!(r.items.is_empty());
        let r = search(&t, &idx, &q("[", SearchMode::Glob), 0);
        assert!(r.pattern_error.is_some());
    }

    #[test]
    fn unicode_forms_match_each_other() {
        let (t, idx) = sample();
        // Query typed decomposed finds the precomposed name, and the reverse.
        let (a, _) = search_ids(&t, &idx, &q("cafe\u{301}", SearchMode::Substring)).unwrap();
        let (b, _) = search_ids(&t, &idx, &q("caf\u{e9}", SearchMode::Substring)).unwrap();
        assert_eq!(a.len(), 2);
        assert_eq!(unique(&a), unique(&b));
    }

    #[test]
    fn hidden_modes() {
        let (t, idx) = sample();
        let mut query = q("report", SearchMode::Substring);
        assert_eq!(search_ids(&t, &idx, &query).unwrap().1, 4);
        query.filters.hidden = HiddenMode::Exclude;
        assert_eq!(
            search_ids(&t, &idx, &query).unwrap().1,
            3,
            "files inside hidden folders are hidden too"
        );
        query.filters.hidden = HiddenMode::Only;
        let (ids, _) = search_ids(&t, &idx, &query).unwrap();
        assert_eq!(names(&t, &ids), ["report-settings.json"]);
    }

    #[test]
    fn filters_and_sorts() {
        let (t, idx) = sample();
        let mut query = q("report", SearchMode::Substring);
        query.filters.min_size = Some(1000);
        assert_eq!(search_ids(&t, &idx, &query).unwrap().1, 2);
        query.filters.min_size = None;
        query.sort = SearchSort::Size;
        let (ids, _) = search_ids(&t, &idx, &query).unwrap();
        assert_eq!(
            names(&t, &ids)[0],
            "annual-report-2024.docx",
            "size sorts largest first"
        );
        query.descending = true;
        let (ids, _) = search_ids(&t, &idx, &query).unwrap();
        assert_eq!(
            names(&t, &ids)[0],
            "report-settings.json",
            "descending flips to smallest first"
        );
        query.sort = SearchSort::Name;
        query.descending = false;
        let (ids, _) = search_ids(&t, &idx, &query).unwrap();
        assert_eq!(names(&t, &ids)[0], "annual-report-2024.docx");
        query.sort = SearchSort::Modified;
        let (ids, _) = search_ids(&t, &idx, &query).unwrap();
        assert_eq!(names(&t, &ids)[0], "report-settings.json", "newest first");
        query.filters.modified_before = Some(250);
        query.filters.modified_after = Some(150);
        let (ids, _) = search_ids(&t, &idx, &query).unwrap();
        assert_eq!(names(&t, &ids), ["annual-report-2024.docx"]);
        let mut query = q("", SearchMode::Substring);
        query.filters.kind = Some(crate::models::FileKind::Documents);
        let (ids, _) = search_ids(&t, &idx, &query).unwrap();
        assert!(names(&t, &ids).iter().all(|n| n.ends_with(".pdf")
            || n.ends_with(".docx")
            || n.ends_with(".txt")
            || n.ends_with(".md")));
    }

    #[test]
    fn scope_limits_results() {
        let (t, idx) = sample();
        let hidden_dir = t.find_path(std::path::Path::new("/r/.config")).unwrap();
        let mut query = q("report", SearchMode::Substring);
        query.filters.scope = Some(hidden_dir);
        assert_eq!(search_ids(&t, &idx, &query).unwrap().1, 1);
    }

    #[test]
    fn deleted_nodes_never_match_and_rebuild_reflects_rescan() {
        let (mut t, _) = sample();
        let r = t.find_path(std::path::Path::new("/r")).unwrap();
        t.clear_children(r);
        t.aggregate();
        let idx = SearchIndex::build(&t);
        assert_eq!(
            search_ids(&t, &idx, &q("report", SearchMode::Substring))
                .unwrap()
                .1,
            0
        );
    }

    #[test]
    fn listing_is_capped_but_total_is_true() {
        let mut t = Tree::new();
        let r = add(&mut t, ROOT, "/r", NodeKind::Dir, 0, 0, 0);
        for i in 0..(MAX_LISTED + 500) {
            add(&mut t, r, &format!("item{i}.dat"), NodeKind::File, 1, 0, 0);
        }
        t.aggregate();
        let idx = SearchIndex::build(&t);
        let res = search(&t, &idx, &q("item", SearchMode::Substring), 0);
        assert_eq!(res.total as usize, MAX_LISTED + 500);
        assert!(res.truncated);
        assert_eq!(res.items.len(), PAGE_SIZE);
        let last = search(
            &t,
            &idx,
            &q("item", SearchMode::Substring),
            (MAX_LISTED - 1) as u32,
        );
        assert_eq!(last.items.len(), 1);
        assert!(last.next_cursor.is_none());
    }

    #[test]
    fn pages_continue_without_overlap() {
        let (t, idx) = sample();
        let all = search_ids(&t, &idx, &q("", SearchMode::Substring))
            .unwrap()
            .0;
        let p = page(&t, &all, all.len() as u32, 0);
        assert_eq!(p.items.len(), all.len());
        assert!(p.next_cursor.is_none() && !p.truncated);
    }
}
