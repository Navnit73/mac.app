/** Split `name` into [before, match, after] for highlighting, or null if it does not match. */
export function highlightParts(
  name: string,
  query: string,
  prefixOnly = false,
): [string, string, string] | null {
  const q = query.normalize('NFC').toLowerCase()
  if (!q) return null
  const shown = name.normalize('NFC')
  const lower = shown.toLowerCase()
  // Lower-casing can change length for a few scripts; skip highlighting rather than misplace it.
  if (lower.length !== shown.length) return null
  const at = lower.indexOf(q)
  if (at < 0 || (prefixOnly && at !== 0)) return null
  return [shown.slice(0, at), shown.slice(at, at + q.length), shown.slice(at + q.length)]
}

/** Keep the start and end of a long path, which carry the most meaning. */
export function middleTruncate(s: string, max: number): string {
  if (s.length <= max) return s
  const keep = max - 1
  const head = Math.ceil(keep * 0.4)
  return `${s.slice(0, head)}…${s.slice(s.length - (keep - head))}`
}

/** Replace the user's home folder with `~`. */
export function tildify(p: string | null | undefined): string {
  if (!p) return ''
  return p.replace(/^\/Users\/[^/]+/, '~')
}
