import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import type { AppError } from '../types/bindings/AppError'
import type { ChartMode } from '../types/bindings/ChartMode'
import type { ChartNode } from '../types/bindings/ChartNode'
import type { ChildrenPage } from '../types/bindings/ChildrenPage'
import type { Crumb } from '../types/bindings/Crumb'
import type { FdaStatus } from '../types/bindings/FdaStatus'
import type { NodeDetails } from '../types/bindings/NodeDetails'
import type { OverviewSummary } from '../types/bindings/OverviewSummary'
import type { SmartGroupSummary } from '../types/bindings/SmartGroupSummary'
import type { SortKey } from '../types/bindings/SortKey'
import type { TopFilesResult } from '../types/bindings/TopFilesResult'
import type { TopFilters } from '../types/bindings/TopFilters'
import type { PingEvent } from '../types/bindings/PingEvent'
import type { PingResponse } from '../types/bindings/PingResponse'
import type { ReportPage } from '../types/bindings/ReportPage'
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
}

export const events = {
  onTick: (handler: (e: PingEvent) => void): Promise<UnlistenFn> =>
    listen<PingEvent>('app://tick', (ev) => handler(ev.payload)),
  onScanProgress: (handler: (e: ScanProgressEvent) => void): Promise<UnlistenFn> =>
    listen<ScanProgressEvent>('scan://progress', (ev) => handler(ev.payload)),
  onScanDone: (handler: (e: ScanDoneEvent) => void): Promise<UnlistenFn> =>
    listen<ScanDoneEvent>('scan://done', (ev) => handler(ev.payload)),
}

export function describeError(e: unknown): string {
  if (typeof e === 'object' && e !== null && 'kind' in e) {
    const err = e as AppError
    return `${err.kind}: ${err.message}`
  }
  return String(e)
}
