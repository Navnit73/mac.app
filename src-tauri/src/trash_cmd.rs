//! Trash, Undo, Empty Trash and Activity commands. Every removal in the app goes through
//! `trash_items`, which checks each item again right before it moves (see `core::trash`).

use std::collections::HashSet;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use duobolt_core::activity::{self, Store};
use duobolt_core::index::{NodeId, Tree};
use duobolt_core::models::{
    BatchPage, BatchSummary, EmptyTrashPrep, Risk, Room, ScanId, TrashBatchResult, TrashPreview,
    TrashPreviewItem, UndoResult, WeeklyStats,
};
use duobolt_core::scanner::bulk::BulkReader;
use duobolt_core::scanner::{rescan_subtree, ScanOutput};
use duobolt_core::trash::{self, Candidate, Context, Expected, MacTrash, MAX_BATCH};
use duobolt_core::{overview, platform, space, AppError};
use tauri::State;

use crate::scan::ScanManager;

const TOKEN_TTL: Duration = Duration::from_secs(60);

#[derive(Default)]
pub struct TrashState {
    store: Mutex<Option<Store>>,
    token: Mutex<Option<(String, Instant)>>,
}

pub(crate) fn home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default()
}

pub(crate) fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

fn db_path() -> PathBuf {
    home().join("Library/Application Support/DuoBolt/activity.sqlite")
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl TrashState {
    /// Run `f` with the activity database, opening it on first use.
    pub(crate) fn with_store<T>(
        &self,
        f: impl FnOnce(&mut Store) -> Result<T, AppError>,
    ) -> Result<T, AppError> {
        let mut guard = lock(&self.store);
        if guard.is_none() {
            *guard = Some(Store::open(&db_path())?);
        }
        match guard.as_mut() {
            Some(store) => f(store),
            None => Err(AppError::Internal("activity database unavailable".into())),
        }
    }
}

/// On launch: mark items that left the Trash while DuoBolt was closed.
pub fn reconcile_on_launch(state: &TrashState) {
    if let Err(e) = state.with_store(|s| s.reconcile()) {
        tracing::warn!("activity reconcile failed: {e}");
    }
}

/// Resolve node ids to move candidates. A node inside another selected node is dropped (moving
/// the folder already moves it). Returns (candidate, display size, node id).
fn candidates(tree: &Tree, ids: &[NodeId]) -> Result<Vec<(Candidate, u64, NodeId)>, AppError> {
    if ids.len() > MAX_BATCH {
        return Err(AppError::InvalidInput(format!(
            "select at most {MAX_BATCH} items at a time"
        )));
    }
    let set: HashSet<NodeId> = ids.iter().copied().collect();
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for &id in ids {
        if !seen.insert(id) {
            continue;
        }
        let Some(n) = tree.node(id) else { continue };
        let mut parent = n.parent;
        let mut nested = false;
        while let Some(p) = tree.node(parent) {
            if set.contains(&parent) {
                nested = true;
                break;
            }
            parent = p.parent;
        }
        if nested {
            continue;
        }
        out.push((
            Candidate {
                path: tree.path(id),
                expected: Expected {
                    dev: n.dev,
                    inode: n.inode,
                    size: n.size_logical,
                    mtime: n.mtime,
                    is_dir: n.is_dir(),
                },
            },
            n.total_alloc,
            id,
        ));
    }
    Ok(out)
}

fn risk_for(path: &Path) -> Risk {
    match overview::describe(path, &home()) {
        Some(d)
            if d.starts_with("App cache") || d.starts_with("Developer") || d.starts_with("Log") =>
        {
            Risk::Safe
        }
        _ => Risk::Review,
    }
}

/// Refresh the scan below the folders that changed, so totals and search stay true.
pub(crate) fn refresh(out: &mut ScanOutput, paths: &[PathBuf]) {
    let mut ids: Vec<NodeId> = Vec::new();
    for p in paths {
        // Walk up until a folder the scan knows about (a restore may recreate folders).
        let mut cur = p.parent();
        while let Some(dir) = cur {
            if let Some(id) = out.tree.find_path(dir) {
                ids.push(id);
                break;
            }
            cur = dir.parent();
        }
    }
    ids.sort_unstable();
    ids.dedup();
    // Rescanning a folder covers its subfolders.
    let set: HashSet<NodeId> = ids.iter().copied().collect();
    ids.retain(|id| {
        let mut p = out.tree.node(*id).map(|n| n.parent);
        while let Some(pid) = p {
            if set.contains(&pid) {
                return false;
            }
            p = out.tree.node(pid).map(|n| n.parent);
        }
        true
    });
    let cancel = std::sync::atomic::AtomicBool::new(false);
    for id in ids {
        if let Err(e) = rescan_subtree(&BulkReader::new(), out, id, &cancel, &mut |_| {}) {
            tracing::warn!("rescan after trash failed: {e}");
        }
    }
}

#[tauri::command]
pub async fn preview_trash(
    scans: State<'_, ScanManager>,
    scan_id: ScanId,
    node_ids: Vec<NodeId>,
) -> Result<TrashPreview, AppError> {
    let (cands, roots) = scans.with_output(scan_id, |out, roots| {
        let items: Vec<_> = candidates(&out.tree, &node_ids)?
            .into_iter()
            .map(|(c, size, id)| {
                let name = space::display_name(&out.tree, id);
                let n = out.tree.node(id);
                let is_dir = n.is_some_and(|n| n.is_dir());
                let pkg = n.is_some_and(|n| n.has(duobolt_core::index::flags::PACKAGE));
                (
                    c,
                    size,
                    id,
                    name,
                    is_dir,
                    space::file_kind(out.tree.name(id), is_dir, pkg),
                )
            })
            .collect();
        Ok((items, roots.iter().map(PathBuf::from).collect::<Vec<_>>()))
    })?;
    tauri::async_runtime::spawn_blocking(move || {
        let ctx = Context::new(roots);
        let mut preview = TrashPreview {
            items: Vec::with_capacity(cands.len()),
            allowed_count: 0,
            allowed_bytes: 0,
            blocked_count: 0,
        };
        for (c, size, id, name, is_dir, kind) in cands {
            let blocked = trash::preflight(&c, &ctx).err();
            if blocked.is_some() {
                preview.blocked_count += 1;
            } else {
                preview.allowed_count += 1;
                preview.allowed_bytes += size;
            }
            preview.items.push(TrashPreviewItem {
                node_id: id,
                name,
                path: c.path.to_string_lossy().into_owned(),
                size,
                file_kind: kind,
                is_dir,
                risk: risk_for(&c.path),
                blocked,
            });
        }
        Ok(preview)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?
}

#[tauri::command]
pub async fn trash_items(
    scans: State<'_, ScanManager>,
    trash_state: State<'_, TrashState>,
    scan_id: ScanId,
    node_ids: Vec<NodeId>,
    room: Room,
) -> Result<TrashBatchResult, AppError> {
    let (cands, roots) = scans.with_output(scan_id, |out, roots| {
        let c: Vec<(Candidate, u64)> = candidates(&out.tree, &node_ids)?
            .into_iter()
            .map(|(c, size, _)| (c, size))
            .collect();
        Ok((c, roots.iter().map(PathBuf::from).collect::<Vec<_>>()))
    })?;
    let ctx = Context::new(roots);
    let outcome = trash::move_items(&cands, &ctx, &MacTrash);
    let result = trash_state.with_store(|s| s.record(room, &outcome, now()))?;
    let moved: Vec<PathBuf> = outcome.moved.iter().map(|m| m.original.clone()).collect();
    if !moved.is_empty() {
        let _ = scans.with_output_mut(scan_id, |out| {
            refresh(out, &moved);
            Ok(())
        });
    }
    Ok(result)
}

#[tauri::command]
pub async fn undo_batch(
    scans: State<'_, ScanManager>,
    trash_state: State<'_, TrashState>,
    batch_id: i64,
    item_ids: Option<Vec<i64>>,
    allow_rename: bool,
    scan_id: Option<ScanId>,
) -> Result<UndoResult, AppError> {
    let (result, paths) =
        trash_state.with_store(|s| s.undo(batch_id, item_ids.as_deref(), allow_rename))?;
    if let Some(id) = scan_id {
        if !paths.is_empty() {
            let _ = scans.with_output_mut(id, |out| {
                refresh(out, &paths);
                Ok(())
            });
        }
    }
    Ok(result)
}

/// ⌘Z: undo the most recent batch that still has items in the Trash.
#[tauri::command]
pub async fn undo_last(
    scans: State<'_, ScanManager>,
    trash_state: State<'_, TrashState>,
    scan_id: Option<ScanId>,
) -> Result<Option<UndoResult>, AppError> {
    let Some(batch) = trash_state.with_store(|s| s.last_undoable())? else {
        return Ok(None);
    };
    undo_batch(scans, trash_state, batch, None, false, scan_id)
        .await
        .map(Some)
}

fn random_token() -> Result<String, AppError> {
    let mut buf = [0u8; 16];
    std::fs::File::open("/dev/urandom")?.read_exact(&mut buf)?;
    Ok(buf.iter().map(|b| format!("{b:02x}")).collect())
}

#[tauri::command]
pub async fn prepare_empty_trash(
    trash_state: State<'_, TrashState>,
) -> Result<EmptyTrashPrep, AppError> {
    let (items, bytes) = trash_state.with_store(|s| s.in_trash_totals())?;
    let token = random_token()?;
    *lock(&trash_state.token) = Some((token.clone(), Instant::now()));
    Ok(EmptyTrashPrep {
        token,
        duobolt_items: items,
        duobolt_bytes: bytes,
        trash_bytes: trash::user_trash_bytes(),
    })
}

/// The only permanent deletion in the app. Requires the token from `prepare_empty_trash`.
#[tauri::command]
pub async fn empty_trash(
    scans: State<'_, ScanManager>,
    trash_state: State<'_, TrashState>,
    token: String,
    scan_id: Option<ScanId>,
) -> Result<u32, AppError> {
    let valid = {
        let mut guard = lock(&trash_state.token);
        let ok = guard
            .as_ref()
            .is_some_and(|(t, at)| *t == token && at.elapsed() < TOKEN_TTL);
        // A token works once, whatever the outcome.
        *guard = None;
        ok
    };
    if !valid {
        return Err(AppError::InvalidInput(
            "Confirmation expired. Open Empty Trash again.".into(),
        ));
    }
    tauri::async_runtime::spawn_blocking(trash::empty_trash_via_finder)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))??;
    let emptied = trash_state.with_store(|s| s.mark_all_emptied())?;
    if let Some(id) = scan_id {
        let trash_dir = home().join(".Trash");
        let _ = scans.with_output_mut(id, |out| {
            refresh(out, &[trash_dir.join("x")]);
            Ok(())
        });
    }
    Ok(emptied)
}

#[tauri::command]
pub async fn list_batches(
    trash_state: State<'_, TrashState>,
    limit: u32,
    offset: u32,
) -> Result<Vec<BatchSummary>, AppError> {
    trash_state.with_store(|s| {
        s.reconcile()?;
        s.list_batches(limit, offset)
    })
}

#[tauri::command]
pub async fn get_batch(
    trash_state: State<'_, TrashState>,
    batch_id: i64,
    page: u32,
) -> Result<BatchPage, AppError> {
    trash_state.with_store(|s| s.get_batch(batch_id, page))
}

#[tauri::command]
pub async fn get_weekly_stats(
    trash_state: State<'_, TrashState>,
    weeks: u32,
) -> Result<WeeklyStats, AppError> {
    trash_state.with_store(|s| s.weekly_stats(weeks, now()))
}

/// Writes a CSV into Downloads (never overwriting) and reveals it. Returns the file path.
#[tauri::command]
pub async fn export_activity_csv(trash_state: State<'_, TrashState>) -> Result<String, AppError> {
    let csv = trash_state.with_store(|s| s.csv())?;
    let date = activity::iso_utc(now());
    let stem = format!("DuoBolt activity {}", date.get(..10).unwrap_or("export"));
    let path = activity::write_new_file(&home().join("Downloads"), &stem, &csv)?;
    let _ = platform::reveal_in_finder(&path);
    Ok(path.to_string_lossy().into_owned())
}

#[tauri::command]
pub async fn clear_activity(trash_state: State<'_, TrashState>) -> Result<(), AppError> {
    trash_state.with_store(|s| s.clear())
}
