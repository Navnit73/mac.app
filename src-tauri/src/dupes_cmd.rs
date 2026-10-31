//! Duplicate scan, keep rules and verified removal. The selection lives here, in Rust, so the
//! "keep at least one copy" rule cannot be bypassed from the UI.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Instant;

use mymacstorage_core::duplicates::{self, candidates, find_duplicates, Session};
use mymacstorage_core::index::NodeId;
use mymacstorage_core::models::{
    DupDoneEvent, DupGroupsPage, DupOptions, DupSavings, DupStatus, FileKind, KeepRule, Room,
    ScanId, TrashBatchResult,
};
use mymacstorage_core::trash::{Context, MacTrash};
use mymacstorage_core::{platform, AppError};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::scan::ScanManager;
use crate::trash_cmd::{now, refresh, TrashState};

#[derive(Default)]
pub struct DupState {
    session: Mutex<Option<(ScanId, Session)>>,
    cancel: Arc<AtomicBool>,
    running: AtomicBool,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl DupState {
    fn with_session<T>(
        &self,
        scan_id: ScanId,
        f: impl FnOnce(&mut Session) -> Result<T, AppError>,
    ) -> Result<T, AppError> {
        let mut guard = lock(&self.session);
        match guard.as_mut() {
            Some((id, s)) if *id == scan_id => f(s),
            _ => Err(AppError::NotFound(
                "No duplicate results for this scan. Run the duplicate scan again.".into(),
            )),
        }
    }

    /// (potential bytes, removable copies) for the Overview card, if a duplicate scan ran.
    pub fn overview_totals(&self, scan_id: ScanId) -> Option<(u64, u32)> {
        let guard = lock(&self.session);
        let (id, s) = guard.as_ref()?;
        (*id == scan_id).then(|| {
            let copies = s.groups.iter().map(|g| g.files.len() as u32 - 1).sum();
            (s.potential_bytes(), copies)
        })
    }
}

#[tauri::command]
pub async fn start_duplicate_scan(
    app: AppHandle,
    scans: State<'_, ScanManager>,
    dups: State<'_, DupState>,
    scan_id: ScanId,
    options: DupOptions,
) -> Result<(), AppError> {
    if dups.running.swap(true, Ordering::SeqCst) {
        return Err(AppError::InvalidInput(
            "A duplicate scan is already running.".into(),
        ));
    }
    // Fail fast when the scan is unknown or unfinished; the tree walk itself runs on the thread.
    if let Err(e) = scans.with_output(scan_id, |_, _| Ok(())) {
        dups.running.store(false, Ordering::SeqCst);
        return Err(e);
    }
    dups.cancel.store(false, Ordering::SeqCst);
    *lock(&dups.session) = None;
    let cancel = dups.cancel.clone();

    std::thread::spawn(move || {
        let start = Instant::now();
        let progress_app = app.clone();
        let found = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let cands = progress_app
                .state::<ScanManager>()
                .with_output(scan_id, |out, _| Ok(candidates(&out.tree, &options)))?;
            Ok::<_, AppError>(find_duplicates(
                cands,
                &cancel,
                &|p| {
                    let _ = progress_app.emit("dupes://progress", p);
                },
                None,
            ))
        }));
        let state = app.state::<DupState>();
        let event = match found {
            Ok(Err(e)) => DupDoneEvent {
                summary: None,
                error: Some(e.to_string()),
            },
            Ok(Ok(found)) => {
                let mut session = Session::new(
                    found,
                    start.elapsed().as_millis() as u64,
                    &platform::private_size,
                );
                // Default: keep the oldest copy, usually the original.
                session.apply_rule(KeepRule::Oldest, None, None);
                let summary = session.summary.clone();
                *lock(&state.session) = Some((scan_id, session));
                DupDoneEvent {
                    summary: Some(summary),
                    error: None,
                }
            }
            Err(_) => DupDoneEvent {
                summary: None,
                error: Some("The duplicate scan stopped unexpectedly.".into()),
            },
        };
        state.running.store(false, Ordering::SeqCst);
        let _ = app.emit("dupes://done", event);
    });
    Ok(())
}

#[tauri::command]
pub async fn cancel_duplicate_scan(dups: State<'_, DupState>) -> Result<(), AppError> {
    dups.cancel.store(true, Ordering::SeqCst);
    Ok(())
}

#[tauri::command]
pub async fn get_duplicate_status(
    dups: State<'_, DupState>,
    scan_id: ScanId,
) -> Result<DupStatus, AppError> {
    let running = dups.running.load(Ordering::SeqCst);
    let guard = lock(&dups.session);
    Ok(match guard.as_ref() {
        Some((id, s)) if *id == scan_id => DupStatus {
            running,
            summary: Some(s.summary.clone()),
            savings: s.savings(),
            potential_bytes: s.potential_bytes(),
        },
        _ => DupStatus {
            running,
            summary: None,
            savings: DupSavings::default(),
            potential_bytes: 0,
        },
    })
}

/// One page of groups. For image groups the first copy (all copies are identical) is added to
/// the asset protocol scope so the UI can show a thumbnail; nothing else is readable there.
#[tauri::command]
pub async fn get_duplicate_groups(
    app: AppHandle,
    dups: State<'_, DupState>,
    scan_id: ScanId,
    page: u32,
    kind: Option<FileKind>,
) -> Result<DupGroupsPage, AppError> {
    let page = dups.with_session(scan_id, |s| Ok(s.page(page, kind)))?;
    let scope = app.asset_protocol_scope();
    for g in page
        .groups
        .iter()
        .filter(|g| g.file_kind == FileKind::Images)
    {
        if let Some(f) = g.files.first() {
            if let Err(e) = scope.allow_file(&f.path) {
                tracing::warn!("thumbnail scope: {e}");
            }
        }
    }
    Ok(page)
}

#[tauri::command]
pub async fn apply_keep_rule(
    scans: State<'_, ScanManager>,
    dups: State<'_, DupState>,
    scan_id: ScanId,
    rule: KeepRule,
    folder_node: Option<NodeId>,
    group_ids: Option<Vec<u32>>,
) -> Result<DupSavings, AppError> {
    let preferred: Option<PathBuf> = match folder_node {
        Some(n) => Some(scans.with_output(scan_id, |out, _| {
            // A file stands for the folder it is in.
            out.tree
                .node(n)
                .map(|node| out.tree.path(if node.is_dir() { n } else { node.parent }))
                .ok_or_else(|| AppError::NotFound(format!("node {n}")))
        })?),
        None => None,
    };
    dups.with_session(scan_id, |s| {
        s.apply_rule(rule, preferred.as_deref(), group_ids.as_deref());
        Ok(s.savings())
    })
}

#[tauri::command]
pub async fn set_duplicate_selected(
    dups: State<'_, DupState>,
    scan_id: ScanId,
    group_id: u32,
    node_id: NodeId,
    selected: bool,
) -> Result<DupSavings, AppError> {
    dups.with_session(scan_id, |s| {
        s.set_selected(group_id, node_id, selected)?;
        Ok(s.savings())
    })
}

#[tauri::command]
pub async fn duplicate_selection(
    dups: State<'_, DupState>,
    scan_id: ScanId,
) -> Result<Vec<NodeId>, AppError> {
    dups.with_session(scan_id, |s| Ok(s.selected_nodes()))
}

/// Compare every selected copy with the kept one, then move the matching ones to the Trash
/// (or, with `clones`, replace them with APFS clones; the originals still go to the Trash).
#[tauri::command]
pub async fn remove_duplicates(
    app: AppHandle,
    scan_id: ScanId,
    clones: bool,
) -> Result<TrashBatchResult, AppError> {
    // Re-hashing every selected copy can take minutes: keep it off the async runtime.
    let tm_app = app.clone();
    let result = crate::cleanup_cmd::blocking(app, move |app| {
        remove_blocking(
            &app.state::<ScanManager>(),
            &app.state::<DupState>(),
            &app.state::<TrashState>(),
            scan_id,
            clones,
        )
    })
    .await;
    if let Ok(r) = &result {
        crate::telemetry::track(
            &tm_app,
            "duplicates.deleted",
            serde_json::json!({ "count": r.moved_count, "bytes": r.moved_bytes, "clones": clones }),
        );
    }
    result
}

fn remove_blocking(
    scans: &ScanManager,
    dups: &DupState,
    trash_state: &TrashState,
    scan_id: ScanId,
    clones: bool,
) -> Result<TrashBatchResult, AppError> {
    let roots: Vec<PathBuf> = scans.with_output(scan_id, |_, roots| {
        Ok(roots.iter().map(PathBuf::from).collect())
    })?;
    let ctx = Context::new(roots);
    let (outcome, done) = dups.with_session(scan_id, |s| {
        let verified = s.verify(&ctx);
        let (outcome, done) = if clones {
            duplicates::replace_with_clones(s, verified, &ctx, &MacTrash)
        } else {
            duplicates::trash_verified(verified, &ctx, &MacTrash)
        };
        s.forget(&done.iter().copied().collect::<HashSet<_>>());
        Ok((outcome, done))
    })?;
    let result = trash_state.with_store(|st| st.record(Room::Duplicates, &outcome, now()))?;
    if !done.is_empty() {
        let moved: Vec<PathBuf> = outcome.moved.iter().map(|m| m.original.clone()).collect();
        let _ = scans.with_output_mut(scan_id, |out| {
            refresh(out, &moved);
            Ok(())
        });
    }
    Ok(result)
}
