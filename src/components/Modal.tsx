import { useEffect, useRef, type ReactNode } from 'react'

/** Native modal dialog: focus trap, Escape to close, backdrop. */
export default function Modal({
  open,
  onClose,
  labelledBy,
  width = 680,
  children,
}: {
  open: boolean
  onClose: () => void
  labelledBy: string
  width?: number
  children: ReactNode
}) {
  const ref = useRef<HTMLDialogElement>(null)
  useEffect(() => {
    const d = ref.current
    if (!d) return
    if (open && !d.open) {
      d.showModal()
      // Start on the button marked as the safe default (usually Cancel).
      d.querySelector<HTMLElement>('[data-autofocus]')?.focus()
    }
    if (!open && d.open) d.close()
  }, [open])
  return (
    <dialog
      ref={ref}
      aria-labelledby={labelledBy}
      onCancel={(e) => {
        e.preventDefault()
        onClose()
      }}
      className="m-auto max-w-[92vw] rounded-2xl border border-line bg-panel p-0 text-fg backdrop:bg-neutral-950/50 backdrop:backdrop-blur-[2px]"
      style={{ width }}
    >
      {open && children}
    </dialog>
  )
}
