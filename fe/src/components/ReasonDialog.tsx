import { type FormEvent, type ReactNode, useEffect, useRef, useState } from 'react'

interface Props {
  open: boolean
  title: string
  children?: ReactNode
  label?: string
  confirmLabel?: string
  /** Lunghezza minima della motivazione; 0 = facoltativa. */
  minLength?: number
  busy?: boolean
  onConfirm: (reason: string) => void
  onCancel: () => void
}

/** Finestra modale che chiede una motivazione (deroga FEFO, storno, annullamento). */
export function ReasonDialog({
  open,
  title,
  children,
  label = 'Motivazione',
  confirmLabel = 'Conferma',
  minLength = 3,
  busy,
  onConfirm,
  onCancel,
}: Props) {
  const ref = useRef<HTMLDialogElement>(null)
  const [reason, setReason] = useState('')

  useEffect(() => {
    const dialog = ref.current
    if (!dialog) return
    if (open && !dialog.open) {
      setReason('')
      dialog.showModal()
    } else if (!open && dialog.open) {
      dialog.close()
    }
  }, [open])

  const submit = (e: FormEvent) => {
    e.preventDefault()
    if (reason.trim().length < minLength) return
    onConfirm(reason.trim())
  }

  return (
    <dialog
      ref={ref}
      className="dialog"
      onCancel={(e) => {
        e.preventDefault()
        onCancel()
      }}
    >
      <form onSubmit={submit}>
        <h2>{title}</h2>
        {children}
        <label className="field">
          <span>
            {label}
            {minLength > 0 ? ` (almeno ${minLength} caratteri)` : ' (facoltativa)'}
          </span>
          <textarea value={reason} rows={3} onChange={(e) => setReason(e.target.value)} autoFocus />
        </label>
        <div className="actions">
          <button type="button" onClick={onCancel} disabled={busy}>
            Annulla
          </button>
          <button type="submit" className="primary" disabled={busy || reason.trim().length < minLength}>
            {confirmLabel}
          </button>
        </div>
      </form>
    </dialog>
  )
}
