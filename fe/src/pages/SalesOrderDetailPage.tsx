import { useState } from 'react'
import { Link, useLocation, useParams } from 'react-router'
import { api, newIdempotencyKey, operatorHeader, writeHeaders } from '../api/client'
import { clientProblem, unwrap } from '../api/errors'
import type { Movement, PickingSuggestion, PickStrategy, SalesOrderLine, SalesOrderResult } from '../api/types'
import { Loading, OperatorNotice, PageHeader } from '../components/common'
import { MovementsTable } from '../components/MovementsTable'
import { OperationResultView } from '../components/OperationResultView'
import { PickingSuggestionView } from '../components/PickingSuggestionView'
import { ProblemAlert } from '../components/ProblemAlert'
import { ReasonDialog } from '../components/ReasonDialog'
import { ReverseMovementDialog } from '../components/ReverseMovementDialog'
import { SalesOrderStatusBadge } from '../components/SalesOrderStatusBadge'
import { useAsync } from '../hooks/useAsync'
import { useSubmit } from '../hooks/useSubmit'
import { fmtDateTime, fmtQty, orNull, parseNumber } from '../lib/format'
import { options, strategyLabel } from '../lib/labels'
import { useApp } from '../state/appContext'

type DialogKind = 'release' | 'cancel' | null

function LineQuantityEditor({
  line,
  busy,
  onSave,
  onCancel,
}: {
  line: SalesOrderLine
  busy: boolean
  onSave: (quantity: number) => void
  onCancel: () => void
}) {
  const [value, setValue] = useState(String(line.quantity_ordered))
  return (
    <span className="inline-edit">
      <input type="number" step="any" min="0" value={value} onChange={(e) => setValue(e.target.value)} />
      <button type="button" className="small primary" disabled={busy} onClick={() => onSave(parseNumber(value))}>
        Salva
      </button>
      <button type="button" className="small" onClick={onCancel}>
        Annulla
      </button>
    </span>
  )
}

export function SalesOrderDetailPage() {
  const { id = '' } = useParams()
  const location = useLocation()
  const { warehouses } = useApp()
  const initialResult = (location.state as { result?: SalesOrderResult } | null)?.result
  const [lastResult, setLastResult] = useState<SalesOrderResult | null>(initialResult ?? null)
  const [allowPartial, setAllowPartial] = useState(false)
  const [strategy, setStrategy] = useState<PickStrategy>('AUTO')
  const [externalRef, setExternalRef] = useState('')
  const [suggestion, setSuggestion] = useState<PickingSuggestion | null>(null)
  const [editingLine, setEditingLine] = useState<string | null>(null)
  const [dialog, setDialog] = useState<DialogKind>(null)
  const [toReverse, setToReverse] = useState<Movement | null>(null)
  const action = useSubmit()

  const order = useAsync(
    () => unwrap(api.GET('/sales-orders/{orderId}', { params: { path: { orderId: id } } })),
    [id],
  )
  const history = useAsync(
    () => unwrap(api.GET('/movements', { params: { query: { sales_order_id: id, page_size: 200 } } })),
    [id],
  )

  if (order.loading && !order.data) return <Loading />
  if (order.error && !order.data) return <ProblemAlert problem={order.error} />
  if (!order.data) return null
  const o = order.data

  const closed = o.status === 'FULFILLED' || o.status === 'CANCELLED'
  const canReserve = !closed && o.lines.some((l) => l.quantity_open > 0)
  const canRelease = !closed && o.lines.some((l) => l.quantity_reserved > 0)
  const canFulfill = canRelease
  const warehouse = warehouses.find((w) => w.id === o.warehouse_id)

  const apply = (result: SalesOrderResult | undefined) => {
    if (!result) return
    order.setData(result)
    setLastResult(result)
    setSuggestion(null)
    history.reload()
  }

  const orderPath = { orderId: id }

  const reserve = async () =>
    apply(
      await action.run(() =>
        unwrap(
          api.POST('/sales-orders/{orderId}/reserve', {
            params: { path: orderPath, header: writeHeaders(newIdempotencyKey()) },
            body: { allow_partial: allowPartial },
          }),
        ),
      ),
    )

  const release = async (reason: string) => {
    setDialog(null)
    apply(
      await action.run(() =>
        unwrap(
          api.POST('/sales-orders/{orderId}/release', {
            params: { path: orderPath, header: writeHeaders(newIdempotencyKey()) },
            body: { reason: orNull(reason) },
          }),
        ),
      ),
    )
  }

  const cancel = async (reason: string) => {
    setDialog(null)
    apply(
      await action.run(() =>
        unwrap(
          api.POST('/sales-orders/{orderId}/cancel', {
            params: { path: orderPath, header: operatorHeader() },
            body: { reason: orNull(reason) },
          }),
        ),
      ),
    )
  }

  const saveLine = async (line: SalesOrderLine, quantity: number) => {
    const result = await action.run(() => {
      if (!(quantity > 0)) throw clientProblem('La quantità deve essere maggiore di zero.')
      return unwrap(
        api.PATCH('/sales-orders/{orderId}/lines/{lineId}', {
          params: { path: { orderId: id, lineId: line.id }, header: operatorHeader() },
          body: { quantity },
        }),
      )
    })
    if (result) {
      setEditingLine(null)
      apply(result)
    }
  }

  const preview = async () => {
    const data = await action.run(() =>
      unwrap(
        api.POST('/operations/picking-suggestions', {
          body: { warehouse_id: o.warehouse_id, strategy, sales_order_id: id },
        }),
      ),
    )
    if (data) setSuggestion(data)
  }

  const fulfill = async () =>
    apply(
      await action.run(() =>
        unwrap(
          api.POST('/sales-orders/{orderId}/fulfill', {
            params: { path: orderPath, header: writeHeaders(newIdempotencyKey()) },
            body: { mode: 'AUTO', strategy, external_ref: orNull(externalRef) },
          }),
        ),
      ),
    )

  return (
    <>
      <PageHeader title={`Ordine ${o.number}`}>
        <Link to="/sales-orders">← Ordini cliente</Link>
      </PageHeader>

      <section className="card">
        <div className="card-head">
          <h2>
            {o.customer_name} <SalesOrderStatusBadge status={o.status} />
          </h2>
          <button type="button" onClick={order.reload}>
            Aggiorna
          </button>
        </div>
        <dl className="props">
          <div>
            <dt>Magazzino</dt>
            <dd>{warehouse ? `${warehouse.code} · ${warehouse.name}` : o.warehouse_id}</dd>
          </div>
          <div>
            <dt>Riferimento esterno</dt>
            <dd>{o.external_ref ?? '—'}</dd>
          </div>
          <div>
            <dt>Creato</dt>
            <dd>{fmtDateTime(o.created_at)}</dd>
          </div>
          <div>
            <dt>Aggiornato</dt>
            <dd>{fmtDateTime(o.updated_at)}</dd>
          </div>
          {o.notes && (
            <div className="span-2">
              <dt>Note</dt>
              <dd>{o.notes}</dd>
            </div>
          )}
        </dl>

        <div className="table-wrap">
          <table className="table">
            <thead>
              <tr>
                <th>#</th>
                <th>SKU</th>
                <th>Prodotto</th>
                <th className="num">Ordinata</th>
                <th className="num">Impegnata</th>
                <th className="num">Evasa</th>
                <th className="num">Da impegnare</th>
                <th />
              </tr>
            </thead>
            <tbody>
              {o.lines.map((line) => (
                <tr key={line.id}>
                  <td>{line.line_no}</td>
                  <td>
                    <Link to={`/products/${line.product_id}`}>{line.sku}</Link>
                  </td>
                  <td>{line.product_name}</td>
                  <td className="num">
                    {editingLine === line.id ? (
                      <LineQuantityEditor
                        line={line}
                        busy={action.busy}
                        onSave={(q) => saveLine(line, q)}
                        onCancel={() => setEditingLine(null)}
                      />
                    ) : (
                      fmtQty(line.quantity_ordered)
                    )}
                  </td>
                  <td className="num">{fmtQty(line.quantity_reserved)}</td>
                  <td className="num">{fmtQty(line.quantity_fulfilled)}</td>
                  <td className="num">{fmtQty(line.quantity_open)}</td>
                  <td>
                    {!closed && editingLine !== line.id && (
                      <button type="button" className="small" onClick={() => setEditingLine(line.id)}>
                        Modifica quantità
                      </button>
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </section>

      {!closed && (
        <section className="card">
          <h2>Azioni</h2>
          <OperatorNotice />
          <div className="action-groups">
            <div className="action-group">
              <h3>Prenotazione</h3>
              <label className="field checkbox">
                <input type="checkbox" checked={allowPartial} onChange={(e) => setAllowPartial(e.target.checked)} />
                <span>Consenti prenotazione parziale</span>
              </label>
              <div className="actions">
                <button type="button" className="primary" disabled={!canReserve || action.busy} onClick={reserve}>
                  Prenota
                </button>
                <button type="button" disabled={!canRelease || action.busy} onClick={() => setDialog('release')}>
                  Rilascia impegno
                </button>
              </div>
            </div>
            <div className="action-group">
              <h3>Evasione</h3>
              <label className="field">
                <span>Strategia</span>
                <select
                  value={strategy}
                  onChange={(e) => {
                    setStrategy(e.target.value as PickStrategy)
                    setSuggestion(null)
                  }}
                >
                  {options(strategyLabel).map(([value, label]) => (
                    <option key={value} value={value}>
                      {label}
                    </option>
                  ))}
                </select>
              </label>
              <label className="field">
                <span>Riferimento esterno (es. DDT di uscita)</span>
                <input value={externalRef} onChange={(e) => setExternalRef(e.target.value)} />
              </label>
              <div className="actions">
                <button type="button" disabled={!canFulfill || action.busy} onClick={preview}>
                  Anteprima prelievo
                </button>
                <button type="button" className="primary" disabled={!canFulfill || action.busy} onClick={fulfill}>
                  Evadi (automatico)
                </button>
              </div>
            </div>
            <div className="action-group">
              <h3>Annullamento</h3>
              <p className="muted tiny">Rilascia tutti gli impegni; le quantità già evase restano evase.</p>
              <button type="button" className="danger" disabled={action.busy} onClick={() => setDialog('cancel')}>
                Annulla ordine
              </button>
            </div>
          </div>
          <ProblemAlert problem={action.problem} onDismiss={() => action.setProblem(null)} />
          {suggestion && (
            <>
              <h3>Prelievo suggerito per l'impegnato</h3>
              <PickingSuggestionView suggestion={suggestion} />
            </>
          )}
        </section>
      )}
      {closed && <ProblemAlert problem={action.problem} onDismiss={() => action.setProblem(null)} />}

      <OperationResultView title="Esito dell'ultima operazione" result={lastResult} />

      <section className="card">
        <div className="card-head">
          <h2>Storico movimenti dell'ordine</h2>
          <button type="button" onClick={history.reload}>
            Aggiorna
          </button>
        </div>
        <ProblemAlert problem={history.error} />
        {history.loading && <Loading />}
        {history.data && <MovementsTable movements={history.data.items} onReverse={setToReverse} />}
      </section>

      <ReasonDialog
        open={dialog !== null}
        title={dialog === 'cancel' ? `Annulla l'ordine ${o.number}` : `Rilascia l'impegno dell'ordine ${o.number}`}
        confirmLabel={dialog === 'cancel' ? 'Annulla ordine' : 'Rilascia'}
        minLength={0}
        onCancel={() => setDialog(null)}
        onConfirm={(reason) => (dialog === 'cancel' ? cancel(reason) : release(reason))}
      >
        <p>
          {dialog === 'cancel'
            ? "L'ordine verrà annullato e tutti gli impegni rilasciati."
            : "Tutta la quantità impegnata dell'ordine torna disponibile."}
        </p>
      </ReasonDialog>

      <ReverseMovementDialog
        movement={toReverse}
        onClose={() => setToReverse(null)}
        onDone={() => {
          setToReverse(null)
          order.reload()
          history.reload()
        }}
      />
    </>
  )
}
