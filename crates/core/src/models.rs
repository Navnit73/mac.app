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

/// The app macOS checks for Full Disk Access: MyMacStorage itself when it runs as an app, or the app
/// that started it (Terminal, an editor) while developing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct FdaTarget {
    /// Display name, like "MyMacStorage" or "Visual Studio Code".
    pub name: String,
    /// The `.app` bundle to add in System Settings.
    pub path: String,
    /// False when another app hosts MyMacStorage, so access must be given to that app.
    pub is_self: bool,
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
    /// Folder containing this entry; only filled for the large-files list.
    pub location: Option<String>,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export)]
pub enum CategoryId {
    Developer,
    OldDownloads,
    Caches,
    Logs,
    Mail,
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
    #[ts(type = "number")]
    pub files: u64,
    #[ts(type = "number")]
    pub dirs: u64,
    #[ts(type = "number")]
    pub skipped: u64,
    #[ts(type = "number")]
    pub elapsed_ms: u64,
    pub largest_folder: Option<Crumb>,
    #[ts(type = "number")]
    pub largest_folder_bytes: u64,
}

// ---------------------------------------------------------------------------------------------
// M3: Find
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub enum SearchMode {
    /// Name contains the text (default).
    #[default]
    Substring,
    Prefix,
    Exact,
    Glob,
    Regex,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub enum HiddenMode {
    #[default]
    Include,
    Exclude,
    Only,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub enum SearchSort {
    #[default]
    Relevance,
    Name,
    Size,
    Modified,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct SearchFilters {
    pub kind: Option<FileKind>,
    #[ts(type = "number | null")]
    pub min_size: Option<u64>,
    #[ts(type = "number | null")]
    pub max_size: Option<u64>,
    /// Unix seconds; only items modified at or after this time.
    #[ts(type = "number | null")]
    pub modified_after: Option<i64>,
    /// Unix seconds; only items modified before this time.
    #[ts(type = "number | null")]
    pub modified_before: Option<i64>,
    pub hidden: HiddenMode,
    /// Restrict to this folder (node id).
    pub scope: Option<u32>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct SearchQuery {
    /// Empty text matches everything (useful with filters).
    pub text: String,
    pub mode: SearchMode,
    pub filters: SearchFilters,
    pub sort: SearchSort,
    /// Reverse the sort's natural order (relevance and name: ascending; size and modified: largest or newest first).
    pub descending: bool,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct SearchResult {
    pub items: Vec<NodeRow>,
    /// Number of matches found (can exceed what is listed).
    pub total: u32,
    pub next_cursor: Option<u32>,
    /// True when more than the listing cap matched; the UI asks the user to refine.
    pub truncated: bool,
    /// Set for an invalid glob or regex; the list is then empty.
    pub pattern_error: Option<String>,
}

// ---------------------------------------------------------------------------------------------
// M4: Safe Trash + Activity
// ---------------------------------------------------------------------------------------------

/// Where a removal started; recorded with every batch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub enum Room {
    Space,
    Find,
    Inspector,
    Cleanup,
    Duplicates,
    Applications,
}

impl Room {
    pub fn as_str(&self) -> &'static str {
        match self {
            Room::Space => "Space",
            Room::Find => "Find",
            Room::Inspector => "Inspector",
            Room::Cleanup => "Cleanup",
            Room::Duplicates => "Duplicates",
            Room::Applications => "Applications",
        }
    }

    pub fn parse(s: &str) -> Room {
        match s {
            "Space" => Room::Space,
            "Find" => Room::Find,
            "Cleanup" => Room::Cleanup,
            "Duplicates" => Room::Duplicates,
            "Applications" => Room::Applications,
            _ => Room::Inspector,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub enum ItemStatus {
    InTrash,
    Restored,
    /// Gone from the Trash (emptied by the user or by MyMacStorage).
    Emptied,
    Failed,
    Skipped,
    /// Something now exists at the original path; still in the Trash.
    Conflict,
}

impl ItemStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            ItemStatus::InTrash => "in_trash",
            ItemStatus::Restored => "restored",
            ItemStatus::Emptied => "emptied",
            ItemStatus::Failed => "failed",
            ItemStatus::Skipped => "skipped",
            ItemStatus::Conflict => "conflict",
        }
    }

    pub fn parse(s: &str) -> ItemStatus {
        match s {
            "in_trash" => ItemStatus::InTrash,
            "restored" => ItemStatus::Restored,
            "emptied" => ItemStatus::Emptied,
            "skipped" => ItemStatus::Skipped,
            "conflict" => ItemStatus::Conflict,
            _ => ItemStatus::Failed,
        }
    }
}

/// One row of the Review sheet.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct TrashPreviewItem {
    pub node_id: u32,
    pub name: String,
    pub path: String,
    #[ts(type = "number")]
    pub size: u64,
    pub file_kind: FileKind,
    pub is_dir: bool,
    pub risk: Risk,
    /// Why this item cannot be moved; `None` when it can.
    pub blocked: Option<String>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct TrashPreview {
    pub items: Vec<TrashPreviewItem>,
    pub allowed_count: u32,
    #[ts(type = "number")]
    pub allowed_bytes: u64,
    pub blocked_count: u32,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct SkippedItem {
    pub path: String,
    pub reason: String,
}

/// Outcome of moving a selection to the Trash.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct TrashBatchResult {
    /// `None` when nothing could be moved (no batch recorded).
    #[ts(type = "number | null")]
    pub batch_id: Option<i64>,
    pub moved_count: u32,
    #[ts(type = "number")]
    pub moved_bytes: u64,
    pub skipped: Vec<SkippedItem>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct BatchSummary {
    #[ts(type = "number")]
    pub id: i64,
    #[ts(type = "number")]
    pub created_at: i64,
    pub room: Room,
    pub item_count: u32,
    /// Bytes still counted as recovered: items in the Trash or emptied (restored ones excluded).
    #[ts(type = "number")]
    pub bytes: u64,
    pub in_trash: u32,
    pub restored: u32,
    pub emptied: u32,
    pub conflicts: u32,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct BatchItem {
    #[ts(type = "number")]
    pub id: i64,
    pub original_path: String,
    pub trashed_path: Option<String>,
    #[ts(type = "number")]
    pub size: u64,
    pub status: ItemStatus,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct BatchPage {
    pub batch: BatchSummary,
    pub items: Vec<BatchItem>,
    pub total: u32,
    pub next_page: Option<u32>,
}

#[derive(Debug, Clone, Default, Serialize, TS)]
#[ts(export)]
pub struct UndoResult {
    pub restored: u32,
    /// Restored under a "(restored)" name because the original path was taken.
    pub renamed: u32,
    pub conflicts: u32,
    /// No longer in the Trash (it was emptied).
    pub missing: u32,
    pub failed: Vec<SkippedItem>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct EmptyTrashPrep {
    /// Pass back to `empty_trash` within 60 seconds to confirm.
    pub token: String,
    pub mymacstorage_items: u32,
    #[ts(type = "number")]
    pub mymacstorage_bytes: u64,
    /// Size of the whole Trash, when it can be read (needs Full Disk Access).
    #[ts(type = "number | null")]
    pub trash_bytes: Option<u64>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct WeekStat {
    /// Unix seconds of Monday 00:00 UTC.
    #[ts(type = "number")]
    pub week_start: i64,
    #[ts(type = "number")]
    pub bytes: u64,
    pub batches: u32,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct WeeklyStats {
    pub weeks: Vec<WeekStat>,
    #[ts(type = "number")]
    pub total_bytes: u64,
    pub total_batches: u32,
    #[ts(type = "number")]
    pub biggest_bytes: u64,
    #[ts(type = "number")]
    pub this_week_bytes: u64,
}

// ---------------------------------------------------------------------------------------------
// M5: Duplicates
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct DupOptions {
    /// Files smaller than this are ignored (default 1 MB).
    #[ts(type = "number")]
    pub min_size: u64,
    /// Only these kinds; empty means all.
    pub kinds: Vec<FileKind>,
    /// Only inside this folder (node id).
    pub scope: Option<u32>,
    /// Also look inside Photos and Music libraries. Editing them can break those apps.
    pub include_media_libraries: bool,
    /// Also look in `~/Library` and hidden folders, where apps keep their own data.
    pub include_app_data: bool,
}

impl Default for DupOptions {
    fn default() -> Self {
        Self {
            min_size: 1_000_000,
            kinds: Vec::new(),
            scope: None,
            include_media_libraries: false,
            include_app_data: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub enum DupStage {
    Grouping,
    QuickCheck,
    Comparing,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct DupProgress {
    pub stage: DupStage,
    pub files_done: u32,
    pub files_total: u32,
    #[ts(type = "number")]
    pub bytes_done: u64,
    #[ts(type = "number")]
    pub bytes_total: u64,
    #[ts(type = "number | null")]
    pub eta_ms: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub enum KeepRule {
    Newest,
    Oldest,
    ShortestPath,
    /// Keep the copy inside a chosen folder; groups without one are left as they are.
    PreferredFolder,
    /// Clear the selection so the user picks by hand.
    Manual,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct DupFile {
    pub node_id: u32,
    pub name: String,
    pub path: String,
    #[ts(type = "number")]
    pub mtime: i64,
    /// Bytes freed if this copy goes away (APFS private size; shared clone blocks excluded).
    #[ts(type = "number")]
    pub private_size: u64,
    pub selected: bool,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct DupGroup {
    pub id: u32,
    #[ts(type = "number")]
    pub size: u64,
    pub file_kind: FileKind,
    pub name: String,
    /// Bytes freed by the current selection in this group.
    #[ts(type = "number")]
    pub reclaimable: u64,
    pub files: Vec<DupFile>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct DupGroupsPage {
    pub groups: Vec<DupGroup>,
    pub total: u32,
    pub next_page: Option<u32>,
}

#[derive(Debug, Clone, Default, Serialize, TS)]
#[ts(export)]
pub struct DupSavings {
    #[ts(type = "number")]
    pub bytes: u64,
    pub files: u32,
    pub groups: u32,
}

#[derive(Debug, Clone, Default, Serialize, TS)]
#[ts(export)]
pub struct DupSummary {
    pub groups: u32,
    pub files: u32,
    /// Most that could be freed: every copy but one in every group.
    #[ts(type = "number")]
    pub potential_bytes: u64,
    /// Files dropped because they changed while being read, or could not be read.
    pub skipped: Vec<SkippedItem>,
    #[ts(type = "number")]
    pub elapsed_ms: u64,
    pub cancelled: bool,
}

/// Where the duplicate scan for a given file scan stands.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct DupStatus {
    pub running: bool,
    pub summary: Option<DupSummary>,
    pub savings: DupSavings,
    /// What could still be freed (every copy but one), after removals.
    #[ts(type = "number")]
    pub potential_bytes: u64,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct DupDoneEvent {
    pub summary: Option<DupSummary>,
    pub error: Option<String>,
}

// ---------------------------------------------------------------------------------------------
// M6: Cleanup
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct CleanupCategory {
    pub id: CategoryId,
    pub name: String,
    /// What this is and whether it comes back.
    pub blurb: String,
    pub risk: Risk,
    /// Everything the rules found (report-only items excluded).
    #[ts(type = "number")]
    pub bytes: u64,
    pub items: u32,
    #[ts(type = "number")]
    pub selected_bytes: u64,
    pub selected_items: u32,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct CleanupItem {
    pub node_id: u32,
    pub name: String,
    pub path: String,
    pub category: CategoryId,
    pub rule_id: String,
    pub rule_name: String,
    pub risk: Risk,
    /// Apps or tools recreate it when needed.
    pub regenerates: bool,
    #[ts(type = "number")]
    pub bytes: u64,
    pub files: u32,
    #[ts(type = "number")]
    pub mtime: i64,
    pub is_dir: bool,
    /// Name of the running app that uses this, if any. Such items are not selected by default.
    pub running_app: Option<String>,
    /// Shown but never removed by MyMacStorage; `guidance` says what to do instead.
    pub report_only: bool,
    pub guidance: Option<String>,
    pub selected: bool,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct CleanupItemsPage {
    pub items: Vec<CleanupItem>,
    pub total: u32,
    pub next_page: Option<u32>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct CleanupSummary {
    pub categories: Vec<CleanupCategory>,
    #[ts(type = "number")]
    pub selected_bytes: u64,
    pub selected_items: u32,
    /// Apps that are open and own selected items; offered "Quit first" in the review.
    pub running_selected: Vec<String>,
    /// Bytes in the user's Trash, if readable (shown as information with Empty Trash).
    #[ts(type = "number | null")]
    pub trash_bytes: Option<u64>,
}

/// What the Review sheet shows before a cleanup runs.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct CleanupPlan {
    pub node_ids: Vec<u32>,
    /// Open apps owning selected items: offered "Quit them first".
    pub running_apps: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub enum SelectMode {
    /// Every item that may be batch-selected (not careful, not report-only).
    All,
    None,
    /// Back to the defaults: safe items of apps that are not running.
    Defaults,
}

// ---------------------------------------------------------------------------------------------
// M6: Applications
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub enum AppSource {
    AppStore,
    /// Part of macOS: shown, never removable.
    System,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub enum AppArch {
    AppleSilicon,
    Intel,
    Universal,
    Unknown,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct AppSummary {
    pub id: u32,
    pub name: String,
    pub bundle_id: String,
    pub version: String,
    pub path: String,
    pub source: AppSource,
    pub arch: AppArch,
    /// Size of the .app bundle.
    #[ts(type = "number")]
    pub app_bytes: u64,
    /// Settings, data and caches found for it (shared items left out).
    #[ts(type = "number")]
    pub data_bytes: u64,
    #[ts(type = "number")]
    pub cache_bytes: u64,
    #[ts(type = "number | null")]
    pub last_used: Option<i64>,
    pub removable: bool,
    /// Why it cannot be removed, when it cannot.
    pub locked_reason: Option<String>,
    pub running: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub enum RelatedKind {
    Support,
    Caches,
    Preferences,
    Containers,
    GroupContainers,
    SavedState,
    Logs,
    LaunchAgents,
    HttpStorage,
    WebKit,
    /// `/Library/Application Support`: shared by all users, review first.
    SystemSupport,
}

/// How sure MyMacStorage is that a file belongs to the app.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub enum Confidence {
    /// Named exactly after the bundle id.
    Exact,
    /// Name starts with the bundle id (for example a helper).
    Prefix,
    /// Named after the app's display name.
    Name,
    /// Used by several apps of the same developer.
    Shared,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct RelatedFile {
    pub node_id: u32,
    pub name: String,
    pub path: String,
    pub kind: RelatedKind,
    pub confidence: Confidence,
    #[ts(type = "number")]
    pub bytes: u64,
    /// Selected for removal by default.
    pub suggested: bool,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct AppDetails {
    pub app: AppSummary,
    pub app_node: Option<u32>,
    pub related: Vec<RelatedFile>,
    pub signed_by: Option<String>,
    /// Installer receipts in /private/var/db/receipts (information only).
    pub receipts: Vec<String>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct AppsList {
    pub apps: Vec<AppSummary>,
    /// Leftovers of removed apps found in the Library folders (bytes, items).
    #[ts(type = "number")]
    pub leftover_bytes: u64,
    pub leftover_items: u32,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct AppsDoneEvent {
    pub count: u32,
    pub error: Option<String>,
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
