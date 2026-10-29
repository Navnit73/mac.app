//! Shared data types exchanged with the UI. TypeScript bindings are generated from these.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Identifier of a scan held in the managed state.
pub type ScanId = u32;

/// Response of the `ping` command, used to prove the IPC round trip.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct PingResponse {
    pub message: String,
    pub core_version: String,
}

/// Payload of the `app://tick` demo event.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct PingEvent {
    pub seq: u32,
}

/// Build the ping response.
pub fn ping(name: &str) -> PingResponse {
    PingResponse {
        message: format!("pong, {name}"),
        core_version: env!("CARGO_PKG_VERSION").to_string(),
    }
}

/// User-controllable scan settings.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct ScanOptions {
    /// Glob patterns matched against full paths; matches are skipped and reported.
    pub exclude_globs: Vec<String>,
    /// Hidden files and dot-folders are included by default.
    pub include_hidden: bool,
    /// Descend into other volumes (external drives, network shares).
    pub cross_volumes: bool,
    /// Worker thread count; `None` picks `min(cores, 8)`.
    pub workers: Option<u32>,
}

impl Default for ScanOptions {
    fn default() -> Self {
        Self {
            exclude_globs: Vec::new(),
            include_hidden: true,
            cross_volumes: false,
            workers: None,
        }
    }
}

/// Throttled progress snapshot (about 10 per second).
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct ScanProgress {
    #[ts(type = "number")]
    pub files: u64,
    #[ts(type = "number")]
    pub dirs: u64,
    /// Allocated bytes seen so far (before hard-link de-duplication).
    #[ts(type = "number")]
    pub bytes: u64,
    pub current_path: String,
    #[ts(type = "number")]
    pub elapsed_ms: u64,
    #[ts(type = "number")]
    pub files_per_sec: u64,
}

/// Final numbers of a (possibly cancelled) scan.
#[derive(Debug, Clone, Default, Serialize, TS)]
#[ts(export)]
pub struct ScanSummary {
    #[ts(type = "number")]
    pub files: u64,
    #[ts(type = "number")]
    pub dirs: u64,
    /// Allocated bytes, hard links counted once, dataless files excluded.
    #[ts(type = "number")]
    pub total_alloc: u64,
    #[ts(type = "number")]
    pub total_logical: u64,
    #[ts(type = "number")]
    pub skipped: u64,
    pub cancelled: bool,
    #[ts(type = "number")]
    pub elapsed_ms: u64,
}

/// Lifecycle of a scan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub enum ScanStatus {
    Running,
    Done,
    Cancelled,
    Failed,
}

/// Result of `get_scan_state`.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct ScanState {
    pub id: ScanId,
    pub status: ScanStatus,
    pub progress: Option<ScanProgress>,
    pub summary: Option<ScanSummary>,
}

/// Payload of `scan://progress`.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct ScanProgressEvent {
    pub id: ScanId,
    pub progress: ScanProgress,
}

/// Payload of `scan://done`.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct ScanDoneEvent {
    pub id: ScanId,
    pub status: ScanStatus,
    pub summary: Option<ScanSummary>,
    pub error: Option<String>,
}

/// Why a path is not part of the totals.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub enum SkipReason {
    PermissionDenied,
    Vanished,
    IoError,
    Excluded,
    Dataless,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct SkippedPath {
    pub path: String,
    pub reason: SkipReason,
}

/// One page of the scan report.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct ReportPage {
    pub items: Vec<SkippedPath>,
    #[ts(type = "number")]
    pub total: u64,
    /// Page index to request next, if any.
    pub next_page: Option<u32>,
}

// ---------------------------------------------------------------------------------------------
// M2: Overview, Space, Inspector
// ---------------------------------------------------------------------------------------------

/// Whether the app can read folders protected by Full Disk Access.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub enum FdaStatus {
    Granted,
    Denied,
    /// None of the probe folders exist, so nothing can be concluded.
    Unknown,
}

/// Capacity of a volume.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct VolumeInfo {
    pub name: String,
    #[ts(type = "number")]
    pub total: u64,
    /// What `df` reports as free.
    #[ts(type = "number")]
    pub free_raw: u64,
    /// Free space including what macOS can purge on demand (the headline "free").
    #[ts(type = "number")]
    pub available_important: u64,
    #[ts(type = "number")]
    pub purgeable: u64,
}

/// Coarse file classification used for colors, filters and smart groups.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export)]
pub enum FileKind {
    Folder,
    Documents,
    Images,
    Video,
    Audio,
    Archives,
    Apps,
    Code,
    DiskImages,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub enum EntryKind {
    File,
    Dir,
    Symlink,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub enum SortKey {
    Size,
    Name,
    Count,
    Modified,
}

/// One row in a folder listing or the large-files list.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct NodeRow {
    pub id: u32,
    pub name: String,
    pub kind: EntryKind,
    pub file_kind: FileKind,
    pub is_package: bool,
    pub is_hidden: bool,
    pub is_dataless: bool,
    pub is_unreadable: bool,
    /// Allocated bytes including everything below.
    #[ts(type = "number")]
    pub size: u64,
    #[ts(type = "number")]
    pub size_logical: u64,
    pub file_count: u32,
    #[ts(type = "number")]
    pub mtime: i64,
    /// Share of the parent's size, 0..=1.
    pub parent_fraction: f32,
    pub has_children: bool,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct ChildrenPage {
    pub items: Vec<NodeRow>,
    pub total: u32,
    pub next_cursor: Option<u32>,
    #[ts(type = "number")]
    pub parent_size: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub enum SmartGroup {
    OldInstallers,
    Archives,
    Videos,
    DiskImages,
    OldDownloads,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct TopFilters {
    #[ts(type = "number")]
    pub min_size: u64,
    pub kind: Option<FileKind>,
    /// Only files not modified for this many days.
    pub older_than_days: Option<u32>,
    /// Restrict to this folder (node id).
    pub scope: Option<u32>,
    pub group: Option<SmartGroup>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct TopFilesResult {
    pub items: Vec<NodeRow>,
    pub matched_count: u32,
    #[ts(type = "number")]
    pub matched_bytes: u64,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct SmartGroupSummary {
    pub group: SmartGroup,
    pub count: u32,
    #[ts(type = "number")]
    pub bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub enum ChartMode {
    Sunburst,
    Treemap,
}

/// How long ago a file was last modified.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub enum AgeBucket {
    /// Under 30 days.
    Recent,
    /// Under one year.
    ThisYear,
    /// One year or more.
    Old,
}

/// A node of the pre-aggregated chart tree. `id` is `None` for the "Other" bucket.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct ChartNode {
    pub id: Option<u32>,
    pub name: String,
    #[ts(type = "number")]
    pub value: u64,
    pub is_dir: bool,
    pub file_kind: FileKind,
    pub age: AgeBucket,
    /// Number of entries folded into an "Other" bucket.
    pub other_count: u32,
    pub children: Vec<ChartNode>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct Crumb {
    pub id: u32,
    pub name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub enum ICloudState {
    Local,
    /// Placeholder: the content lives in iCloud and is not counted.
    CloudOnly,
}

/// Everything the Inspector shows for one node.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct NodeDetails {
    pub id: u32,
    pub name: String,
    pub path: String,
    pub kind: EntryKind,
    pub file_kind: FileKind,
    pub is_package: bool,
    pub is_hidden: bool,
    /// False if the path vanished since the scan.
    pub exists: bool,
    #[ts(type = "number")]
    pub size_logical: u64,
    #[ts(type = "number")]
    pub size_alloc: u64,
    /// Allocated size including everything below.
    #[ts(type = "number")]
    pub total_alloc: u64,
    pub file_count: u32,
    #[ts(type = "number | null")]
    pub created: Option<i64>,
    #[ts(type = "number")]
    pub modified: i64,
    #[ts(type = "number | null")]
    pub last_used: Option<i64>,
    pub owner: Option<String>,
    pub permissions: Option<String>,
    pub hard_links: u32,
    pub tags: Vec<String>,
    pub where_froms: Vec<String>,
    pub icloud: ICloudState,
    /// Set when the path is on the protected list and cannot be moved to the Trash.
    pub protected_reason: Option<String>,
    /// Plain-language hint about what this is, when known.
    pub why: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, TS)]
#[ts(export)]
pub enum CategoryId {
    Developer,
    OldDownloads,
    Caches,
    Logs,
    Duplicates,
    Leftovers,
    LargeOld,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub enum Risk {
    Safe,
    Review,
    Careful,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct CategorySummary {
    pub id: CategoryId,
    pub name: String,
    pub blurb: String,
    #[ts(type = "number")]
    pub bytes: u64,
    pub items: u32,
    pub risk: Risk,
    /// False when the analysis for this category arrives in a later milestone.
    pub analyzed: bool,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct OverviewSummary {
    pub volume: VolumeInfo,
    pub roots: Vec<String>,
    /// True when the scan covers the whole volume; otherwise "other" is outside the scan.
    pub covers_volume: bool,
    /// Allocated bytes found by the scan.
    #[ts(type = "number")]
    pub scanned_bytes: u64,
    pub categories: Vec<CategorySummary>,
    /// Scanned bytes not in any clearable category.
    #[ts(type = "number")]
    pub kept_bytes: u64,
    /// Used space the scan did not account for (system, purgeable, snapshots, unscanned folders).
    #[ts(type = "number")]
    pub system_other_bytes: u64,
    /// `None` when the Trash cannot be read (needs Full Disk Access).
    #[ts(type = "number | null")]
    pub trash_bytes: Option<u64>,
    pub files: u64,
    pub dirs: u64,
    pub skipped: u64,
    #[ts(type = "number")]
    pub elapsed_ms: u64,
    pub largest_folder: Option<Crumb>,
    #[ts(type = "number")]
    pub largest_folder_bytes: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ping_greets() {
        let r = ping("ui");
        assert_eq!(r.message, "pong, ui");
        assert!(!r.core_version.is_empty());
    }

    #[test]
    fn options_default_includes_hidden() {
        let o: ScanOptions = serde_json::from_str("{}").unwrap();
        assert!(o.include_hidden);
        assert!(!o.cross_volumes);
    }
}
