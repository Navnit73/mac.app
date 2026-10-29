//! Thin Tauri command layer. All real logic lives in `duobolt-core`.

use std::time::Duration;

mod scan;
mod settings;
mod views;

use duobolt_core::models::{ping as core_ping, PingEvent, PingResponse};
use duobolt_core::AppError;
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
    let result = tauri::Builder::default()
        .manage(scan::ScanManager::default())
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
            views::remove_exclusion
        ])
        .run(tauri::generate_context!());
    if let Err(e) = result {
        tracing::error!("tauri failed: {e}");
        std::process::exit(1);
    }
}
