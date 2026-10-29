import { KIND_COLOR } from '../../lib/colors'
import type { FileKind } from '../../types/bindings/FileKind'
import Icon from '../Icon'

/** Tinted tile with a folder, package or file glyph in the kind's color. */
export default function EntryIcon({
  kind,
  isDir,
  isPackage,
}: {
  kind: FileKind
  isDir: boolean
  isPackage: boolean
}) {
  const color = KIND_COLOR[kind]
  return (
    <span
      className="flex h-6 w-6 shrink-0 items-center justify-center rounded-md"
      style={{ background: `color-mix(in srgb, ${color} 22%, transparent)`, color }}
    >
      <Icon name={isPackage ? 'package' : isDir ? 'folder' : 'file'} size={14} />
    </span>
  )
}
