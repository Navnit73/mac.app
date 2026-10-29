import type { IconName } from './components/Icon'

export interface Room {
  path: string
  label: string
  blurb: string
  icon: IconName
  /** Milestone in which the room becomes functional; undefined once built. */
  milestone?: string
}

export const ROOMS: readonly Room[] = [
  {
    path: '/overview',
    label: 'Overview',
    blurb: 'The whole drive on one screen',
    icon: 'overview',
  },
  {
    path: '/find',
    label: 'Find',
    blurb: 'Any file, found as you type',
    icon: 'find',
  },
  { path: '/space', label: 'Space', blurb: 'The heavy things come to the top', icon: 'space' },
  {
    path: '/cleanup',
    label: 'Cleanup',
    blurb: 'Caches, build files and old installers, grouped and totalled',
    icon: 'cleanup',
    milestone: 'M6',
  },
  {
    path: '/duplicates',
    label: 'Duplicates',
    blurb: 'Files that exist twice, matched byte for byte',
    icon: 'duplicates',
    milestone: 'M5',
  },
  {
    path: '/applications',
    label: 'Applications',
    blurb: 'Every app with everything it left behind',
    icon: 'applications',
    milestone: 'M6',
  },
  {
    path: '/monitor',
    label: 'Monitor',
    blurb: 'CPU, memory, ports and battery, live',
    icon: 'monitor',
    milestone: 'M7',
  },
  {
    path: '/compress',
    label: 'Compress',
    blurb: 'Shrink videos and photos instead of deleting them',
    icon: 'compress',
    milestone: 'M8',
  },
  {
    path: '/activity',
    label: 'Activity',
    blurb: 'The space you got back, week by week',
    icon: 'activity',
  },
] as const
