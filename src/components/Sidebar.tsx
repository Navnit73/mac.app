import { NavLink } from 'react-router-dom'
import { ROOMS } from '../rooms'
import Icon, { type IconName } from './Icon'

const link = ({ isActive }: { isActive: boolean }) =>
  `group relative flex items-center gap-3 rounded-lg px-3 py-2 transition-colors ${
    isActive ? 'bg-selected font-semibold text-fg' : 'text-muted hover:bg-hover hover:text-fg'
  }`

/** Lime marker and icon tint for the current room. */
function NavItem({ to, icon, label }: { to: string; icon: IconName; label: string }) {
  return (
    <NavLink to={to} className={link}>
      {({ isActive }) => (
        <>
          {isActive && (
            <span
              aria-hidden="true"
              className="absolute top-1.5 bottom-1.5 -left-2.5 w-[3px] rounded-r-full bg-accent"
            />
          )}
          <Icon name={icon} className={isActive ? 'text-accent' : ''} />
          <span>{label}</span>
        </>
      )}
    </NavLink>
  )
}

export default function Sidebar() {
  return (
    <nav
      aria-label="Rooms"
      className="surface-black flex w-56 shrink-0 flex-col border-r border-line bg-sidebar"
    >
      {/* Room for the window's traffic lights; dragging here moves the window. */}
      <div data-tauri-drag-region className="h-[52px] shrink-0" />
      <div data-tauri-drag-region className="flex items-center gap-2.5 px-5 pb-6">
        <span className="flex h-7 w-7 items-center justify-center rounded-lg bg-accent text-accent-fg">
          <Icon name="bolt" size={16} strokeWidth={2.2} />
        </span>
        <span className="font-display text-[15px] font-semibold tracking-tight">MyMacStorage</span>
      </div>
      <div className="eyebrow px-5 pb-2">Rooms</div>
      <ul className="flex flex-col gap-0.5 px-2.5">
        {ROOMS.map((room) => (
          <li key={room.path}>
            <NavItem to={room.path} icon={room.icon} label={room.label} />
          </li>
        ))}
      </ul>
      <div className="mt-auto border-t border-line px-2.5 py-3">
        <NavItem to="/dev" icon="dev" label="Developer" />
      </div>
    </nav>
  )
}
