//! Read-only views over a finished scan (Overview, Space, Inspector) and a few system actions.
//!
//! Commands take a scan id and node ids, never raw paths, so the UI cannot ask Rust to touch a
//! path the scan did not produce.

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use duobolt_core::index::NodeId;
use duobolt_core::models::{
    ChartMode, ChartNode, ChildrenPage, Crumb, FdaStatus, NodeDetails, OverviewSummary, ScanId,
    SmartGroupSummary, SortKey, TopFilesResult, TopFilters,
};
use duobolt_core::{inspect, overview, platform, space, AppError};
use tauri::State;

use crate::dupes_cmd::DupState;
use crate::scan::ScanManager;
use crate::settings;

fn home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default()
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

#[tauri::command]
pub async fn get_fda_status() -> Result<FdaStatus, AppError> {
    Ok(platform::full_disk_access(&home()))
}

#[tauri::command]
pub async fn open_fda_settings() -> Result<(), AppError> {
    platform::open_full_disk_access_settings()
}

#[tauri::command]
pub async fn get_overview(
    state: State<'_, ScanManager>,
    dups: State<'_, DupState>,
    id: ScanId,
) -> Result<OverviewSummary, AppError> {
    let dup_totals = dups.overview_totals(id);
    state.with_output(id, |out, roots| {
        let probe = roots
            .first()
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/"));
        let volume = platform::volume_info(&probe)?;
        let mut summary =
            overview::summarize(&out.tree, &out.summary, roots, volume, &home(), now());
        if let Some((bytes, copies)) = dup_totals {
            overview::set_duplicates(&mut summary, bytes, copies);
        }
        Ok(summary)
    })
}

#[tauri::command]
pub async fn get_children(
    state: State<'_, ScanManager>,
    id: ScanId,
    node_id: NodeId,
    sort: SortKey,
    descending: bool,
    cursor: u32,
) -> Result<ChildrenPage, AppError> {
    state.with_output(id, |out, _| {
        space::get_children(&out.tree, node_id, sort, descending, cursor)
    })
}

#[tauri::command]
pub async fn get_top_files(
    state: State<'_, ScanManager>,
    id: ScanId,
    filters: TopFilters,
    limit: u32,
) -> Result<TopFilesResult, AppError> {
    state.with_output(id, |out, _| {
        Ok(space::get_top_files(
            &out.tree,
            &filters,
            limit as usize,
            now(),
            Some(&home()),
        ))
    })
}

#[tauri::command]
pub async fn get_smart_groups(
    state: State<'_, ScanManager>,
    id: ScanId,
) -> Result<Vec<SmartGroupSummary>, AppError> {
    state.with_output(id, |out, _| {
        Ok(space::smart_groups(&out.tree, now(), Some(&home())))
    })
}

#[tauri::command]
pub async fn get_chart_data(
    state: State<'_, ScanManager>,
    id: ScanId,
    node_id: NodeId,
    depth: u8,
    mode: ChartMode,
) -> Result<ChartNode, AppError> {
    state.with_output(id, |out, _| {
        space::get_chart_data(&out.tree, node_id, depth, mode, now())
    })
}

#[tauri::command]
pub async fn get_breadcrumb(
    state: State<'_, ScanManager>,
    id: ScanId,
    node_id: NodeId,
) -> Result<Vec<Crumb>, AppError> {
    state.with_output(id, |out, _| space::get_breadcrumb(&out.tree, node_id))
}

#[tauri::command]
pub async fn get_node_details(
    state: State<'_, ScanManager>,
    id: ScanId,
    node_id: NodeId,
) -> Result<NodeDetails, AppError> {
    state.with_output(id, |out, _| {
        inspect::node_details(&out.tree, node_id, &home())
    })
}

fn node_path(state: &ScanManager, id: ScanId, node_id: NodeId) -> Result<PathBuf, AppError> {
    state.with_output(id, |out, _| {
        out.tree
            .node(node_id)
            .map(|_| out.tree.path(node_id))
            .ok_or_else(|| AppError::NotFound(format!("node {node_id}")))
    })
}

#[tauri::command]
pub async fn reveal_in_finder(
    state: State<'_, ScanManager>,
    id: ScanId,
    node_id: NodeId,
) -> Result<(), AppError> {
    platform::reveal_in_finder(&node_path(&state, id, node_id)?)
}

#[tauri::command]
pub async fn open_node(
    state: State<'_, ScanManager>,
    id: ScanId,
    node_id: NodeId,
) -> Result<(), AppError> {
    platform::open_path(&node_path(&state, id, node_id)?)
}

#[tauri::command]
pub async fn quick_look(
    state: State<'_, ScanManager>,
    id: ScanId,
    node_id: NodeId,
) -> Result<(), AppError> {
    platform::quick_look(&node_path(&state, id, node_id)?)
}

#[tauri::command]
pub async fn add_exclusion(
    state: State<'_, ScanManager>,
    id: ScanId,
    node_id: NodeId,
) -> Result<Vec<String>, AppError> {
    let path = node_path(&state, id, node_id)?
        .to_string_lossy()
        .into_owned();
    let mut s = settings::load();
    if !s.exclusions.contains(&path) {
        s.exclusions.push(path);
        settings::save(&s)?;
    }
    Ok(s.exclusions)
}

#[tauri::command]
pub async fn get_exclusions() -> Result<Vec<String>, AppError> {
    Ok(settings::load().exclusions)
}

#[tauri::command]
pub async fn remove_exclusion(path: String) -> Result<Vec<String>, AppError> {
    let mut s = settings::load();
    s.exclusions.retain(|p| *p != path);
    settings::save(&s)?;
    Ok(s.exclusions)
}
