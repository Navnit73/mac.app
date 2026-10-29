import type { Room } from '../rooms'
import Icon from './Icon'

export default function RoomPlaceholder({ room }: { room: Room }) {
  return (
    <section className="flex h-full flex-col items-center justify-center gap-3 p-8 text-center">
      <div className="flex h-14 w-14 items-center justify-center rounded-2xl bg-accent-soft text-accent">
        <Icon name={room.icon} size={26} />
      </div>
      <h1 className="text-2xl font-semibold">{room.label}</h1>
      <p className="max-w-sm text-muted">{room.blurb}.</p>
      {room.milestone && (
        <span className="chip" title="Planned in the build roadmap">
          Arrives in {room.milestone}
        </span>
      )}
    </section>
  )
}
