import { NavLink } from 'react-router-dom'
import { ROOMS } from '../rooms'
import Icon from './Icon'

const link = ({ isActive }: { isActive: boolean }) =>
  `group flex items-center gap-2.5 rounded-lg px-2.5 py-1.5 transition-colors ${
    isActive ? 'bg-selected font-semibold text-fg' : 'text-muted hover:bg-hover hover:text-fg'
  }`

export default function Sidebar() {
  return (
    <nav aria-label="Rooms" className="flex w-52 shrink-0 flex-col border-r border-line bg-sidebar">
      <div data-tauri-drag-region className="h-[52px] shrink-0" />
      <div className="eyebrow px-5 pb-1.5">Rooms</div>
      <ul className="flex flex-col gap-0.5 px-2.5">
        {ROOMS.map((room) => (
          <li key={room.path}>
            <NavLink to={room.path} className={link}>
              <Icon name={room.icon} />
              <span>{room.label}</span>
            </NavLink>
          </li>
        ))}
      </ul>
      <div className="mt-auto px-2.5 pb-3">
        <NavLink to="/dev" className={link}>
          <Icon name="dev" />
          <span>Developer</span>
        </NavLink>
      </div>
    </nav>
  )
}
