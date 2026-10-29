import type { AgeBucket } from '../types/bindings/AgeBucket'
import type { CategoryId } from '../types/bindings/CategoryId'
import type { FileKind } from '../types/bindings/FileKind'

/** Data colors are CSS variables so light and dark mode swap in one place. */
export const KIND_COLOR: Record<FileKind, string> = {
  Video: 'var(--s1)',
  Images: 'var(--s2)',
  Audio: 'var(--s3)',
  Archives: 'var(--s4)',
  Documents: 'var(--s5)',
  Code: 'var(--s6)',
  Apps: 'var(--s7)',
  DiskImages: 'var(--s8)',
  Folder: 'var(--folder)',
  Other: 'var(--other)',
}

export const KIND_LABEL: Record<FileKind, string> = {
  Video: 'Video',
  Images: 'Images',
  Audio: 'Audio',
  Archives: 'Archives',
  Documents: 'Documents',
  Code: 'Code',
  Apps: 'Apps',
  DiskImages: 'Disk images',
  Folder: 'Folders',
  Other: 'Other',
}

/** Display order for legends. */
export const KIND_ORDER: FileKind[] = [
  'Video',
  'Images',
  'Audio',
  'Archives',
  'Documents',
  'Code',
  'Apps',
  'DiskImages',
  'Folder',
  'Other',
]

/** Age is ordered, so it uses one hue (blue) from light to dark. */
export const AGE_COLOR: Record<AgeBucket, string> = {
  Recent: 'color-mix(in srgb, var(--s1) 38%, var(--panel))',
  ThisYear: 'color-mix(in srgb, var(--s1) 70%, var(--panel))',
  Old: 'color-mix(in srgb, var(--s1) 100%, #000 22%)',
}

export const AGE_LABEL: Record<AgeBucket, string> = {
  Recent: 'Under 30 days',
  ThisYear: '30 days to 1 year',
  Old: 'Over 1 year',
}

export const CATEGORY_COLOR: Record<CategoryId, string> = {
  Developer: 'var(--s1)',
  OldDownloads: 'var(--s2)',
  Caches: 'var(--s3)',
  Logs: 'var(--s4)',
  LargeOld: 'var(--s5)',
  Duplicates: 'var(--s6)',
  Leftovers: 'var(--s7)',
}

/** Where a category's "Review" action will live. */
export const CATEGORY_ROUTE: Record<CategoryId, string> = {
  Developer: '/cleanup',
  OldDownloads: '/cleanup',
  Caches: '/cleanup',
  Logs: '/cleanup',
  LargeOld: '/space',
  Duplicates: '/duplicates',
  Leftovers: '/applications',
}
