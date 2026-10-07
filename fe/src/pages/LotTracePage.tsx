import { Link, useParams } from 'react-router'
import { api, operatorHeader } from '../api/client'
import { unwrap } from '../api/errors'
import { Badge, Empty, Loading, PageHeader } from '../components/common'
import { MovementsTable } from '../components/MovementsTable'
import { ProblemAlert } from '../components/ProblemAlert'
import { ShipmentsTable } from '../components/ShipmentsTable'
import { useAsync } from '../hooks/useAsync'
import { useSubmit } from '../hooks/useSubmit'
import { fmtDate, fmtDateTime, fmtQty } from '../lib/format'
import { lotStatusLabel } from '../lib/labels'

export function LotTracePage() {
  const { id = '' } = useParams()
  const trace = useAsync(() => unwrap(api.GET('/lots/{lotId}/trace', { params: { path: { lotId: id } } })), [id])
  const toggle = useSubmit()

  if (trace.loading && !trace.data) return <Loading />
  if (trace.error && !trace.data) return <ProblemAlert problem={trace.error} />
  if (!trace.data) return null
  const { lot, totals, current_stock, movements, shipments } = trace.data

  const setStatus = async (status: 'ACTIVE' | 'BLOCKED') => {
    const done = await toggle.run(() =>
      unwrap(
        api.PATCH('/lots/{lotId}', {
          params: { path: { lotId: id }, header: operatorHeader() },
          body: { status },
        }),
      ),
    )
    if (done) trace.reload()
  }

  return (
    <>
      <PageHeader title={`Lotto ${lot.lot_code}`}>
        <Link to="/trace">← Tracciabilità</Link>
      </PageHeader>

      <section className="card">
        <div className="card-head">
          <h2>
            <Link to={`/products/${lot.product_id}`}>{lot.sku}</Link>{' '}
            <Badge tone={lot.status === 'BLOCKED' ? 'danger' : 'ok'}>{lotStatusLabel[lot.status]}</Badge>{' '}
            {lot.expired && <Badge tone="danger">Scaduto</Badge>}
          </h2>
          {lot.status === 'ACTIVE' ? (
            <button type="button" className="danger" disabled={toggle.busy} onClick={() => setStatus('BLOCKED')}>
              Blocca lotto
            </button>
          ) : (
            <button type="button" disabled={toggle.busy} onClick={() => setStatus('ACTIVE')}>
              Sblocca lotto
            </button>
          )}
        </div>
        <ProblemAlert problem={toggle.problem} onDismiss={() => toggle.setProblem(null)} />
        <dl className="props">
          <div>
            <dt>Produzione</dt>
            <dd>{fmtDate(lot.production_date)}</dd>
          </div>
          <div>
            <dt>Scadenza</dt>
            <dd>
              {fmtDate(lot.expiry_date)}
              {lot.days_to_expiry != null && ` (${lot.days_to_expiry} giorni)`}
            </dd>
          </div>
          <div>
            <dt>Primo ricevimento</dt>
            <dd>{fmtDateTime(lot.received_at)}</dd>
          </div>
          <div>
            <dt>Ricevuto</dt>
            <dd>{fmtQty(totals.received)}</dd>
          </div>
          <div>
            <dt>Spedito</dt>
            <dd>{fmtQty(totals.shipped)}</dd>
          </div>
          <div>
            <dt>Rettifiche + / −</dt>
            <dd>
              {fmtQty(totals.adjusted_in)} / {fmtQty(totals.adjusted_out)}
            </dd>
          </div>
          <div>
            <dt>In giacenza</dt>
            <dd>
              <strong>{fmtQty(totals.on_hand)}</strong>
            </dd>
          </div>
        </dl>
      </section>

      <section className="card">
        <h2>Giacenza attuale</h2>
        {current_stock.length === 0 ? (
          <Empty>Nessuna giacenza.</Empty>
        ) : (
          <table className="table">
            <thead>
              <tr>
                <th>Ubicazione</th>
                <th className="num">Quantità</th>
                <th>Utilizzabile</th>
              </tr>
            </thead>
            <tbody>
              {current_stock.map((row) => (
                <tr key={row.id}>
                  <td>{row.location_code}</td>
                  <td className="num">{fmtQty(row.quantity, row.uom)}</td>
                  <td>{row.usable ? 'Sì' : 'No'}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </section>

      <section className="card">
        <h2>Spedizioni a clienti</h2>
        <ShipmentsTable shipments={shipments} />
      </section>

      <section className="card">
        <h2>Movimenti (dal più vecchio)</h2>
        <MovementsTable movements={movements} />
      </section>
    </>
  )
}
