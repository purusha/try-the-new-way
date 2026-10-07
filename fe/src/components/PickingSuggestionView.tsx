import type { PickingSuggestion } from '../api/types'
import { fmtDate, fmtQty } from '../lib/format'
import { Badge } from './common'

/** Allocazioni suggerite (POST /operations/picking-suggestions) con eventuale quantità scoperta. */
export function PickingSuggestionView({ suggestion }: { suggestion: PickingSuggestion }) {
  if (suggestion.lines.length === 0) return <p className="empty">Nulla da prelevare.</p>
  return (
    <div className="suggestion">
      {suggestion.lines.map((line, i) => (
        <div key={`${line.product_id}-${line.sales_order_line_id ?? i}`} className="suggestion-line">
          <h3>
            {line.sku} — richiesti {fmtQty(line.requested)}, allocati {fmtQty(line.allocated)}{' '}
            <Badge tone="info">{line.strategy_applied}</Badge>{' '}
            {line.shortfall > 0 && <Badge tone="danger">Mancano {fmtQty(line.shortfall)}</Badge>}
          </h3>
          {line.picks.length === 0 ? (
            <p className="empty">Nessuna giacenza utilizzabile.</p>
          ) : (
            <table className="table compact">
              <thead>
                <tr>
                  <th>Ubicazione</th>
                  <th>Lotto</th>
                  <th>Scadenza</th>
                  <th className="num">Quantità</th>
                  <th>Seriali</th>
                </tr>
              </thead>
              <tbody>
                {line.picks.map((pick) => (
                  <tr key={`${pick.location_id}-${pick.lot_id ?? ''}`}>
                    <td>{pick.location_code}</td>
                    <td>{pick.lot_code ?? '—'}</td>
                    <td>{fmtDate(pick.expiry_date)}</td>
                    <td className="num">{fmtQty(pick.quantity)}</td>
                    <td className="serials">{pick.serials?.join(', ') || '—'}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </div>
      ))}
    </div>
  )
}
