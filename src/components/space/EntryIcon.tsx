import { KIND_COLOR } from '../../lib/colors'
import type { FileKind } from '../../types/bindings/FileKind'
import Icon from '../Icon'

/** Tinted tile with a folder, package or file glyph in the kind's color. */
export default function EntryIcon({
  kind,
  isDir,
  isPackage,
  large = false,
}: {
  kind: FileKind
  isDir: boolean
  isPackage: boolean
  /** 32px tile for roomy lists like Find; 24px otherwise. */
  large?: boolean
}) {
  const color = KIND_COLOR[kind]
  return (
    <span
      className={`flex shrink-0 items-center justify-center ${large ? 'h-8 w-8 rounded-lg' : 'h-6 w-6 rounded-md'}`}
      style={{ background: `color-mix(in srgb, ${color} 18%, transparent)`, color }}
    >
      <Icon name={isPackage ? 'package' : isDir ? 'folder' : 'file'} size={large ? 16 : 14} />
    </span>
  )
}
