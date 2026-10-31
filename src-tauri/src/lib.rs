//! Thin Tauri command layer. All real logic lives in `mymacstorage-core`.

use std::time::Duration;

mod apps_cmd;
mod cleanup_cmd;
mod dupes_cmd;
mod scan;
mod settings;
mod telemetry;
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
            telemetry::start(app.handle());
            if let Err(e) = install_menu(app.handle()) {
                tracing::warn!("could not build the app menu: {e}");
            }
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
            apps_cmd::uninstall_app,
            telemetry::track_event,
            telemetry::get_offered_update,
            telemetry::check_for_updates,
            telemetry::open_update_download,
            telemetry::skip_update,
            telemetry::quit_app
        ])
        .build(tauri::generate_context!());
    match result {
        Ok(app) => app.run(|handle, event| {
            if let tauri::RunEvent::Exit = event {
                telemetry::flush_on_quit(handle);
            }
        }),
        Err(e) => {
            tracing::error!("tauri failed: {e}");
            std::process::exit(1);
        }
    }
}

const MENU_CHECK_UPDATES: &str = "check_updates";
const MENU_SHARE_USAGE: &str = "share_usage";

/// The standard macOS menu plus "Check for Updates…" and "Share Anonymous Usage Data" under
/// the app menu, right after "About".
fn install_menu(app: &AppHandle) -> tauri::Result<()> {
    use tauri::menu::{CheckMenuItem, Menu, MenuItem, MenuItemKind, PredefinedMenuItem};
    use tauri::Manager;

    let menu = Menu::default(app)?;
    let share = app
        .try_state::<telemetry::Telemetry>()
        .is_some_and(|t| t.share_usage());
    if let Some(MenuItemKind::Submenu(app_menu)) = menu.items()?.into_iter().next() {
        let check = MenuItem::with_id(
            app,
            MENU_CHECK_UPDATES,
            "Check for Updates…",
            true,
            None::<&str>,
        )?;
        let share_item = CheckMenuItem::with_id(
            app,
            MENU_SHARE_USAGE,
            "Share Anonymous Usage Data",
            true,
            share,
            None::<&str>,
        )?;
        app_menu.insert(&check, 1)?;
        app_menu.insert(&share_item, 2)?;
        app_menu.insert(&PredefinedMenuItem::separator(app)?, 3)?;
        app.on_menu_event(move |app, event| {
            if event.id() == MENU_CHECK_UPDATES {
                let _ = app.emit(telemetry::CHECK_UPDATES_EVENT, ());
            } else if event.id() == MENU_SHARE_USAGE {
                // The menu toggles the check mark itself; read the new state.
                let on = share_item.is_checked().unwrap_or(true);
                telemetry::set_share_usage(app, on);
            }
        });
    }
    app.set_menu(menu)?;
    Ok(())
}
