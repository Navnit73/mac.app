import type { SVGProps } from 'react'

const PATHS = {
  overview: 'M4 13a8 8 0 1 1 16 0M12 13l4-4M6.5 17.5h11',
  find: 'M11 18a7 7 0 1 0 0-14 7 7 0 0 0 0 14ZM20 20l-4-4',
  space: 'M12 3v9h9M12 3a9 9 0 1 0 9 9',
  cleanup: 'M5 19 19 5M14 4l1.5 3L19 8.5 15.5 10 14 13l-1.5-3L9 8.5 12.5 7ZM6 14l.8 1.7L8.5 16.5 6.8 17.3 6 19l-.8-1.7L3.5 16.5l1.7-.8Z',
  duplicates: 'M9 9h10v10H9zM5 15V5h10',
  applications: 'M4 4h6v6H4zM14 4h6v6h-6zM4 14h6v6H4zM14 14h6v6h-6z',
  monitor: 'M3 12h4l2.5-7 4 14 2.5-7h5',
  compress: 'M4 14h6v6M20 10h-6V4M14 10l7-7M10 14l-7 7',
  activity: 'M12 7v5l3 2M21 12a9 9 0 1 1-3-6.7M21 4v5h-5',
  dev: 'm8 8-4 4 4 4M16 8l4 4-4 4M13.5 5l-3 14',
  folder: 'M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2Z',
  file: 'M7 3h7l5 5v13H7zM14 3v5h5',
  package: 'm12 3 8 4.5v9L12 21l-8-4.5v-9ZM4 7.5l8 4.5 8-4.5M12 12v9',
  chevron: 'm9 6 6 6-6 6',
  back: 'm15 6-6 6 6 6',
  forward: 'm9 6 6 6-6 6',
  close: 'M6 6l12 12M18 6 6 18',
  lock: 'M7 11V8a5 5 0 0 1 10 0v3M6 11h12v9H6z',
  info: 'M12 11v5M12 8h.01M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18Z',
  reveal: 'M14 4h6v6M20 4l-9 9M18 14v5H5V6h5',
  eye: 'M2 12s4-7 10-7 10 7 10 7-4 7-10 7S2 12 2 12ZM12 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6Z',
  copy: 'M9 9h11v11H9zM5 15V4h11',
  ban: 'M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18ZM5.6 5.6l12.8 12.8',
  trash: 'M5 7h14M10 7V4h4v3M7 7l1 13h8l1-13',
  shield: 'M12 3 5 6v5c0 4.5 3 8 7 10 4-2 7-5.5 7-10V6Z',
  check: 'm5 12 5 5 9-10',
  warning: 'M12 4 3 20h18ZM12 10v4M12 17h.01',
  cloud: 'M7 18a4 4 0 0 1-.6-7.96A6 6 0 0 1 18 9.5 4.3 4.3 0 0 1 17 18Z',
  list: 'M8 6h12M8 12h12M8 18h12M4 6h.01M4 12h.01M4 18h.01',
  sunburst: 'M12 12m-2 0a2 2 0 1 0 4 0 2 2 0 1 0-4 0M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18ZM12 7a5 5 0 1 0 0 10 5 5 0 0 0 0-10Z',
  treemap: 'M3 4h10v9H3zM15 4h6v5h-6zM15 11h6v9h-6zM3 15h10v5H3z',
  sort: 'M8 5v14M5 16l3 3 3-3M16 19V5M13 8l3-3 3 3',
} as const

export type IconName = keyof typeof PATHS

interface Props extends Omit<SVGProps<SVGSVGElement>, 'name'> {
  name: IconName
  size?: number
}

export default function Icon({ name, size = 16, ...rest }: Props) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.8}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      {...rest}
    >
      <path d={PATHS[name]} />
    </svg>
  )
}
