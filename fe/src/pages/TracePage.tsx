import { useState } from 'react'
import { Link } from 'react-router'
import { api } from '../api/client'
import { unwrap } from '../api/errors'
import { Badge, Empty, Loading, PageHeader } from '../components/common'
import { ProblemAlert } from '../components/ProblemAlert'
import { useAsync } from '../hooks/useAsync'
import { useDebounced } from '../hooks/useDebounced'
import { fmtDate, fmtQty } from '../lib/format'
import { lotStatusLabel, serialStatusLabel } from '../lib/labels'

function LotSearch() {
  const [query, setQuery] = useState('')
  const q = useDebounced(query.trim())
  const lots = useAsync(
    () => unwrap(api.GET('/lots', { params: { query: { lot_code: q, page_size: 50 } } })),
    [q],
    q.length > 0,
  )
  return (
    <section className="card">
      <h2>Lotti</h2>
      <input type="search" placeholder="Codice lotto (ricerca parziale)" value={query} onChange={(e) => setQuery(e.target.value)} />
      <ProblemAlert problem={lots.error} />
      {q && lots.loading && <Loading />}
      {q && lots.data && lots.data.items.length === 0 && <Empty>Nessun lotto.</Empty>}
      {q && lots.data && lots.data.items.length > 0 && (
        <table className="table">
          <thead>
            <tr>
              <th>Lotto</th>
              <th>SKU</th>
              <th>Scadenza</th>
              <th>Stato</th>
              <th className="num">Giacenza</th>
            </tr>
          </thead>
          <tbody>
            {lots.data.items.map((lot) => (
              <tr key={lot.id}>
                <td>
                  <Link to={`/lots/${lot.id}`}>{lot.lot_code}</Link>
                </td>
                <td>{lot.sku}</td>
                <td>{fmtDate(lot.expiry_date)}</td>
                <td className="flags">
                  <Badge tone={lot.status === 'BLOCKED' ? 'danger' : 'ok'}>{lotStatusLabel[lot.status]}</Badge>
                  {lot.expired && <Badge tone="danger">Scaduto</Badge>}
                </td>
                <td className="num">{fmtQty(lot.on_hand)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </section>
  )
}

function SerialSearch() {
  const [query, setQuery] = useState('')
  const q = useDebounced(query.trim())
  const serials = useAsync(
    () => unwrap(api.GET('/serials', { params: { query: { serial_number: q, page_size: 50 } } })),
    [q],
    q.length > 0,
  )
  return (
    <section className="card">
      <h2>Seriali</h2>
      <input type="search" placeholder="Numero di serie (ricerca parziale)" value={query} onChange={(e) => setQuery(e.target.value)} />
      <ProblemAlert problem={serials.error} />
      {q && serials.loading && <Loading />}
      {q && serials.data && serials.data.items.length === 0 && <Empty>Nessun seriale.</Empty>}
      {q && serials.data && serials.data.items.length > 0 && (
        <table className="table">
          <thead>
            <tr>
              <th>Seriale</th>
              <th>SKU</th>
              <th>Lotto</th>
              <th>Ubicazione</th>
              <th>Stato</th>
            </tr>
          </thead>
          <tbody>
            {serials.data.items.map((s) => (
              <tr key={s.id}>
                <td>
                  <Link to={`/serials/${s.id}`}>{s.serial_number}</Link>
                </td>
                <td>{s.sku}</td>
                <td>{s.lot_id ? <Link to={`/lots/${s.lot_id}`}>{s.lot_code}</Link> : '—'}</td>
                <td>{s.location_code ?? '—'}</td>
                <td>{serialStatusLabel[s.status]}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </section>
  )
}

export function TracePage() {
  return (
    <>
      <PageHeader title="Tracciabilità" />
      <div className="two-cols even">
        <LotSearch />
        <SerialSearch />
      </div>
    </>
  )
}
