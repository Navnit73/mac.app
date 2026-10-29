import { useEffect, useState } from 'react'
import { useNavigate } from 'react-router-dom'
import Icon from '../components/Icon'
import ScanButton from '../components/ScanButton'
import Breadcrumb from '../components/space/Breadcrumb'
import ChartLegend from '../components/space/ChartLegend'
import FolderList from '../components/space/FolderList'
import LargeFiles from '../components/space/LargeFiles'
import Sunburst from '../components/space/Sunburst'
import Treemap from '../components/space/Treemap'
import { commands, describeError } from '../lib/ipc'
import { useFindStore } from '../state/findStore'
import { useScanStore } from '../state/scanStore'
import { useSpaceStore, type ColorBy, type SpaceTab, type SpaceView } from '../state/spaceStore'
import type { ChartNode } from '../types/bindings/ChartNode'

function Segmented<T extends string>({
  label,
  value,
  options,
  onChange,
}: {
  label: string
  value: T
  options: { value: T; label: string; icon?: 'list' | 'sunburst' | 'treemap' }[]
  onChange: (v: T) => void
}) {
  return (
    <div className="seg" role="group" aria-label={label}>
      {options.map((o) => (
        <button
          key={o.value}
          type="button"
          aria-pressed={value === o.value}
          onClick={() => onChange(o.value)}
        >
          <span className="flex items-center gap-1.5">
            {o.icon && <Icon name={o.icon} size={13} />}
            {o.label}
          </span>
        </button>
      ))}
    </div>
  )
}

function Chart({ scanId }: { scanId: number }) {
  const view = useSpaceStore((s) => s.view)
  const colorBy = useSpaceStore((s) => s.colorBy)
  const currentId = useSpaceStore((s) => s.currentId)
  const selectedId = useSpaceStore((s) => s.selectedId)
  const navigate = useSpaceStore((s) => s.navigate)
  const select = useSpaceStore((s) => s.select)
  const goBack = useSpaceStore((s) => s.goBack)
  const [data, setData] = useState<ChartNode | null>(null)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    if (currentId === null) return
    let live = true
    const mode = view === 'treemap' ? 'Treemap' : 'Sunburst'
    commands
      .getChartData(scanId, currentId, view === 'treemap' ? 3 : 5, mode)
      .then((d) => live && (setData(d), setError(null)))
      .catch((e) => live && setError(describeError(e)))
    return () => {
      live = false
    }
  }, [scanId, currentId, view])

  if (error) return <p className="p-6 text-bad">{error}</p>
  if (!data) return <p className="p-6 text-muted">Drawing…</p>
  if (data.children.length === 0) {
    return (
      <div className="flex flex-col items-center gap-2 p-16 text-center text-muted">
        <Icon name="folder" size={28} />
        <p>Nothing to draw here. This folder is empty or has no readable content.</p>
      </div>
    )
  }

  return (
    <div className="flex h-full min-h-0 flex-col gap-3 overflow-auto p-5">
      <div className="min-h-0 flex-1">
        {view === 'sunburst' ? (
          <Sunburst
            root={data}
            colorBy={colorBy}
            selectedId={selectedId}
            onZoom={navigate}
            onSelect={select}
            onUp={goBack}
          />
        ) : (
          <Treemap
            root={data}
            colorBy={colorBy}
            selectedId={selectedId}
            onZoom={navigate}
            onSelect={select}
          />
        )}
      </div>
      <ChartLegend root={data} colorBy={colorBy} />
    </div>
  )
}

export default function Space() {
  const phase = useScanStore((s) => s.phase)
  const scanId = useScanStore((s) => s.scanId)
  const view = useSpaceStore((s) => s.view)
  const tab = useSpaceStore((s) => s.tab)
  const colorBy = useSpaceStore((s) => s.colorBy)
  const currentId = useSpaceStore((s) => s.currentId)
  const setView = useSpaceStore((s) => s.setView)
  const setTab = useSpaceStore((s) => s.setTab)
  const setColorBy = useSpaceStore((s) => s.setColorBy)
  const navigateRoute = useNavigate()
  const [crumbName, setCrumbName] = useState<string | null>(null)

  useEffect(() => {
    if (scanId === null || currentId === null) return
    let live = true
    commands
      .getBreadcrumb(scanId, currentId)
      .then((c) => live && setCrumbName(c[c.length - 1]?.name ?? null))
      .catch(() => live && setCrumbName(null))
    return () => {
      live = false
    }
  }, [scanId, currentId])

  const searchHere = () => {
    if (currentId === null) return
    useFindStore.getState().set({ scope: { id: currentId, name: crumbName ?? 'this folder' } })
    navigateRoute('/find')
    useFindStore.getState().requestFocus()
  }

  if (phase === 'idle' || phase === 'running' || scanId === null || currentId === null) {
    return (
      <section className="flex h-full flex-col items-center justify-center gap-4 p-8 text-center">
        <div className="flex h-14 w-14 items-center justify-center rounded-2xl bg-accent-soft text-accent">
          <Icon name="space" size={26} />
        </div>
        <h1 className="text-2xl font-semibold">
          {phase === 'running' ? 'Scanning…' : 'Scan to explore your space'}
        </h1>
        <p className="max-w-sm text-muted">
          {phase === 'running'
            ? 'The map appears as soon as the scan finishes.'
            : 'See your biggest files and folders, and map them as a sunburst or treemap.'}
        </p>
        {phase !== 'running' && <ScanButton large />}
      </section>
    )
  }

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex flex-wrap items-center gap-3 border-b border-line bg-panel px-4 py-2">
        <Segmented<SpaceTab>
          label="What to show"
          value={tab}
          onChange={setTab}
          options={[
            { value: 'folders', label: 'Folders' },
            { value: 'large', label: 'Large files' },
          ]}
        />
        {tab === 'folders' && (
          <>
            <Breadcrumb scanId={scanId} />
            <button
              type="button"
              className="btn btn-ghost h-7 px-2 text-muted"
              onClick={searchHere}
              title="Search inside this folder"
            >
              <Icon name="find" size={13} /> Search here
            </button>
            <div className="ml-auto flex items-center gap-3">
              {view !== 'list' && (
                <Segmented<ColorBy>
                  label="Color by"
                  value={colorBy}
                  onChange={setColorBy}
                  options={[
                    { value: 'kind', label: 'Kind' },
                    { value: 'age', label: 'Age' },
                  ]}
                />
              )}
              <Segmented<SpaceView>
                label="View"
                value={view}
                onChange={setView}
                options={[
                  { value: 'list', label: 'List', icon: 'list' },
                  { value: 'sunburst', label: 'Sunburst', icon: 'sunburst' },
                  { value: 'treemap', label: 'Treemap', icon: 'treemap' },
                ]}
              />
            </div>
          </>
        )}
      </div>
      <div className="min-h-0 flex-1 bg-panel">
        {tab === 'large' ? (
          <LargeFiles scanId={scanId} />
        ) : view === 'list' ? (
          <FolderList scanId={scanId} />
        ) : (
          <Chart scanId={scanId} />
        )}
      </div>
    </div>
  )
}
