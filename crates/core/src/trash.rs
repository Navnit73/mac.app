//! The only module allowed to remove files; moves items to the Trash with undo (M4).
//!
//! Only the protected-path list exists so far; it is shared with the Inspector, which shows a lock
//! for paths that can never be moved. M4 builds the move/undo machinery around it.

use std::path::{Component, Path};

/// Why `path` may never be moved to the Trash, or `None` if it is allowed.
///
/// Matching is case-insensitive (macOS volumes usually are) and component-wise, so
/// `/System-old` is not caught by `/System` but `/system/Library` is.
pub fn protected_reason(path: &Path) -> Option<&'static str> {
    let lower: Vec<String> = path
        .components()
        .filter_map(|c| match c {
            Component::Normal(s) => Some(s.to_string_lossy().to_lowercase()),
            _ => None,
        })
        .collect();
    let parts: Vec<&str> = lower.iter().map(String::as_str).collect();
    let starts = |prefix: &[&str]| parts.starts_with(prefix);

    if !path.is_absolute() || path.components().any(|c| c == Component::ParentDir) {
        return Some("Only absolute, normalized paths can be changed.");
    }
    if parts.is_empty() {
        return Some("The startup volume itself cannot be moved.");
    }
    if starts(&["system"]) {
        return Some("Part of macOS. It is protected and cannot be removed.");
    }
    if starts(&["usr"]) && !starts(&["usr", "local"]) {
        return Some("Part of macOS. It is protected and cannot be removed.");
    }
    if starts(&["bin"]) || starts(&["sbin"]) {
        return Some("Part of macOS. It is protected and cannot be removed.");
    }
    if starts(&["private", "var", "db"]) || starts(&["library", "apple"]) {
        return Some("Part of macOS. It is protected and cannot be removed.");
    }
    if parts.last() == Some(&".trash") || parts.contains(&".trash") {
        return Some("Already in the Trash.");
    }
    if starts(&["volumes"]) && parts.len() <= 2 {
        return Some("A mounted volume cannot be moved.");
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> Option<&'static str> {
        protected_reason(Path::new(s))
    }

    #[test]
    fn system_paths_are_protected() {
        for s in [
            "/System",
            "/System/Library/x",
            "/usr/bin/ls",
            "/bin/sh",
            "/sbin/mount",
            "/private/var/db/receipts",
            "/Library/Apple/foo",
            "/",
        ] {
            assert!(p(s).is_some(), "{s}");
        }
    }

    #[test]
    fn case_variants_and_traversal_are_caught() {
        assert!(p("/system/library").is_some());
        assert!(p("/SYSTEM").is_some());
        assert!(p("/Users/me/../../System").is_some());
        assert!(p("relative/path").is_some());
        assert!(p("/Users/me/.Trash/old.txt").is_some());
    }

    #[test]
    fn user_files_are_allowed() {
        for s in [
            "/Users/me/Downloads/a.dmg",
            "/usr/local/share/x",
            "/Applications/Foo.app",
            "/System-old/x",
            "/Volumes/Ext/stuff",
        ] {
            assert!(p(s).is_none(), "{s}");
        }
        assert!(p("/Volumes/Ext").is_some());
    }
}
