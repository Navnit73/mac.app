export interface Rect {
  x: number
  y: number
  w: number
  h: number
}

export interface Placed<T> {
  item: T
  rect: Rect
}

/**
 * Squarified treemap layout (Bruls, Huizing, van Wijk): lays items out in rows along the shorter
 * side so tiles stay close to square. Areas are proportional to `value`.
 */
export function squarify<T extends { value: number }>(items: T[], rect: Rect): Placed<T>[] {
  const total = items.reduce((a, i) => a + Math.max(i.value, 0), 0)
  if (total <= 0 || rect.w <= 0 || rect.h <= 0) return []
  const scale = (rect.w * rect.h) / total
  const nodes = items
    .filter((i) => i.value > 0)
    .sort((a, b) => b.value - a.value)
    .map((item) => ({ item, area: item.value * scale }))

  const out: Placed<T>[] = []
  let { x, y, w, h } = rect

  const worst = (row: { area: number }[], side: number): number => {
    const sum = row.reduce((a, r) => a + r.area, 0)
    let max = 0
    let min = Infinity
    for (const r of row) {
      max = Math.max(max, r.area)
      min = Math.min(min, r.area)
    }
    return Math.max((side * side * max) / (sum * sum), (sum * sum) / (side * side * min))
  }

  const layoutRow = (row: typeof nodes) => {
    const sum = row.reduce((a, r) => a + r.area, 0)
    if (w >= h) {
      // Column on the left, items stacked top to bottom.
      const colW = sum / h
      let cy = y
      for (const r of row) {
        const ih = r.area / colW
        out.push({ item: r.item, rect: { x, y: cy, w: colW, h: ih } })
        cy += ih
      }
      x += colW
      w -= colW
    } else {
      // Row on top, items side by side.
      const rowH = sum / w
      let cx = x
      for (const r of row) {
        const iw = r.area / rowH
        out.push({ item: r.item, rect: { x: cx, y, w: iw, h: rowH } })
        cx += iw
      }
      y += rowH
      h -= rowH
    }
  }

  let row: typeof nodes = []
  let i = 0
  while (i < nodes.length) {
    const next = nodes[i]
    if (!next) break
    const side = Math.min(w, h)
    const candidate = [...row, next]
    if (row.length === 0 || worst(candidate, side) <= worst(row, side)) {
      row = candidate
      i++
    } else {
      layoutRow(row)
      row = []
    }
  }
  if (row.length > 0) layoutRow(row)
  return out
}
