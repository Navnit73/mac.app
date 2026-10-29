//! Scan state and commands. Heavy work runs on a dedicated thread; commands stay small.

use std::collections::HashMap;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};

use duobolt_core::find::{self, SearchIndex};
use duobolt_core::index::NodeId;
use duobolt_core::models::{
    ReportPage, ScanDoneEvent, ScanId, ScanOptions, ScanProgress, ScanProgressEvent, ScanState,
    ScanStatus, ScanSummary,
};
use duobolt_core::models::{SearchQuery, SearchResult};
use duobolt_core::scanner::bulk::BulkReader;
use duobolt_core::scanner::{rescan_subtree as core_rescan, scan, ScanOutput};
use duobolt_core::AppError;
use tauri::{AppHandle, Emitter, State};

const MAX_ROOTS: usize = 16;

fn read<T>(l: &RwLock<T>) -> RwLockReadGuard<'_, T> {
    l.read().unwrap_or_else(|e| e.into_inner())
}

fn write<T>(l: &RwLock<T>) -> RwLockWriteGuard<'_, T> {
    l.write().unwrap_or_else(|e| e.into_inner())
}

struct Slot {
    status: ScanStatus,
    progress: Option<ScanProgress>,
    summary: Option<ScanSummary>,
    output: Option<ScanOutput>,
    /// Name index for Find, built on first search and dropped whenever the tree changes.
    index: Option<Arc<SearchIndex>>,
    /// Ids of the last search, so paging does not re-run it.
    last_search: Option<CachedSearch>,
}

struct CachedSearch {
    key: String,
    ids: Arc<Vec<NodeId>>,
    total: u32,
}

pub(crate) struct ScanHandle {
    pub(crate) roots: Vec<String>,
    cancel: AtomicBool,
    slot: RwLock<Slot>,
}

/// Managed state: all scans by id. Only one scan runs at a time and only the latest
/// finished scan is kept, to bound memory.
#[derive(Default)]
pub struct ScanManager {
    next: AtomicU32,
    scans: RwLock<HashMap<ScanId, Arc<ScanHandle>>>,
}

impl ScanManager {
    /// Run `f` on the finished scan's data under a read lock.
    pub(crate) fn with_output<T>(
        &self,
        id: ScanId,
        f: impl FnOnce(&ScanOutput, &[String]) -> Result<T, AppError>,
    ) -> Result<T, AppError> {
        let h = self.get(id)?;
        let slot = read(&h.slot);
        match &slot.output {
            Some(out) => f(out, &h.roots),
            None => Err(AppError::NotFound("scan has no data yet".into())),
        }
    }

    fn get(&self, id: ScanId) -> Result<Arc<ScanHandle>, AppError> {
        read(&self.scans)
            .get(&id)
            .cloned()
            .ok_or_else(|| AppError::NotFound(format!("scan {id}")))
    }
}

fn validate_roots(roots: &[String]) -> Result<Vec<PathBuf>, AppError> {
    if roots.is_empty() || roots.len() > MAX_ROOTS {
        return Err(AppError::InvalidInput(format!(
            "expected 1 to {MAX_ROOTS} roots"
        )));
    }
    roots
        .iter()
        .map(|r| {
            let p = PathBuf::from(r);
            if !p.is_absolute() {
                return Err(AppError::InvalidInput(format!("not an absolute path: {r}")));
            }
            let canon = std::fs::canonicalize(&p)?;
            if !canon.is_dir() {
                return Err(AppError::InvalidInput(format!("not a directory: {r}")));
            }
            Ok(canon)
        })
        .collect()
}

#[tauri::command]
pub async fn start_scan(
    app: AppHandle,
    state: State<'_, ScanManager>,
    roots: Vec<String>,
    options: Option<ScanOptions>,
) -> Result<ScanId, AppError> {
    let roots = validate_roots(&roots)?;
    let mut options = options.unwrap_or_default();
    options
        .exclude_globs
        .extend(crate::settings::load().exclusion_globs());

    let id = {
        let mut scans = write(&state.scans);
        if scans
            .values()
            .any(|h| read(&h.slot).status == ScanStatus::Running)
        {
            return Err(AppError::InvalidInput("a scan is already running".into()));
        }
        scans.clear();
        let id = state.next.fetch_add(1, Ordering::Relaxed) + 1;
        let handle = Arc::new(ScanHandle {
            roots: roots
                .iter()
                .map(|r| r.to_string_lossy().into_owned())
                .collect(),
            cancel: AtomicBool::new(false),
            slot: RwLock::new(Slot {
                status: ScanStatus::Running,
                progress: None,
                summary: None,
                output: None,
                index: None,
                last_search: None,
            }),
        });
        scans.insert(id, handle.clone());
        (id, handle)
    };
    let (id, handle) = id;

    std::thread::spawn(move || {
        let progress_app = app.clone();
        let progress_handle = handle.clone();
        let result = catch_unwind(AssertUnwindSafe(|| {
            scan(
                &BulkReader::new(),
                &roots,
                &options,
                &handle.cancel,
                &mut |p| {
                    write(&progress_handle.slot).progress = Some(p.clone());
                    let _ =
                        progress_app.emit("scan://progress", ScanProgressEvent { id, progress: p });
                },
            )
        }));

        let done = match result {
            Ok(Ok(out)) => {
                let status = if out.summary.cancelled {
                    ScanStatus::Cancelled
                } else {
                    ScanStatus::Done
                };
                let summary = out.summary.clone();
                let mut slot = write(&handle.slot);
                slot.status = status.clone();
                slot.summary = Some(summary.clone());
                slot.output = Some(out);
                ScanDoneEvent {
                    id,
                    status,
                    summary: Some(summary),
                    error: None,
                }
            }
            Ok(Err(e)) => fail(&handle, id, e.to_string()),
            Err(_) => fail(&handle, id, "scan thread panicked".into()),
        };
        let _ = app.emit("scan://done", done);
    });
    Ok(id)
}

fn fail(handle: &ScanHandle, id: ScanId, msg: String) -> ScanDoneEvent {
    tracing::error!("scan {id} failed: {msg}");
    write(&handle.slot).status = ScanStatus::Failed;
    ScanDoneEvent {
        id,
        status: ScanStatus::Failed,
        summary: None,
        error: Some(msg),
    }
}

#[tauri::command]
pub async fn cancel_scan(state: State<'_, ScanManager>, id: ScanId) -> Result<(), AppError> {
    state.get(id)?.cancel.store(true, Ordering::Relaxed);
    Ok(())
}

#[tauri::command]
pub async fn get_scan_state(
    state: State<'_, ScanManager>,
    id: ScanId,
) -> Result<ScanState, AppError> {
    let h = state.get(id)?;
    let slot = read(&h.slot);
    Ok(ScanState {
        id,
        status: slot.status.clone(),
        progress: slot.progress.clone(),
        summary: slot.summary.clone(),
    })
}

#[tauri::command]
pub async fn get_scan_report(
    state: State<'_, ScanManager>,
    id: ScanId,
    page: u32,
) -> Result<ReportPage, AppError> {
    let h = state.get(id)?;
    let slot = read(&h.slot);
    match &slot.output {
        Some(out) => Ok(out.report.page(page)),
        None => Err(AppError::NotFound("scan has no report yet".into())),
    }
}

#[tauri::command]
pub async fn rescan_subtree(
    state: State<'_, ScanManager>,
    id: ScanId,
    node_id: NodeId,
) -> Result<ScanSummary, AppError> {
    let h = state.get(id)?;
    tauri::async_runtime::spawn_blocking(move || {
        let mut slot = write(&h.slot);
        if slot.status == ScanStatus::Running {
            return Err(AppError::InvalidInput("scan still running".into()));
        }
        let Some(out) = slot.output.as_mut() else {
            return Err(AppError::NotFound("scan has no data".into()));
        };
        let cancel = AtomicBool::new(false);
        core_rescan(&BulkReader::new(), out, node_id, &cancel, &mut |_| {})?;
        let summary = out.summary.clone();
        // The tree changed: the search index and cached results are stale.
        slot.index = None;
        slot.last_search = None;
        slot.summary = Some(summary.clone());
        Ok(summary)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?
}

/// Search the scan's names. Builds the index on first use; repeats of the same query (paging)
/// reuse the cached id list.
#[tauri::command]
pub async fn search(
    state: State<'_, ScanManager>,
    id: ScanId,
    query: SearchQuery,
    cursor: u32,
) -> Result<SearchResult, AppError> {
    if query.text.len() > 1024 {
        return Err(AppError::InvalidInput("query too long".into()));
    }
    let h = state.get(id)?;
    tauri::async_runtime::spawn_blocking(move || {
        let key = serde_json::to_string(&query).map_err(|e| AppError::Internal(e.to_string()))?;

        // Fast path: same query as last time, only a new page.
        {
            let slot = read(&h.slot);
            if let (Some(out), Some(c)) = (&slot.output, &slot.last_search) {
                if c.key == key {
                    return Ok(find::page(&out.tree, &c.ids, c.total, cursor));
                }
            }
        }

        let mut slot = write(&h.slot);
        let index = match &slot.index {
            Some(i) => i.clone(),
            None => {
                let Some(out) = &slot.output else {
                    return Err(AppError::NotFound("scan has no data yet".into()));
                };
                let built = Arc::new(SearchIndex::build(&out.tree));
                slot.index = Some(built.clone());
                built
            }
        };
        let Some(out) = &slot.output else {
            return Err(AppError::NotFound("scan has no data yet".into()));
        };
        match find::search_ids(&out.tree, &index, &query) {
            Ok((ids, total)) => {
                let result = find::page(&out.tree, &ids, total, cursor);
                slot.last_search = Some(CachedSearch {
                    key,
                    ids: Arc::new(ids),
                    total,
                });
                Ok(result)
            }
            Err(e) => Ok(SearchResult {
                items: Vec::new(),
                total: 0,
                next_cursor: None,
                truncated: false,
                pattern_error: Some(e),
            }),
        }
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?
}
