//! The scan report: every path that is not part of the totals, with a reason.

use crate::models::{ReportPage, SkipReason, SkippedPath};

/// Fixed page size so the IPC payload stays small.
pub const REPORT_PAGE_SIZE: usize = 200;

#[derive(Debug, Default)]
pub struct ScanReport {
    skipped: Vec<SkippedPath>,
}

impl ScanReport {
    pub fn push(&mut self, path: String, reason: SkipReason) {
        self.skipped.push(SkippedPath { path, reason });
    }

    pub fn len(&self) -> usize {
        self.skipped.len()
    }

    pub fn is_empty(&self) -> bool {
        self.skipped.is_empty()
    }

    pub fn items(&self) -> &[SkippedPath] {
        &self.skipped
    }

    pub fn count(&self, reason: SkipReason) -> usize {
        self.skipped.iter().filter(|s| s.reason == reason).count()
    }

    /// Drop entries at or below `prefix` (used before a subtree rescan).
    pub fn remove_under(&mut self, prefix: &str) {
        // A prefix ending in `/` (the `/` root itself) already ends at a boundary.
        let at_boundary = prefix.ends_with('/');
        self.skipped.retain(|s| {
            !(s.path == prefix
                || s.path
                    .strip_prefix(prefix)
                    .is_some_and(|rest| at_boundary || rest.starts_with('/')))
        });
    }

    pub fn page(&self, page: u32) -> ReportPage {
        let start = (page as usize).saturating_mul(REPORT_PAGE_SIZE);
        let end = (start + REPORT_PAGE_SIZE).min(self.skipped.len());
        let items = self.skipped.get(start..end).unwrap_or(&[]).to_vec();
        ReportPage {
            items,
            total: self.skipped.len() as u64,
            next_page: (end < self.skipped.len()).then(|| page + 1),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pages_are_bounded() {
        let mut r = ScanReport::default();
        for i in 0..450 {
            r.push(format!("/p/{i}"), SkipReason::IoError);
        }
        let p0 = r.page(0);
        assert_eq!(p0.items.len(), REPORT_PAGE_SIZE);
        assert_eq!(p0.next_page, Some(1));
        let p2 = r.page(2);
        assert_eq!(p2.items.len(), 50);
        assert_eq!(p2.next_page, None);
        assert!(r.page(9).items.is_empty());
    }

    #[test]
    fn remove_under_respects_boundaries() {
        let mut r = ScanReport::default();
        r.push("/a/b".into(), SkipReason::Excluded);
        r.push("/a/bc".into(), SkipReason::Excluded);
        r.push("/a/b/c".into(), SkipReason::Excluded);
        r.remove_under("/a/b");
        assert_eq!(r.len(), 1);
        assert_eq!(r.items()[0].path, "/a/bc");

        let mut r = ScanReport::default();
        r.push("/a".into(), SkipReason::Excluded);
        r.remove_under("/");
        assert_eq!(r.len(), 0);
    }
}
