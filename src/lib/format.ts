const UNITS = ['B', 'KB', 'MB', 'GB', 'TB']

/** Decimal units, matching Finder and System Settings. */
export function formatBytes(n: number): string {
  let v = n
  let i = 0
  while (v >= 1000 && i < UNITS.length - 1) {
    v /= 1000
    i++
  }
  return `${i === 0 ? v : v.toFixed(v >= 100 ? 0 : 1)} ${UNITS[i]}`
}

export function formatCount(n: number): string {
  return n.toLocaleString('en-US')
}

export function formatDuration(ms: number): string {
  const s = Math.floor(ms / 1000)
  return s < 60 ? `${(ms / 1000).toFixed(1)}s` : `${Math.floor(s / 60)}m ${s % 60}s`
}

export function formatPercent(f: number): string {
  if (f > 0 && f < 0.001) return '<0.1%'
  return `${(f * 100).toFixed(f < 0.1 ? 1 : 0)}%`
}

export function formatDate(unixSeconds: number | null | undefined): string {
  if (!unixSeconds) return '—'
  return new Date(unixSeconds * 1000).toLocaleDateString('en-US', {
    year: 'numeric',
    month: 'short',
    day: 'numeric',
  })
}

/** "3 months ago" style label, for the Inspector and list rows. */
export function formatAge(unixSeconds: number | null | undefined, nowMs = Date.now()): string {
  if (!unixSeconds) return '—'
  const days = Math.floor((nowMs / 1000 - unixSeconds) / 86400)
  if (days < 1) return 'Today'
  if (days < 2) return 'Yesterday'
  if (days < 30) return `${days} days ago`
  if (days < 365) return `${Math.floor(days / 30)} mo ago`
  const y = Math.floor(days / 365)
  return `${y} yr${y > 1 ? 's' : ''} ago`
}
