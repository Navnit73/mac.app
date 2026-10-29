//! Real-disk scan benchmark: `cargo run --release -p duobolt-core --example scan_bench -- <dir> [std|bulk] [--views]`
//!
//! `--views` also times the Overview / Space / Inspector queries on the scanned tree.

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::time::Instant;

use duobolt_core::find::{search, SearchIndex};
use duobolt_core::models::{ChartMode, ScanOptions, SearchMode, SearchQuery, SortKey, TopFilters};
use duobolt_core::scanner::bulk::BulkReader;
use duobolt_core::scanner::scan;
use duobolt_core::scanner::std_reader::StdReader;
use duobolt_core::{apps, cleanup, index::ROOT, overview, platform, space};

fn main() {
    let mut args = std::env::args().skip(1);
    if std::env::args().nth(1).as_deref() == Some("--apps") {
        run_apps();
        return;
    }
    let Some(dir) = args.next() else {
        eprintln!("usage: scan_bench <dir> [std|bulk]");
        std::process::exit(2);
    };
    let rest: Vec<String> = args.collect();
    let which = rest
        .iter()
        .find(|a| *a == "std" || *a == "bulk")
        .cloned()
        .unwrap_or_else(|| "bulk".into());
    let views = rest.iter().any(|a| a == "--views");
    let dupes = rest.iter().any(|a| a == "--dupes");
    let clean = rest.iter().any(|a| a == "--cleanup");
    let cancel = AtomicBool::new(false);
    let opts = ScanOptions::default();
    let roots = [PathBuf::from(dir)];
    let start = Instant::now();
    let res = if which == "std" {
        scan(&StdReader, &roots, &opts, &cancel, &mut |_| {})
    } else {
        scan(&BulkReader::new(), &roots, &opts, &cancel, &mut |_| {})
    };
    match res {
        Ok(out) => {
            let secs = start.elapsed().as_secs_f64();
            let s = &out.summary;
            println!(
                "{which}: {} files, {} dirs, {:.2} GiB alloc, {} skipped, {:.2}s, {:.0} entries/s, {} KiB (du -sk style)",
                s.files,
                s.dirs,
                s.total_alloc as f64 / (1u64 << 30) as f64,
                s.skipped,
                secs,
                (s.files + s.dirs) as f64 / secs,
                s.total_alloc / 1024,
            );
            if views {
                run_views(&out);
            }
            if dupes {
                run_dupes(&out);
            }
            if clean {
                run_cleanup(&out);
            }
        }
        Err(e) => {
            eprintln!("scan failed: {e}");
            std::process::exit(1);
        }
    }
}

/// App footprints on real data: scans the app and Library folders, prints the biggest apps.
fn run_apps() {
    let home = std::path::PathBuf::from(std::env::var("HOME").unwrap_or_default());
    let t = Instant::now();
    let list = apps::discover(&apps::app_dirs(&home));
    let roots = apps::scan_roots(&home);
    let out = scan(
        &BulkReader::new(),
        &roots,
        &ScanOptions::default(),
        &AtomicBool::new(false),
        &mut |_| {},
    )
    .expect("scan");
    let rel = apps::related_all(&out.tree, &home, &list);
    println!(
        "{} apps, scan+match {:.1}s",
        list.len(),
        t.elapsed().as_secs_f64()
    );
    let paths: Vec<&std::path::Path> = list.iter().map(|a| a.path.as_path()).collect();
    let t = Instant::now();
    let used = platform::last_used_many(&paths);
    println!(
        "last used for all: {:.0} ms",
        t.elapsed().as_secs_f64() * 1000.0
    );
    let mut sums: Vec<_> = list
        .iter()
        .enumerate()
        .map(|(i, a)| {
            (
                apps::summarize(
                    i,
                    a,
                    &out.tree,
                    rel.get(&i).map_or(&[][..], |v| v),
                    used[i],
                    false,
                ),
                i,
            )
        })
        .collect();
    sums.sort_by_key(|(s, _)| std::cmp::Reverse(s.app_bytes + s.data_bytes));
    for (s, i) in sums.iter().take(12) {
        println!(
            "{:<28} {:<40} app {:>6.2} GB data {:>6.2} GB {:?} {:?} {}",
            s.name,
            s.bundle_id,
            s.app_bytes as f64 / 1e9,
            s.data_bytes as f64 / 1e9,
            s.arch,
            s.source,
            s.locked_reason.as_deref().unwrap_or("")
        );
        for r in rel.get(i).map_or(&[][..], |v| v).iter().take(8) {
            println!(
                "      {:?}/{:?} {:>8.3} GB {}",
                r.kind,
                r.confidence,
                r.bytes as f64 / 1e9,
                r.path.display()
            );
        }
    }
}

/// Rule results on a real scan: per-rule totals, the largest items, leftovers and timings.
fn run_cleanup(out: &duobolt_core::scanner::ScanOutput) {
    let home = std::path::PathBuf::from(std::env::var("HOME").unwrap_or_default());
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    let t = Instant::now();
    let running = platform::running_apps();
    let installed_apps = apps::discover(&apps::app_dirs(&home));
    let installed = apps::installed_ids(&installed_apps, &running);
    println!(
        "apps: {} installed, {} running ({:.0} ms)",
        installed_apps.len(),
        running.len(),
        t.elapsed().as_secs_f64() * 1000.0
    );
    let t = Instant::now();
    let rules = cleanup::Rules::bundled(&home).expect("rules");
    let a = cleanup::analyze(&out.tree, &rules, &home, now, &running, &installed);
    println!(
        "analyze: {:.0} ms, {} items",
        t.elapsed().as_secs_f64() * 1000.0,
        a.items.len()
    );
    let s = cleanup::Session::new(a, &rules, None);
    let sum = s.summary(&rules);
    for c in &sum.categories {
        println!(
            "  {:<18} {:>8.2} GB {:>5} items  selected {:>8.2} GB",
            c.name,
            c.bytes as f64 / 1e9,
            c.items,
            c.selected_bytes as f64 / 1e9
        );
    }
    let mut by_rule: std::collections::BTreeMap<String, (u64, u32)> = Default::default();
    for it in &s.analysis.items {
        let key = match it.source {
            cleanup::Source::Rule(i) => rules.rules[i].id.clone(),
            cleanup::Source::Leftover => "leftovers".into(),
        };
        let e = by_rule.entry(key).or_default();
        e.0 += it.bytes;
        e.1 += 1;
    }
    for (k, (b, n)) in by_rule {
        println!("    {k:<22} {:>8.2} GB {n:>5}", b as f64 / 1e9);
    }
    let mut items: Vec<_> = s.analysis.items.iter().collect();
    items.sort_by_key(|i| std::cmp::Reverse(i.bytes));
    println!("largest:");
    for it in items.iter().take(15) {
        println!(
            "    {:>8.2} GB  {}",
            it.bytes as f64 / 1e9,
            it.path.display()
        );
    }
    println!("leftovers:");
    for it in s
        .analysis
        .items
        .iter()
        .filter(|i| i.source == cleanup::Source::Leftover)
        .take(40)
    {
        println!(
            "    {:>8.3} GB  {}",
            it.bytes as f64 / 1e9,
            it.path.display()
        );
    }
    println!("running-app items:");
    for it in s
        .analysis
        .items
        .iter()
        .filter(|i| i.running.is_some())
        .take(10)
    {
        println!("    {:?}  {}", it.running, it.path.display());
    }
}

fn run_views(out: &duobolt_core::scanner::ScanOutput) {
    let home = std::path::PathBuf::from(std::env::var("HOME").unwrap_or_default());
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    let time = |label: &str, f: &mut dyn FnMut() -> String| {
        let t = Instant::now();
        let note = f();
        println!(
            "  {label:<14} {:>7.1} ms  {note}",
            t.elapsed().as_secs_f64() * 1000.0
        );
    };
    println!("FDA: {:?}", platform::full_disk_access(&home));
    let root = out.tree.children(ROOT).next().unwrap_or(ROOT);
    time("overview", &mut || {
        let vol = platform::volume_info(&home).expect("volume");
        let rules = cleanup::Rules::bundled(&home).expect("rules");
        let a = cleanup::analyze(&out.tree, &rules, &home, now, &[], &Default::default());
        let o = overview::summarize(&out.tree, &out.summary, &[], vol, now, &a, &rules);
        let cats: Vec<String> = o
            .categories
            .iter()
            .filter(|c| c.analyzed)
            .map(|c| format!("{:?}={:.1}GB", c.id, c.bytes as f64 / 1e9))
            .collect();
        format!(
            "free {:.0} GB of {:.0} GB; {}; trash {:?}",
            o.volume.available_important as f64 / 1e9,
            o.volume.total as f64 / 1e9,
            cats.join(" "),
            o.trash_bytes
        )
    });
    time("children", &mut || {
        let p = space::get_children(&out.tree, root, SortKey::Size, true, 0).expect("children");
        format!(
            "{} of {} rows, first {:?}",
            p.items.len(),
            p.total,
            p.items.first().map(|r| &r.name)
        )
    });
    time("top files", &mut || {
        let f = TopFilters {
            min_size: 100_000_000,
            ..TopFilters::default()
        };
        let r = space::get_top_files(&out.tree, &f, 500, now, Some(&home));
        format!(
            "{} matches, largest {:?}",
            r.matched_count,
            r.items.first().map(|i| (&i.name, i.size))
        )
    });
    time("smart groups", &mut || {
        let g = space::smart_groups(&out.tree, now, Some(&home));
        g.iter()
            .map(|x| format!("{:?}={}", x.group, x.count))
            .collect::<Vec<_>>()
            .join(" ")
    });
    time("sunburst", &mut || {
        let c = space::get_chart_data(&out.tree, root, 5, ChartMode::Sunburst, now).expect("chart");
        format!("{} top-level arcs", c.children.len())
    });
    let mut idx = None;
    time("search index", &mut || {
        let i = SearchIndex::build(&out.tree);
        let n = i.len();
        idx = Some(i);
        format!("{n} names")
    });
    if let Some(idx) = &idx {
        for (label, text, mode) in [
            ("find report", "report", SearchMode::Substring),
            ("find prefix", "read", SearchMode::Prefix),
            ("find e", "e", SearchMode::Substring),
            ("find glob", "*.pdf", SearchMode::Glob),
            ("find regex", r"^img_\d+\.jpe?g$", SearchMode::Regex),
        ] {
            time(label, &mut || {
                let q = SearchQuery {
                    text: text.into(),
                    mode,
                    ..SearchQuery::default()
                };
                let r = search(&out.tree, idx, &q, 0);
                format!("{} matches, {} on first page", r.total, r.items.len())
            });
        }
    }
}

fn run_dupes(out: &duobolt_core::scanner::ScanOutput) {
    use duobolt_core::duplicates::{candidates, find_duplicates, Session};
    use duobolt_core::models::DupOptions;
    use std::sync::atomic::AtomicBool;
    let t = Instant::now();
    let cands = candidates(&out.tree, &DupOptions::default());
    let n = cands.len();
    let bytes: u64 = cands.iter().map(|c| c.size).sum();
    println!(
        "dupes: {n} candidates >= 1 MB ({:.1} GB) in {:.0} ms",
        bytes as f64 / 1e9,
        t.elapsed().as_secs_f64() * 1000.0
    );
    let t = Instant::now();
    let last = std::sync::Mutex::new(String::new());
    let found = find_duplicates(
        cands,
        &AtomicBool::new(false),
        &|p| {
            if let Ok(mut l) = last.lock() {
                *l = format!(
                    "{:?} {}/{} files, {:.1}/{:.1} GB",
                    p.stage,
                    p.files_done,
                    p.files_total,
                    p.bytes_done as f64 / 1e9,
                    p.bytes_total as f64 / 1e9
                );
            }
        },
        None,
    );
    let hashing = t.elapsed();
    let s = Session::new(
        found,
        hashing.as_millis() as u64,
        &duobolt_core::platform::private_size,
    );
    println!(
        "dupes: {} groups, {} files, {:.2} GB could be freed, {} skipped, hashing {:.1} s (last stage: {})",
        s.summary.groups,
        s.summary.files,
        s.summary.potential_bytes as f64 / 1e9,
        s.summary.skipped.len(),
        hashing.as_secs_f64(),
        last.lock().map(|l| l.clone()).unwrap_or_default()
    );
    for g in s.groups.iter().take(5) {
        let names: Vec<String> = g
            .files
            .iter()
            .map(|f| f.cand.path.display().to_string())
            .collect();
        println!(
            "  {:.1} MB x {}: {}",
            g.size as f64 / 1e6,
            g.files.len(),
            names.join(" | ")
        );
    }
}
