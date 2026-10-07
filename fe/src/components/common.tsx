import type { ReactNode } from 'react'
import { Link } from 'react-router'
import { useApp } from '../state/appContext'

type Tone = 'neutral' | 'ok' | 'warn' | 'danger' | 'info'

export function Badge({ tone = 'neutral', children, title }: { tone?: Tone; children: ReactNode; title?: string }) {
  return (
    <span className={`badge badge-${tone}`} title={title}>
      {children}
    </span>
  )
}

export function Loading({ what = 'Caricamento…' }: { what?: string }) {
  return <p className="muted">{what}</p>
}

export function Empty({ children }: { children: ReactNode }) {
  return <p className="empty">{children}</p>
}

interface PaginationProps {
  page: number
  pageSize: number
  total: number
  onPage: (page: number) => void
}

export function Pagination({ page, pageSize, total, onPage }: PaginationProps) {
  const pages = Math.max(1, Math.ceil(total / pageSize))
  return (
    <div className="pagination">
      <button type="button" disabled={page <= 1} onClick={() => onPage(page - 1)}>
        ‹ Precedente
      </button>
      <span>
        Pagina {page} di {pages} · {total} elementi
      </span>
      <button type="button" disabled={page >= pages} onClick={() => onPage(page + 1)}>
        Successiva ›
      </button>
    </div>
  )
}

/** Avviso visibile nelle pagine di scrittura quando l'operatore non è impostato. */
export function OperatorNotice() {
  const { operator } = useApp()
  if (operator) return null
  return (
    <div className="alert alert-warn">
      Per registrare operazioni imposta il <strong>nome dell'operatore</strong> nell'intestazione.
    </div>
  )
}

/** Mostra il contenuto solo se è selezionato un magazzino. */
export function RequireWarehouse({ children }: { children: ReactNode }) {
  const { warehouseId, warehousesLoading } = useApp()
  if (warehouseId) return <>{children}</>
  if (warehousesLoading) return <Loading />
  return (
    <div className="alert alert-warn">
      Nessun magazzino selezionato. Creane uno nella pagina <Link to="/warehouse">Magazzino</Link> e selezionalo
      nell'intestazione.
    </div>
  )
}

export function PageHeader({ title, children }: { title: string; children?: ReactNode }) {
  return (
    <div className="page-header">
      <h1>{title}</h1>
      {children && <div className="page-actions">{children}</div>}
    </div>
  )
}
