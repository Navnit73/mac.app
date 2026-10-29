//! Cleanup commands. The analysis and the user's selection live here, in Rust, keyed by the scan
//! and its tree version; Overview reads the same analysis so both screens show the same totals.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use duobolt_core::cleanup::{analyze, Analysis, Rules, Session};
use duobolt_core::index::NodeId;
use duobolt_core::models::{
    CategoryId, CleanupItemsPage, CleanupPlan, CleanupSummary, Risk, Room, ScanId, SelectMode,
    TrashBatchResult,
};
use duobolt_core::trash::{Context, MacTrash, MAX_BATCH};
use duobolt_core::{apps, platform, AppError};
use tauri::{AppHandle, Manager};

use crate::scan::ScanManager;
use crate::trash_cmd::{home, now, refresh, TrashState};

/// How long apps get to quit when the user asks DuoBolt to quit them first.
const QUIT_WAIT: Duration = Duration::from_secs(10);

struct Cached {
    scan: ScanId,
    version: u64,
    rules: Rules,
    session: Session,
}

#[derive(Default)]
pub struct CleanupState {
    inner: Mutex<Option<Cached>>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl CleanupState {
    /// Make sure the analysis matches the scan's current tree. `force` re-reads running and
    /// installed apps even when the tree did not change. The user's choices are kept.
    fn ensure(&self, scans: &ScanManager, scan_id: ScanId, force: bool) -> Result<(), AppError> {
        let version = scans.version(scan_id)?;
        let mut guard = lock(&self.inner);
        if !force
            && guard
                .as_ref()
                .is_some_and(|c| c.scan == scan_id && c.version == version)
        {
            return Ok(());
        }
        let home = home();
        let rules = Rules::bundled(&home)?;
        let running = platform::running_apps();
        // Apps outside the usual folders count as installed too, so their data is never offered
        // as a leftover.
        let mut installed = apps::installed_ids(&apps::discover(&apps::app_dirs(&home)), &running);
        installed.extend(
            platform::spotlight_app_ids()
                .into_iter()
                .map(|id| id.to_lowercase()),
        );
        let analysis: Analysis = scans.with_output(scan_id, |out, _| {
            Ok(analyze(
                &out.tree,
                &rules,
                &home,
                now(),
                &running,
                &installed,
            ))
        })?;
        let previous = guard
            .as_ref()
            .filter(|c| c.scan == scan_id)
            .map(|c| &c.session);
        let session = Session::new(analysis, &rules, previous);
        *guard = Some(Cached {
            scan: scan_id,
            version,
            rules,
            session,
        });
        Ok(())
    }

    fn with<T>(
        &self,
        scans: &ScanManager,
        scan_id: ScanId,
        f: impl FnOnce(&Rules, &mut Session) -> Result<T, AppError>,
    ) -> Result<T, AppError> {
        self.ensure(scans, scan_id, false)?;
        let mut guard = lock(&self.inner);
        match guard.as_mut() {
            Some(c) if c.scan == scan_id => f(&c.rules, &mut c.session),
            _ => Err(AppError::NotFound(
                "no cleanup analysis for this scan".into(),
            )),
        }
    }

    /// Run `f` with the up-to-date analysis (Overview).
    pub fn with_analysis<T>(
        &self,
        scans: &ScanManager,
        scan_id: ScanId,
        f: impl FnOnce(&Analysis, &Rules) -> T,
    ) -> Result<T, AppError> {
        self.with(scans, scan_id, |rules, s| Ok(f(&s.analysis, rules)))
    }

    /// The rule's risk for a node, if it is a cleanup item of the cached analysis.
    pub fn risk_of(&self, scan_id: ScanId, node: NodeId) -> Option<Risk> {
        let guard = lock(&self.inner);
        let c = guard.as_ref().filter(|c| c.scan == scan_id)?;
        c.session.risk_of_node(&c.rules, node)
    }
}

/// Run heavy work off the async runtime with access to managed state.
async fn blocking<T: Send + 'static>(
    app: AppHandle,
    f: impl FnOnce(&AppHandle) -> Result<T, AppError> + Send + 'static,
) -> Result<T, AppError> {
    tauri::async_runtime::spawn_blocking(move || f(&app))
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
}

#[tauri::command]
pub async fn get_cleanup(
    app: AppHandle,
    scan_id: ScanId,
    refresh_apps: bool,
) -> Result<CleanupSummary, AppError> {
    blocking(app, move |app| {
        let state = app.state::<CleanupState>();
        let scans = app.state::<ScanManager>();
        state.ensure(&scans, scan_id, refresh_apps)?;
        state.with(&scans, scan_id, |rules, s| Ok(s.summary(rules)))
    })
    .await
}

#[tauri::command]
pub async fn list_cleanup_items(
    app: AppHandle,
    scan_id: ScanId,
    category: CategoryId,
    page: u32,
) -> Result<CleanupItemsPage, AppError> {
    blocking(app, move |app| {
        app.state::<CleanupState>()
            .with(&app.state::<ScanManager>(), scan_id, |rules, s| {
                Ok(s.page(rules, category, page))
            })
    })
    .await
}

#[tauri::command]
pub async fn set_cleanup_selected(
    app: AppHandle,
    scan_id: ScanId,
    node_ids: Vec<NodeId>,
    selected: bool,
    confirmed: bool,
) -> Result<CleanupSummary, AppError> {
    if node_ids.len() > MAX_BATCH {
        return Err(AppError::InvalidInput("too many items".into()));
    }
    blocking(app, move |app| {
        app.state::<CleanupState>()
            .with(&app.state::<ScanManager>(), scan_id, |rules, s| {
                s.set_selected(rules, &node_ids, selected, confirmed)?;
                Ok(s.summary(rules))
            })
    })
    .await
}

#[tauri::command]
pub async fn select_cleanup_category(
    app: AppHandle,
    scan_id: ScanId,
    category: Option<CategoryId>,
    mode: SelectMode,
) -> Result<CleanupSummary, AppError> {
    blocking(app, move |app| {
        app.state::<CleanupState>()
            .with(&app.state::<ScanManager>(), scan_id, |rules, s| {
                s.select_category(rules, category, mode);
                Ok(s.summary(rules))
            })
    })
    .await
}

/// The selection, for the Review sheet's dry run.
#[tauri::command]
pub async fn build_cleanup_plan(app: AppHandle, scan_id: ScanId) -> Result<CleanupPlan, AppError> {
    blocking(app, move |app| {
        app.state::<CleanupState>()
            .with(&app.state::<ScanManager>(), scan_id, |rules, s| {
                let items = s.selected_items();
                if items.len() > MAX_BATCH {
                    return Err(AppError::InvalidInput(format!(
                        "Select at most {MAX_BATCH} items at a time."
                    )));
                }
                Ok(CleanupPlan {
                    node_ids: items.iter().map(|i| i.node).collect(),
                    running_apps: s.summary(rules).running_selected,
                })
            })
    })
    .await
}

/// Move the selection to the Trash. Only items the user reviewed (`node_ids`, from
/// `build_cleanup_plan`) can move. With `quit_apps`, open apps that own selected items are
/// asked to quit first (never forced); items of apps still open are left alone.
#[tauri::command]
pub async fn execute_cleanup_plan(
    app: AppHandle,
    scan_id: ScanId,
    node_ids: Vec<NodeId>,
    quit_apps: bool,
) -> Result<TrashBatchResult, AppError> {
    if node_ids.len() > MAX_BATCH {
        return Err(AppError::InvalidInput("too many items".into()));
    }
    let reviewed: HashSet<NodeId> = node_ids.into_iter().collect();
    blocking(app, move |app| {
        let state = app.state::<CleanupState>();
        let scans = app.state::<ScanManager>();
        let roots: Vec<PathBuf> = scans.with_output(scan_id, |_, roots| {
            Ok(roots.iter().map(PathBuf::from).collect())
        })?;
        let ctx = Context::new(roots);
        let outcome = state.with(&scans, scan_id, |rules, s| {
            s.limit_to_reviewed(&reviewed);
            if quit_apps {
                let ids: Vec<String> = s
                    .selected_items()
                    .iter()
                    .filter(|i| i.running.is_some())
                    .filter_map(|i| i.app.clone())
                    .collect::<HashSet<_>>()
                    .into_iter()
                    .collect();
                let left = platform::quit_apps(&ids, QUIT_WAIT);
                if !left.is_empty() {
                    tracing::info!("apps still open after quit request: {left:?}");
                }
            }
            let (outcome, _) = s.execute(rules, &ctx, &MacTrash, &|it| {
                it.app.as_deref().is_some_and(platform::is_running)
            });
            Ok(outcome)
        })?;
        let result = app
            .state::<TrashState>()
            .with_store(|st| st.record(Room::Cleanup, &outcome, now()))?;
        let moved: Vec<PathBuf> = outcome.moved.iter().map(|m| m.original.clone()).collect();
        if !moved.is_empty() {
            let _ = scans.with_output_mut(scan_id, |out| {
                refresh(out, &moved);
                Ok(())
            });
        }
        Ok(result)
    })
    .await
}
