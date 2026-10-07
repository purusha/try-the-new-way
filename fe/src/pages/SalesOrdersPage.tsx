import { type FormEvent, useState } from 'react'
import { Link, useNavigate } from 'react-router'
import { api, newIdempotencyKey, writeHeaders } from '../api/client'
import { clientProblem, unwrap } from '../api/errors'
import type { ProductListItem, SalesOrderStatus } from '../api/types'
import { Empty, Loading, OperatorNotice, PageHeader, Pagination, RequireWarehouse } from '../components/common'
import { ProblemAlert } from '../components/ProblemAlert'
import { ProductPicker } from '../components/ProductPicker'
import { SalesOrderStatusBadge } from '../components/SalesOrderStatusBadge'
import { useAsync } from '../hooks/useAsync'
import { useDebounced } from '../hooks/useDebounced'
import { useSubmit } from '../hooks/useSubmit'
import { fmtDateTime, opt, orNull, parseNumber } from '../lib/format'
import { options, salesOrderStatusLabel } from '../lib/labels'
import { useApp } from '../state/appContext'

const PAGE_SIZE = 25

interface Line {
  key: string
  product: ProductListItem | null
  quantity: string
}

let seq = 0
const emptyLine = (): Line => ({ key: `so${++seq}`, product: null, quantity: '' })

function CreateOrderForm({ warehouseId }: { warehouseId: string }) {
  const navigate = useNavigate()
  const [customer, setCustomer] = useState('')
  const [externalRef, setExternalRef] = useState('')
  const [notes, setNotes] = useState('')
  const [reserve, setReserve] = useState(true)
  const [lines, setLines] = useState<Line[]>(() => [emptyLine()])
  const submit = useSubmit()

  const updateLine = (key: string, patch: Partial<Line>) =>
    setLines((ls) => ls.map((l) => (l.key === key ? { ...l, ...patch } : l)))

  const onSubmit = async (e: FormEvent) => {
    e.preventDefault()
    const created = await submit.run(() => {
      const body = {
        customer_name: customer.trim(),
        external_ref: orNull(externalRef),
        notes: orNull(notes),
        warehouse_id: warehouseId,
        reserve,
        lines: lines.map((l, i) => {
          if (!l.product) throw clientProblem(`Riga ${i + 1}: scegli il prodotto.`)
          const quantity = parseNumber(l.quantity)
          if (!(quantity > 0)) throw clientProblem(`Riga ${i + 1}: la quantità deve essere maggiore di zero.`)
          return { product_id: l.product.id, quantity }
        }),
      }
      return unwrap(api.POST('/sales-orders', { params: { header: writeHeaders(newIdempotencyKey()) }, body }))
    })
    if (created) navigate(`/sales-orders/${created.id}`, { state: { result: created } })
  }

  return (
    <form className="card" onSubmit={onSubmit}>
      <h2>Nuovo ordine cliente</h2>
      <OperatorNotice />
      <div className="form-grid">
        <label className="field span-2">
          <span>Cliente *</span>
          <input value={customer} required onChange={(e) => setCustomer(e.target.value)} />
        </label>
        <label className="field">
          <span>Riferimento esterno</span>
          <input value={externalRef} onChange={(e) => setExternalRef(e.target.value)} />
        </label>
        <label className="field">
          <span>Note</span>
          <input value={notes} onChange={(e) => setNotes(e.target.value)} />
        </label>
      </div>
      {lines.map((line, i) => (
        <div key={line.key} className="line-row">
          <div className="field grow">
            <span>Prodotto {i + 1}</span>
            <ProductPicker
              value={line.product}
              warehouseId={warehouseId}
              onlyActive
              onChange={(product) => updateLine(line.key, { product })}
            />
          </div>
          <label className="field">
            <span>Quantità{line.product ? ` (${line.product.uom})` : ''}</span>
            <input
              type="number"
              step="any"
              min="0"
              value={line.quantity}
              onChange={(e) => updateLine(line.key, { quantity: e.target.value })}
            />
          </label>
          {lines.length > 1 && (
            <button type="button" className="link" onClick={() => setLines((ls) => ls.filter((l) => l.key !== line.key))}>
              Rimuovi
            </button>
          )}
        </div>
      ))}
      <label className="field checkbox">
        <input type="checkbox" checked={reserve} onChange={(e) => setReserve(e.target.checked)} />
        <span>Prenota subito (tutte le righe o nessuna)</span>
      </label>
      <div className="actions">
        <button type="button" onClick={() => setLines((ls) => [...ls, emptyLine()])}>
          + Aggiungi riga
        </button>
        <button type="submit" className="primary" disabled={submit.busy}>
          Crea ordine
        </button>
      </div>
      <ProblemAlert problem={submit.problem} />
    </form>
  )
}

function OrdersList({ warehouseId }: { warehouseId: string }) {
  const [status, setStatus] = useState<SalesOrderStatus | ''>('')
  const [query, setQuery] = useState('')
  const [page, setPage] = useState(1)
  const q = useDebounced(query)

  const orders = useAsync(
    () =>
      unwrap(
        api.GET('/sales-orders', {
          params: {
            query: { warehouse_id: warehouseId, status: status || undefined, q: opt(q), page, page_size: PAGE_SIZE },
          },
        }),
      ),
    [warehouseId, status, q, page],
  )

  return (
    <section className="card">
      <div className="filters">
        <label className="field">
          <span>Stato</span>
          <select
            value={status}
            onChange={(e) => {
              setStatus(e.target.value as SalesOrderStatus | '')
              setPage(1)
            }}
          >
            <option value="">Tutti</option>
            {options(salesOrderStatusLabel).map(([value, label]) => (
              <option key={value} value={value}>
                {label}
              </option>
            ))}
          </select>
        </label>
        <label className="field">
          <span>Cerca</span>
          <input
            type="search"
            placeholder="Numero, riferimento o cliente"
            value={query}
            onChange={(e) => {
              setQuery(e.target.value)
              setPage(1)
            }}
          />
        </label>
      </div>
      <ProblemAlert problem={orders.error} />
      {orders.loading && <Loading />}
      {orders.data && orders.data.items.length === 0 && <Empty>Nessun ordine.</Empty>}
      {orders.data && orders.data.items.length > 0 && (
        <>
          <div className="table-wrap">
            <table className="table">
              <thead>
                <tr>
                  <th>Numero</th>
                  <th>Cliente</th>
                  <th>Rif. esterno</th>
                  <th>Stato</th>
                  <th className="num">Righe</th>
                  <th>Creato</th>
                </tr>
              </thead>
              <tbody>
                {orders.data.items.map((o) => (
                  <tr key={o.id}>
                    <td>
                      <Link to={`/sales-orders/${o.id}`}>{o.number}</Link>
                    </td>
                    <td>{o.customer_name}</td>
                    <td>{o.external_ref ?? '—'}</td>
                    <td>
                      <SalesOrderStatusBadge status={o.status} />
                    </td>
                    <td className="num">{o.lines.length}</td>
                    <td>{fmtDateTime(o.created_at)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
          <Pagination page={page} pageSize={PAGE_SIZE} total={orders.data.total} onPage={setPage} />
        </>
      )}
    </section>
  )
}

export function SalesOrdersPage() {
  const { warehouseId } = useApp()
  const [creating, setCreating] = useState(false)
  return (
    <>
      <PageHeader title="Ordini cliente">
        <button type="button" className="primary" onClick={() => setCreating((c) => !c)}>
          {creating ? 'Chiudi' : 'Nuovo ordine'}
        </button>
      </PageHeader>
      <RequireWarehouse>
        {creating && <CreateOrderForm key={warehouseId} warehouseId={warehouseId} />}
        <OrdersList key={warehouseId} warehouseId={warehouseId} />
      </RequireWarehouse>
    </>
  )
}
