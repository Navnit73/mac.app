import { AGE_COLOR, AGE_LABEL, KIND_COLOR, KIND_LABEL, KIND_ORDER } from '../../lib/colors'
import type { ColorBy } from '../../state/spaceStore'
import type { AgeBucket } from '../../types/bindings/AgeBucket'
import type { ChartNode } from '../../types/bindings/ChartNode'
import type { FileKind } from '../../types/bindings/FileKind'
import { SERIES, topBranches } from './chartColor'

function collect(n: ChartNode, kinds: Set<FileKind>, ages: Set<AgeBucket>) {
  if (n.id !== null) {
    kinds.add(n.file_kind)
    ages.add(n.age)
  }
  n.children.forEach((c) => collect(c, kinds, ages))
}

/** Legend for whatever the chart actually shows, so identity is never color alone. */
export default function ChartLegend({ root, colorBy }: { root: ChartNode; colorBy: ColorBy }) {
  const kinds = new Set<FileKind>()
  const ages = new Set<AgeBucket>()
  root.children.forEach((c) => collect(c, kinds, ages))
  const tops = topBranches(root)
  const items =
    colorBy === 'folder'
      ? [
          ...tops.slice(0, SERIES.length).map((n, i) => ({
            key: `f${n.id}`,
            label: n.name,
            color: SERIES[i] ?? 'var(--folder)',
          })),
          ...(tops.length > SERIES.length
            ? [{ key: 'more', label: 'Other folders', color: 'var(--folder)' }]
            : []),
        ]
      : colorBy === 'kind'
        ? KIND_ORDER.filter((k) => kinds.has(k)).map((k) => ({
            key: k,
            label: KIND_LABEL[k],
            color: KIND_COLOR[k],
          }))
        : (['Recent', 'ThisYear', 'Old'] as AgeBucket[])
            .filter((a) => ages.has(a))
            .map((a) => ({ key: a, label: AGE_LABEL[a], color: AGE_COLOR[a] }))
  return (
    <ul className="flex flex-wrap gap-x-4 gap-y-1.5" aria-label="Legend">
      {items.map((i) => (
        <li key={i.key} className="flex max-w-56 items-center gap-1.5 text-muted">
          <span className="h-2.5 w-2.5 shrink-0 rounded-[3px]" style={{ background: i.color }} />
          <span className="truncate" title={i.label}>
            {i.label}
          </span>
        </li>
      ))}
      <li className="flex items-center gap-1.5 text-muted">
        <span className="h-2.5 w-2.5 rounded-[3px]" style={{ background: 'var(--other)' }} />
        Smaller items
      </li>
      {colorBy === 'folder' && (
        <li className="text-faint">Each top folder has one color; lighter means deeper inside.</li>
      )}
      {colorBy === 'kind' && (
        <li className="text-faint">Paler arcs are folders, tinted by what is mostly inside.</li>
      )}
    </ul>
  )
}
