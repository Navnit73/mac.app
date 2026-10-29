import { AGE_COLOR, KIND_COLOR } from '../../lib/colors'
import type { ColorBy } from '../../state/spaceStore'
import type { ChartNode } from '../../types/bindings/ChartNode'
import type { FileKind } from '../../types/bindings/FileKind'

const dominantCache = new WeakMap<ChartNode, FileKind>()

/** The file kind holding the most bytes below a folder; mixed or unknown content is `Folder`. */
export function dominantKind(n: ChartNode): FileKind {
  const cached = dominantCache.get(n)
  if (cached) return cached
  let result: FileKind
  if (!n.is_dir || n.children.length === 0) {
    result = n.file_kind
  } else {
    const bytes = new Map<FileKind, number>()
    const walk = (c: ChartNode) => {
      if (c.id !== null && (!c.is_dir || c.children.length === 0)) {
        bytes.set(c.file_kind, (bytes.get(c.file_kind) ?? 0) + c.value)
      }
      c.children.forEach(walk)
    }
    n.children.forEach(walk)
    let best: FileKind = 'Folder'
    let max = 0
    for (const [k, v] of bytes) {
      if (k !== 'Other' && k !== 'Folder' && v > max) {
        best = k
        max = v
      }
    }
    // Only call a folder by a kind if that kind is a real share of it.
    result = max > n.value * 0.35 ? best : 'Folder'
  }
  dominantCache.set(n, result)
  return result
}

/** The eight chart hues, in their fixed order (never the brand lime). */
export const SERIES = [
  'var(--s1)',
  'var(--s2)',
  'var(--s3)',
  'var(--s4)',
  'var(--s5)',
  'var(--s6)',
  'var(--s7)',
  'var(--s8)',
]

/** The view's top-level entries, largest first: the ones that get a hue in folder mode. */
export function topBranches(root: ChartNode): ChartNode[] {
  return root.children.filter((c) => c.id !== null).sort((a, b) => b.value - a.value)
}

/**
 * Folder mode: each of the eight largest top-level entries gets one hue, shared by everything
 * inside it; smaller ones share the neutral folder gray. More than eight hues cannot be told
 * apart, so they are never generated.
 */
export function branchHues(root: ChartNode): Map<ChartNode, string> {
  const map = new Map<ChartNode, string>()
  const paint = (n: ChartNode, hue: string) => {
    map.set(n, hue)
    n.children.forEach((c) => paint(c, hue))
  }
  topBranches(root).forEach((top, i) => paint(top, SERIES[i] ?? 'var(--folder)'))
  return map
}

/** Sunburst fill in folder mode: full hue on the inner ring, lighter on each ring out. */
export function ringColor(hue: string, depth: number): string {
  const pct = Math.max(42, 100 - (depth - 1) * 16)
  return pct === 100 ? hue : `color-mix(in srgb, ${hue} ${pct}%, var(--panel))`
}

/** Fill color of a chart node. The "Other" bucket is always neutral. */
export function nodeColor(n: ChartNode, by: ColorBy, hues?: Map<ChartNode, string>): string {
  if (n.id === null) return 'var(--other)'
  if (by === 'folder') return hues?.get(n) ?? 'var(--folder)'
  if (by === 'age') return AGE_COLOR[n.age]
  if (n.is_dir && n.children.length > 0) {
    const k = dominantKind(n)
    return k === 'Folder'
      ? 'var(--folder)'
      : `color-mix(in srgb, ${KIND_COLOR[k]} 58%, var(--panel))`
  }
  return KIND_COLOR[n.file_kind]
}

export function sumDepth(n: ChartNode): number {
  return 1 + Math.max(0, ...n.children.map(sumDepth))
}
