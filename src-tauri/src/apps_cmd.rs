//! Applications commands. A small dedicated scan of the app folders and the Library locations
//! gives sizes and the identity of every file, so uninstalls go through the same checks as any
//! other move (`trash::preflight` against those roots).

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use mymacstorage_core::apps::{self, App, Related};
use mymacstorage_core::index::NodeId;
use mymacstorage_core::models::{
    AppDetails, AppsDoneEvent, AppsList, CategoryId, Room, ScanId, ScanOptions, TrashBatchResult,
};
use mymacstorage_core::scanner::bulk::BulkReader;
use mymacstorage_core::scanner::{scan, ScanOutput};
use mymacstorage_core::trash::{Context, MacTrash};
use mymacstorage_core::{platform, AppError};
use tauri::{AppHandle, Emitter, Manager};

use crate::cleanup_cmd::CleanupState;
use crate::scan::ScanManager;
use crate::trash_cmd::{home, now, refresh, TrashState};

const QUIT_WAIT: Duration = Duration::from_secs(10);
const ICON_PX: u32 = 64;

struct AppsIndex {
    apps: Vec<App>,
    out: ScanOutput,
    roots: Vec<PathBuf>,
    related: HashMap<usize, Vec<Related>>,
    last_used: Vec<Option<i64>>,
    /// Apps removed since the scan. They keep their place so every other app keeps its id: the
    /// UI addresses apps by id, and a shifted id would point at a different app.
    gone: HashSet<usize>,
}

impl AppsIndex {
    fn build() -> Result<Self, AppError> {
        let home = home();
        let apps = apps::discover(&apps::app_dirs(&home));
        let roots = apps::scan_roots(&home);
        let out = scan(
            &BulkReader::new(),
            &roots,
            &ScanOptions::default(),
            &AtomicBool::new(false),
            &mut |_| {},
        )?;
        let related = apps::related_all(&out.tree, &home, &apps);
        let paths: Vec<&std::path::Path> = apps.iter().map(|a| a.path.as_path()).collect();
        let last_used = platform::last_used_many(&paths);
        Ok(Self {
            apps,
            out,
            roots,
            related,
            last_used,
            gone: HashSet::new(),
        })
    }

    fn related(&self, i: usize) -> &[Related] {
        self.related.get(&i).map_or(&[], |v| v.as_slice())
    }

    fn app(&self, id: u32) -> Result<&App, AppError> {
        let i = id as usize;
        self.apps
            .get(i)
            .filter(|_| !self.gone.contains(&i))
            .ok_or_else(|| AppError::NotFound("That app is no longer listed. Scan again.".into()))
    }

    /// After a move: refresh sizes and mark apps whose bundle is gone.
    fn after_move(&mut self, moved: &[PathBuf]) {
        refresh(&mut self.out, moved);
        for (i, a) in self.apps.iter().enumerate() {
            if std::fs::symlink_metadata(&a.path).is_err() {
                self.gone.insert(i);
            }
        }
        self.related = apps::related_all(&self.out.tree, &home(), &self.apps);
    }
}

#[derive(Default)]
pub struct AppsState {
    index: Mutex<Option<AppsIndex>>,
    running: AtomicBool,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl AppsState {
    fn with<T>(
        &self,
        f: impl FnOnce(&mut AppsIndex) -> Result<T, AppError>,
    ) -> Result<T, AppError> {
        match lock(&self.index).as_mut() {
            Some(ix) => f(ix),
            None => Err(AppError::NotFound(
                "Applications have not been scanned yet.".into(),
            )),
        }
    }
}

async fn blocking<T: Send + 'static>(
    app: AppHandle,
    f: impl FnOnce(&AppHandle) -> Result<T, AppError> + Send + 'static,
) -> Result<T, AppError> {
    tauri::async_runtime::spawn_blocking(move || f(&app))
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
}

/// Read every installed app and size it with its related files. Ends with `apps://done`.
#[tauri::command]
pub async fn start_apps_scan(app: AppHandle) -> Result<(), AppError> {
    let state = app.state::<AppsState>();
    if state.running.swap(true, Ordering::SeqCst) {
        return Ok(());
    }
    std::thread::spawn(move || {
        let built = std::panic::catch_unwind(AppsIndex::build);
        let state = app.state::<AppsState>();
        let event = match built {
            Ok(Ok(ix)) => {
                let count = ix.apps.len() as u32;
                *lock(&state.index) = Some(ix);
                AppsDoneEvent { count, error: None }
            }
            Ok(Err(e)) => AppsDoneEvent {
                count: 0,
                error: Some(e.to_string()),
            },
            Err(_) => AppsDoneEvent {
                count: 0,
                error: Some("Reading applications stopped unexpectedly.".into()),
            },
        };
        state.running.store(false, Ordering::SeqCst);
        let _ = app.emit("apps://done", event);
    });
    Ok(())
}

#[tauri::command]
pub async fn list_apps(app: AppHandle, scan_id: Option<ScanId>) -> Result<AppsList, AppError> {
    blocking(app, move |app| {
        let running = platform::running_apps();
        let apps = app.state::<AppsState>().with(|ix| {
            Ok(ix
                .apps
                .iter()
                .enumerate()
                .filter(|(i, _)| !ix.gone.contains(i))
                .map(|(i, a)| {
                    let open = running
                        .iter()
                        .any(|r| r.bundle_id.eq_ignore_ascii_case(&a.bundle_id));
                    apps::summarize(
                        i,
                        a,
                        &ix.out.tree,
                        ix.related(i),
                        ix.last_used.get(i).copied().flatten(),
                        open,
                    )
                })
                .collect::<Vec<_>>())
        })?;
        // Leftovers come from the Cleanup analysis, so both rooms show the same numbers.
        let (leftover_bytes, leftover_items) = match scan_id {
            Some(id) => app
                .state::<CleanupState>()
                .with_analysis(&app.state::<ScanManager>(), id, |a, rules| {
                    a.totals(rules)
                        .get(&CategoryId::Leftovers)
                        .copied()
                        .unwrap_or_default()
                })
                .unwrap_or_default(),
            None => (0, 0),
        };
        Ok(AppsList {
            apps,
            leftover_bytes,
            leftover_items,
        })
    })
    .await
}

#[tauri::command]
pub async fn get_app_details(app: AppHandle, app_id: u32) -> Result<AppDetails, AppError> {
    blocking(app, move |app| {
        let (summary, node, related, path, bundle_id) = app.state::<AppsState>().with(|ix| {
            let a = ix.app(app_id)?;
            let i = app_id as usize;
            let summary = apps::summarize(
                i,
                a,
                &ix.out.tree,
                ix.related(i),
                ix.last_used.get(i).copied().flatten(),
                platform::is_running(&a.bundle_id),
            );
            Ok((
                summary,
                ix.out.tree.find_path(&a.path),
                ix.related(i)
                    .iter()
                    .map(Related::to_model)
                    .collect::<Vec<_>>(),
                a.path.clone(),
                a.bundle_id.clone(),
            ))
        })?;
        Ok(AppDetails {
            app: summary,
            app_node: node,
            related,
            signed_by: platform::code_signer(&path),
            receipts: apps::receipts(&bundle_id),
        })
    })
    .await
}

/// A small PNG of the app's icon, made once and cached. The file is added to the asset protocol
/// scope so the UI can show it; nothing else is.
#[tauri::command]
pub async fn app_icon(app: AppHandle, app_id: u32) -> Result<Option<String>, AppError> {
    blocking(app, move |app| {
        let (icon, bundle_id) = app.state::<AppsState>().with(|ix| {
            let a = ix.app(app_id)?;
            Ok((a.icon.clone(), a.bundle_id.clone()))
        })?;
        let Some(icns) = icon.filter(|p| p.exists()) else {
            return Ok(None);
        };
        let safe: String = bundle_id
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '.' || c == '-' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        let dir = home()
            .join("Library/Application Support")
            .join(crate::settings::DATA_DIR)
            .join("icons");
        std::fs::create_dir_all(&dir)?;
        let png = dir.join(format!("{safe}.png"));
        if !png.exists() && platform::icon_to_png(&icns, &png, ICON_PX).is_err() {
            return Ok(None);
        }
        app.asset_protocol_scope()
            .allow_file(&png)
            .map_err(|e| AppError::Internal(e.to_string()))?;
        Ok(Some(png.to_string_lossy().into_owned()))
    })
    .await
}

/// Move the app and the chosen related files to the Trash. An open app is asked to quit when
/// `quit` is set (never forced); otherwise the user is told to quit it first. `bundle_id` is the
/// app the user looked at; if `app_id` now names another app, nothing happens.
#[tauri::command]
pub async fn uninstall_app(
    app: AppHandle,
    app_id: u32,
    bundle_id: String,
    related: Vec<NodeId>,
    quit: bool,
    scan_id: Option<ScanId>,
) -> Result<TrashBatchResult, AppError> {
    let tm_app = app.clone();
    let result = blocking(app, move |app| {
        let state = app.state::<AppsState>();
        let (outcome, moved) = state.with(|ix| {
            let a = ix.app(app_id)?.clone();
            if a.bundle_id != bundle_id {
                return Err(AppError::InvalidInput(
                    "The app list changed. Scan the apps again, then try once more.".into(),
                ));
            }
            if let Some(why) = apps::locked_reason(&a) {
                return Err(AppError::InvalidInput(why));
            }
            if platform::is_running(&a.bundle_id) {
                if !quit {
                    return Err(AppError::InvalidInput(format!(
                        "{} is open. Quit it first.",
                        a.name
                    )));
                }
                if !platform::quit_apps(std::slice::from_ref(&a.bundle_id), QUIT_WAIT).is_empty() {
                    return Err(AppError::InvalidInput(format!(
                        "{} did not quit. Quit it yourself (it may be asking to save), then try again.",
                        a.name
                    )));
                }
            }
            let plan = apps::plan_uninstall(&ix.out.tree, &a, ix.related(app_id as usize), &related)
                .map_err(AppError::InvalidInput)?;
            let ctx = Context::new(ix.roots.clone());
            let outcome = apps::uninstall(&plan, &ctx, &MacTrash);
            let moved: Vec<PathBuf> = outcome.moved.iter().map(|m| m.original.clone()).collect();
            ix.after_move(&moved);
            Ok((outcome, moved))
        })?;
        let result = app
            .state::<TrashState>()
            .with_store(|st| st.record(Room::Applications, &outcome, now()))?;
        // Keep the main scan true as well when it covers the moved files.
        if let (Some(id), false) = (scan_id, moved.is_empty()) {
            let _ = app.state::<ScanManager>().with_output_mut(id, |out| {
                refresh(out, &moved);
                Ok(())
            });
        }
        Ok(result)
    })
    .await;
    if let Ok(r) = &result {
        crate::telemetry::track(
            &tm_app,
            "app.uninstalled",
            serde_json::json!({ "count": r.moved_count, "bytes": r.moved_bytes }),
        );
    }
    result
}
