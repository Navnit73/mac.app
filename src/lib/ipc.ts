import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import type { AppError } from '../types/bindings/AppError'
import type { ChartMode } from '../types/bindings/ChartMode'
import type { ChartNode } from '../types/bindings/ChartNode'
import type { ChildrenPage } from '../types/bindings/ChildrenPage'
import type { AppDetails } from '../types/bindings/AppDetails'
import type { AppsDoneEvent } from '../types/bindings/AppsDoneEvent'
import type { AppsList } from '../types/bindings/AppsList'
import type { CategoryId } from '../types/bindings/CategoryId'
import type { CleanupItemsPage } from '../types/bindings/CleanupItemsPage'
import type { CleanupPlan } from '../types/bindings/CleanupPlan'
import type { CleanupSummary } from '../types/bindings/CleanupSummary'
import type { SelectMode } from '../types/bindings/SelectMode'
import type { Crumb } from '../types/bindings/Crumb'
import type { DupDoneEvent } from '../types/bindings/DupDoneEvent'
import type { DupGroupsPage } from '../types/bindings/DupGroupsPage'
import type { DupOptions } from '../types/bindings/DupOptions'
import type { DupProgress } from '../types/bindings/DupProgress'
import type { DupSavings } from '../types/bindings/DupSavings'
import type { DupStatus } from '../types/bindings/DupStatus'
import type { FileKind } from '../types/bindings/FileKind'
import type { KeepRule } from '../types/bindings/KeepRule'
import type { FdaStatus } from '../types/bindings/FdaStatus'
import type { NodeDetails } from '../types/bindings/NodeDetails'
import type { OverviewSummary } from '../types/bindings/OverviewSummary'
import type { SmartGroupSummary } from '../types/bindings/SmartGroupSummary'
import type { SortKey } from '../types/bindings/SortKey'
import type { TopFilesResult } from '../types/bindings/TopFilesResult'
import type { TopFilters } from '../types/bindings/TopFilters'
import type { PingEvent } from '../types/bindings/PingEvent'
import type { PingResponse } from '../types/bindings/PingResponse'
import type { BatchPage } from '../types/bindings/BatchPage'
import type { BatchSummary } from '../types/bindings/BatchSummary'
import type { EmptyTrashPrep } from '../types/bindings/EmptyTrashPrep'
import type { ReportPage } from '../types/bindings/ReportPage'
import type { Room } from '../types/bindings/Room'
import type { TrashBatchResult } from '../types/bindings/TrashBatchResult'
import type { TrashPreview } from '../types/bindings/TrashPreview'
import type { UndoResult } from '../types/bindings/UndoResult'
import type { WeeklyStats } from '../types/bindings/WeeklyStats'
import type { SearchQuery } from '../types/bindings/SearchQuery'
import type { SearchResult } from '../types/bindings/SearchResult'
import type { ScanDoneEvent } from '../types/bindings/ScanDoneEvent'
import type { ScanOptions } from '../types/bindings/ScanOptions'
import type { ScanProgressEvent } from '../types/bindings/ScanProgressEvent'
import type { ScanState } from '../types/bindings/ScanState'
import type { ScanSummary } from '../types/bindings/ScanSummary'

/** Typed command wrappers. Payload types are generated from Rust (ts-rs). */
export const commands = {
  ping: (name: string) => invoke<PingResponse>('ping', { name }),
  emitTicks: (count: number) => invoke<void>('emit_ticks', { count }),
  startScan: (roots: string[], options?: Partial<ScanOptions>) =>
    invoke<number>('start_scan', { roots, options: options ?? null }),
  cancelScan: (id: number) => invoke<void>('cancel_scan', { id }),
  getScanState: (id: number) => invoke<ScanState>('get_scan_state', { id }),
  getScanReport: (id: number, page: number) => invoke<ReportPage>('get_scan_report', { id, page }),
  rescanSubtree: (id: number, nodeId: number) =>
    invoke<ScanSummary>('rescan_subtree', { id, nodeId }),

  search: (id: number, query: SearchQuery, cursor: number) =>
    invoke<SearchResult>('search', { id, query, cursor }),
  previewTrash: (scanId: number, nodeIds: number[]) =>
    invoke<TrashPreview>('preview_trash', { scanId, nodeIds }),
  trashItems: (scanId: number, nodeIds: number[], room: Room) =>
    invoke<TrashBatchResult>('trash_items', { scanId, nodeIds, room }),
  undoBatch: (
    batchId: number,
    itemIds: number[] | null,
    allowRename: boolean,
    scanId: number | null,
  ) => invoke<UndoResult>('undo_batch', { batchId, itemIds, allowRename, scanId }),
  undoLast: (scanId: number | null) => invoke<UndoResult | null>('undo_last', { scanId }),
  prepareEmptyTrash: () => invoke<EmptyTrashPrep>('prepare_empty_trash'),
  emptyTrash: (token: string, scanId: number | null) =>
    invoke<number>('empty_trash', { token, scanId }),
  listBatches: (limit: number, offset: number) =>
    invoke<BatchSummary[]>('list_batches', { limit, offset }),
  getBatch: (batchId: number, page: number) => invoke<BatchPage>('get_batch', { batchId, page }),
  getWeeklyStats: (weeks: number) => invoke<WeeklyStats>('get_weekly_stats', { weeks }),
  exportActivityCsv: () => invoke<string>('export_activity_csv'),
  clearActivity: () => invoke<void>('clear_activity'),
  getFdaStatus: () => invoke<FdaStatus>('get_fda_status'),
  openFdaSettings: () => invoke<void>('open_fda_settings'),
  getOverview: (id: number) => invoke<OverviewSummary>('get_overview', { id }),
  getChildren: (id: number, nodeId: number, sort: SortKey, descending: boolean, cursor: number) =>
    invoke<ChildrenPage>('get_children', { id, nodeId, sort, descending, cursor }),
  getTopFiles: (id: number, filters: Partial<TopFilters>, limit: number) =>
    invoke<TopFilesResult>('get_top_files', { id, filters, limit }),
  getSmartGroups: (id: number) => invoke<SmartGroupSummary[]>('get_smart_groups', { id }),
  getChartData: (id: number, nodeId: number, depth: number, mode: ChartMode) =>
    invoke<ChartNode>('get_chart_data', { id, nodeId, depth, mode }),
  getBreadcrumb: (id: number, nodeId: number) => invoke<Crumb[]>('get_breadcrumb', { id, nodeId }),
  getNodeDetails: (id: number, nodeId: number) =>
    invoke<NodeDetails>('get_node_details', { id, nodeId }),
  revealInFinder: (id: number, nodeId: number) => invoke<void>('reveal_in_finder', { id, nodeId }),
  openNode: (id: number, nodeId: number) => invoke<void>('open_node', { id, nodeId }),
  quickLook: (id: number, nodeId: number) => invoke<void>('quick_look', { id, nodeId }),
  addExclusion: (id: number, nodeId: number) => invoke<string[]>('add_exclusion', { id, nodeId }),
  getExclusions: () => invoke<string[]>('get_exclusions'),
  removeExclusion: (path: string) => invoke<string[]>('remove_exclusion', { path }),

  startDuplicateScan: (scanId: number, options: DupOptions) =>
    invoke<void>('start_duplicate_scan', { scanId, options }),
  cancelDuplicateScan: () => invoke<void>('cancel_duplicate_scan'),
  getDuplicateStatus: (scanId: number) => invoke<DupStatus>('get_duplicate_status', { scanId }),
  getDuplicateGroups: (scanId: number, page: number, kind: FileKind | null) =>
    invoke<DupGroupsPage>('get_duplicate_groups', { scanId, page, kind }),
  /** `folderNode` may be a file: its folder is preferred. `groupIds` null means every group. */
  applyKeepRule: (
    scanId: number,
    rule: KeepRule,
    folderNode: number | null,
    groupIds: number[] | null,
  ) => invoke<DupSavings>('apply_keep_rule', { scanId, rule, folderNode, groupIds }),
  setDuplicateSelected: (scanId: number, groupId: number, nodeId: number, selected: boolean) =>
    invoke<DupSavings>('set_duplicate_selected', { scanId, groupId, nodeId, selected }),
  duplicateSelection: (scanId: number) => invoke<number[]>('duplicate_selection', { scanId }),
  removeDuplicates: (scanId: number, clones: boolean) =>
    invoke<TrashBatchResult>('remove_duplicates', { scanId, clones }),

  /** `refreshApps` re-reads which apps are open and installed. */
  getCleanup: (scanId: number, refreshApps: boolean) =>
    invoke<CleanupSummary>('get_cleanup', { scanId, refreshApps }),
  listCleanupItems: (scanId: number, category: CategoryId, page: number) =>
    invoke<CleanupItemsPage>('list_cleanup_items', { scanId, category, page }),
  setCleanupSelected: (scanId: number, nodeIds: number[], selected: boolean, confirmed: boolean) =>
    invoke<CleanupSummary>('set_cleanup_selected', { scanId, nodeIds, selected, confirmed }),
  selectCleanupCategory: (scanId: number, category: CategoryId | null, mode: SelectMode) =>
    invoke<CleanupSummary>('select_cleanup_category', { scanId, category, mode }),
  buildCleanupPlan: (scanId: number) => invoke<CleanupPlan>('build_cleanup_plan', { scanId }),
  executeCleanupPlan: (scanId: number, quitApps: boolean) =>
    invoke<TrashBatchResult>('execute_cleanup_plan', { scanId, quitApps }),

  startAppsScan: () => invoke<void>('start_apps_scan'),
  listApps: (scanId: number | null) => invoke<AppsList>('list_apps', { scanId }),
  getAppDetails: (appId: number) => invoke<AppDetails>('get_app_details', { appId }),
  appIcon: (appId: number) => invoke<string | null>('app_icon', { appId }),
  uninstallApp: (appId: number, related: number[], quit: boolean, scanId: number | null) =>
    invoke<TrashBatchResult>('uninstall_app', { appId, related, quit, scanId }),
}

export const events = {
  onTick: (handler: (e: PingEvent) => void): Promise<UnlistenFn> =>
    listen<PingEvent>('app://tick', (ev) => handler(ev.payload)),
  onScanProgress: (handler: (e: ScanProgressEvent) => void): Promise<UnlistenFn> =>
    listen<ScanProgressEvent>('scan://progress', (ev) => handler(ev.payload)),
  onScanDone: (handler: (e: ScanDoneEvent) => void): Promise<UnlistenFn> =>
    listen<ScanDoneEvent>('scan://done', (ev) => handler(ev.payload)),
  onDupesProgress: (handler: (e: DupProgress) => void): Promise<UnlistenFn> =>
    listen<DupProgress>('dupes://progress', (ev) => handler(ev.payload)),
  onDupesDone: (handler: (e: DupDoneEvent) => void): Promise<UnlistenFn> =>
    listen<DupDoneEvent>('dupes://done', (ev) => handler(ev.payload)),
  onAppsDone: (handler: (e: AppsDoneEvent) => void): Promise<UnlistenFn> =>
    listen<AppsDoneEvent>('apps://done', (ev) => handler(ev.payload)),
}

export function describeError(e: unknown): string {
  if (typeof e === 'object' && e !== null && 'kind' in e) {
    const err = e as AppError
    return `${err.kind}: ${err.message}`
  }
  return String(e)
}
