import { Link } from 'react-router'
import type { Shipment } from '../api/types'
import { fmtDateTime, fmtQty } from '../lib/format'
import { Empty } from './common'

export function ShipmentsTable({ shipments }: { shipments: Shipment[] }) {
  if (shipments.length === 0) return <Empty>Nessuna spedizione a clienti.</Empty>
  return (
    <table className="table">
      <thead>
        <tr>
          <th>Data</th>
          <th>Ordine</th>
          <th>Cliente</th>
          <th>Rif. esterno</th>
          <th className="num">Quantità</th>
        </tr>
      </thead>
      <tbody>
        {shipments.map((s) => (
          <tr key={s.movement_id}>
            <td>{fmtDateTime(s.occurred_at)}</td>
            <td>
              {s.sales_order_id ? (
                <Link to={`/sales-orders/${s.sales_order_id}`}>{s.sales_order_number ?? 'ordine'}</Link>
              ) : (
                '—'
              )}
            </td>
            <td>{s.customer_name ?? '—'}</td>
            <td>{s.external_ref ?? '—'}</td>
            <td className="num">{fmtQty(s.quantity)}</td>
          </tr>
        ))}
      </tbody>
    </table>
  )
}
