//! Thin Tauri command layer. All real logic lives in `mymacstorage-core`.

use std::time::Duration;

mod apps_cmd;
mod cleanup_cmd;
mod dupes_cmd;
mod scan;
mod settings;
mod trash_cmd;
mod views;

use mymacstorage_core::models::{ping as core_ping, PingEvent, PingResponse};
use mymacstorage_core::AppError;
use tauri::{AppHandle, Emitter};

/// Event name for the demo ticker.
const TICK_EVENT: &str = "app://tick";
/// Upper bound so the UI cannot request an unbounded emitter.
const MAX_TICKS: u32 = 20;

/// Round trip UI -> Rust -> UI.
#[tauri::command]
fn ping(name: String) -> Result<PingResponse, AppError> {
    if name.len() > 64 {
        return Err(AppError::InvalidInput("name too long".into()));
    }
    Ok(core_ping(&name))
}

/// Emits `count` `app://tick` events, 200 ms apart, to prove the event channel.
#[tauri::command]
fn emit_ticks(app: AppHandle, count: u32) -> Result<(), AppError> {
    let count = count.min(MAX_TICKS);
    std::thread::spawn(move || {
        for seq in 0..count {
            if app.emit(TICK_EVENT, PingEvent { seq }).is_err() {
                break;
            }
            std::thread::sleep(Duration::from_millis(200));
        }
    });
    Ok(())
}

/// Build and run the application.
pub fn run() {
    settings::migrate_legacy_data_dir();
    let result = tauri::Builder::default()
        .manage(scan::ScanManager::default())
        .manage(trash_cmd::TrashState::default())
        .manage(dupes_cmd::DupState::default())
        .manage(cleanup_cmd::CleanupState::default())
        .manage(apps_cmd::AppsState::default())
        .setup(|app| {
            use tauri::Manager;
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                trash_cmd::reconcile_on_launch(&handle.state::<trash_cmd::TrashState>());
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            ping,
            emit_ticks,
            scan::start_scan,
            scan::cancel_scan,
            scan::get_scan_state,
            scan::get_scan_report,
            scan::rescan_subtree,
            scan::search,
            views::get_fda_status,
            views::open_fda_settings,
            views::get_fda_target,
            views::reveal_fda_target,
            views::get_overview,
            views::get_children,
            views::get_top_files,
            views::get_smart_groups,
            views::get_chart_data,
            views::get_breadcrumb,
            views::get_node_details,
            views::reveal_in_finder,
            views::open_node,
            views::quick_look,
            views::add_exclusion,
            views::get_exclusions,
            views::remove_exclusion,
            trash_cmd::preview_trash,
            trash_cmd::trash_items,
            trash_cmd::undo_batch,
            trash_cmd::undo_last,
            trash_cmd::prepare_empty_trash,
            trash_cmd::empty_trash,
            trash_cmd::list_batches,
            trash_cmd::get_batch,
            trash_cmd::get_weekly_stats,
            trash_cmd::export_activity_csv,
            trash_cmd::clear_activity,
            dupes_cmd::start_duplicate_scan,
            dupes_cmd::cancel_duplicate_scan,
            dupes_cmd::get_duplicate_status,
            dupes_cmd::get_duplicate_groups,
            dupes_cmd::apply_keep_rule,
            dupes_cmd::set_duplicate_selected,
            dupes_cmd::duplicate_selection,
            dupes_cmd::remove_duplicates,
            cleanup_cmd::get_cleanup,
            cleanup_cmd::list_cleanup_items,
            cleanup_cmd::set_cleanup_selected,
            cleanup_cmd::select_cleanup_category,
            cleanup_cmd::build_cleanup_plan,
            cleanup_cmd::execute_cleanup_plan,
            apps_cmd::start_apps_scan,
            apps_cmd::list_apps,
            apps_cmd::get_app_details,
            apps_cmd::app_icon,
            apps_cmd::uninstall_app
        ])
        .run(tauri::generate_context!());
    if let Err(e) = result {
        tracing::error!("tauri failed: {e}");
        std::process::exit(1);
    }
}
