import { Link } from 'react-router'
import type { Movement } from '../api/types'
import { isReversible } from '../lib/describe'
import { fmtDateTime, fmtQty } from '../lib/format'
import { movementTypeLabel } from '../lib/labels'
import { Badge, Empty } from './common'

function typeText(m: Movement): string {
  const label = movementTypeLabel[m.type]
  if (m.type === 'ADJUSTMENT' && m.direction) return `${label} ${m.direction === 'IN' ? '(+)' : '(−)'}`
  return label
}

function orderRef(m: Movement) {
  if (m.sales_order_id && m.sales_order_number) {
    return <Link to={`/sales-orders/${m.sales_order_id}`}>{m.sales_order_number}</Link>
  }
  return m.purchase_order_number ?? m.external_ref ?? '—'
}

interface Props {
  movements: Movement[]
  onReverse?: (movement: Movement) => void
  emptyText?: string
}

export function MovementsTable({ movements, onReverse, emptyText }: Props) {
  if (movements.length === 0) return <Empty>{emptyText ?? 'Nessun movimento.'}</Empty>
  return (
    <div className="table-wrap">
      <table className="table">
        <thead>
          <tr>
            <th>N°</th>
            <th>Data</th>
            <th>Tipo</th>
            <th>SKU</th>
            <th>Lotto</th>
            <th>Da</th>
            <th>A</th>
            <th className="num">Quantità</th>
            <th>Causale</th>
            <th>Ordine / rif.</th>
            <th>Operatore</th>
            <th>Note</th>
            {onReverse && <th />}
          </tr>
        </thead>
        <tbody>
          {movements.map((m) => (
            <tr key={m.id}>
              <td>{m.number}</td>
              <td className="nowrap">{fmtDateTime(m.occurred_at)}</td>
              <td className="nowrap">{typeText(m)}</td>
              <td>{m.sku}</td>
              <td>{m.lot_id && m.lot_code ? <Link to={`/lots/${m.lot_id}`}>{m.lot_code}</Link> : '—'}</td>
              <td>{m.from_location_code ?? '—'}</td>
              <td>{m.to_location_code ?? '—'}</td>
              <td className="num">
                {fmtQty(m.quantity)}
                {m.serials.length > 0 && (
                  <div className="tiny muted" title={m.serials.join(', ')}>
                    {m.serials.length} seriali
                  </div>
                )}
              </td>
              <td>{m.reason_code}</td>
              <td>{orderRef(m)}</td>
              <td>{m.operator}</td>
              <td className="flags">
                {m.reverses_movement_id && <Badge tone="info">Storno</Badge>}
                {m.reversed_by_movement_id && <Badge tone="warn">Stornato</Badge>}
                {m.fefo_override_reason && (
                  <Badge tone="warn" title={m.fefo_override_reason}>
                    Deroga FEFO
                  </Badge>
                )}
                {m.note && <span className="tiny muted">{m.note}</span>}
              </td>
              {onReverse && (
                <td>
                  {isReversible(m) && (
                    <button type="button" className="small" onClick={() => onReverse(m)}>
                      Storna
                    </button>
                  )}
                </td>
              )}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  )
}
