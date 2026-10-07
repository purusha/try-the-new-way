import { Link } from 'react-router'
import type { Alert, AlertSeverity, AlertType } from '../api/types'
import { fmtDate, fmtQty } from '../lib/format'
import { alertTypeLabel, severityLabel } from '../lib/labels'
import { Badge } from './common'

const severityTone: Record<AlertSeverity, 'info' | 'warn' | 'danger'> = {
  INFO: 'info',
  WARNING: 'warn',
  CRITICAL: 'danger',
}

function AlertItem({ alert }: { alert: Alert }) {
  return (
    <li>
      <Badge tone={severityTone[alert.severity]}>{severityLabel[alert.severity]}</Badge>{' '}
      <Link to={`/products/${alert.product_id}`}>{alert.sku}</Link>
      {alert.lot_id && alert.lot_code && (
        <>
          {' · lotto '}
          <Link to={`/lots/${alert.lot_id}`}>{alert.lot_code}</Link>
        </>
      )}
      {alert.expiry_date && ` · scad. ${fmtDate(alert.expiry_date)}`}
      {alert.quantity != null && ` · q.tà ${fmtQty(alert.quantity)}`}
      {alert.threshold != null && ` · soglia ${fmtQty(alert.threshold)}`}
      {alert.warehouse_code && ` · ${alert.warehouse_code}`}
      <div className="muted">{alert.message}</div>
    </li>
  )
}

/** Elenco semplice di alert (es. nelle risposte delle operazioni). */
export function AlertsList({ alerts }: { alerts: Alert[] }) {
  if (alerts.length === 0) return null
  return (
    <ul className="alert-list">
      {alerts.map((a, i) => (
        <AlertItem key={`${a.type}-${a.product_id}-${a.lot_id ?? ''}-${a.warehouse_id ?? ''}-${i}`} alert={a} />
      ))}
    </ul>
  )
}

const typeOrder: AlertType[] = ['RESERVATION_AT_RISK', 'EXPIRED', 'EXPIRING', 'LOW_STOCK']

/** Alert raggruppati per tipo. */
export function GroupedAlerts({ alerts }: { alerts: Alert[] }) {
  if (alerts.length === 0) return <p className="empty">Nessun alert.</p>
  return (
    <div className="alert-groups">
      {typeOrder.map((type) => {
        const group = alerts.filter((a) => a.type === type)
        if (group.length === 0) return null
        return (
          <section key={type}>
            <h3>
              {alertTypeLabel[type]} <span className="muted">({group.length})</span>
            </h3>
            <AlertsList alerts={group} />
          </section>
        )
      })}
    </div>
  )
}
