import { type FormEvent, useState } from 'react'
import { api, newIdempotencyKey, writeHeaders } from '../api/client'
import { clientProblem, unwrap } from '../api/errors'
import type { Location, OperationResult, ProductListItem, PurchaseOrder, ReceiptLine } from '../api/types'
import { Loading, OperatorNotice, PageHeader, RequireWarehouse } from '../components/common'
import { OperationResultView } from '../components/OperationResultView'
import { ProblemAlert } from '../components/ProblemAlert'
import { ProductPicker } from '../components/ProductPicker'
import { LocationSelect, LotSelect } from '../components/selects'
import { useAsync } from '../hooks/useAsync'
import { useLocations } from '../hooks/useLocations'
import { useSubmit } from '../hooks/useSubmit'
import { fmtDate, fmtQty, orNull, parseNumber, parseSerials } from '../lib/format'
import { purchaseOrderStatusLabel } from '../lib/labels'
import { useApp } from '../state/appContext'

type LotMode = 'none' | 'existing' | 'new'

interface Line {
  key: string
  product: ProductListItem | null
  quantity: string
  locationId: string
  lotMode: LotMode
  lotId: string
  lotCode: string
  productionDate: string
  expiryDate: string
  serials: string
  poLineId: string
}

let lineSeq = 0
function emptyLine(): Line {
  lineSeq += 1
  return {
    key: `l${lineSeq}`,
    product: null,
    quantity: '',
    locationId: '',
    lotMode: 'none',
    lotId: '',
    lotCode: '',
    productionDate: '',
    expiryDate: '',
    serials: '',
    poLineId: '',
  }
}

function toReceiptLine(line: Line, index: number): ReceiptLine {
  const n = index + 1
  if (!line.product) throw clientProblem(`Riga ${n}: scegli il prodotto.`)
  const quantity = parseNumber(line.quantity)
  if (!(quantity > 0)) throw clientProblem(`Riga ${n}: la quantità deve essere maggiore di zero.`)
  if (!line.locationId) throw clientProblem(`Riga ${n}: scegli l'ubicazione di destinazione.`)

  const result: ReceiptLine = {
    product_id: line.product.id,
    quantity,
    location_id: line.locationId,
    purchase_order_line_id: line.poLineId || null,
  }
  if (line.lotMode === 'existing') {
    if (!line.lotId) throw clientProblem(`Riga ${n}: scegli il lotto esistente.`)
    result.lot_id = line.lotId
  } else if (line.lotMode === 'new') {
    if (!line.lotCode.trim()) throw clientProblem(`Riga ${n}: indica il codice del nuovo lotto.`)
    result.lot = {
      lot_code: line.lotCode.trim(),
      production_date: orNull(line.productionDate),
      expiry_date: orNull(line.expiryDate),
    }
  }
  if (line.product.tracking === 'SERIAL') result.serials = parseSerials(line.serials)
  return result
}

interface LineEditorProps {
  line: Line
  index: number
  warehouseId: string
  locations: Location[]
  order: PurchaseOrder | undefined
  canRemove: boolean
  onChange: (patch: Partial<Line>) => void
  onRemove: () => void
}

function LineEditor({ line, index, warehouseId, locations, order, canRemove, onChange, onRemove }: LineEditorProps) {
  const p = line.product
  const serialCount = parseSerials(line.serials).length
  const poLines = (order?.lines ?? []).filter((l) => !p || l.product_id === p.id)

  return (
    <fieldset className="line">
      <legend>
        Riga {index + 1}
        {canRemove && (
          <button type="button" className="link" onClick={onRemove}>
            Rimuovi
          </button>
        )}
      </legend>
      <div className="form-grid">
        <div className="field span-2">
          <span>Prodotto *</span>
          <ProductPicker
            value={p}
            warehouseId={warehouseId}
            onlyActive
            onChange={(product) =>
              onChange({
                product,
                lotMode: !product || product.tracking === 'NONE' ? 'none' : 'new',
                lotId: '',
                poLineId: '',
              })
            }
          />
        </div>
        <label className="field">
          <span>Quantità *{p ? ` (${p.uom})` : ''}</span>
          <input
            type="number"
            step={p?.tracking === 'SERIAL' ? '1' : 'any'}
            min="0"
            value={line.quantity}
            onChange={(e) => onChange({ quantity: e.target.value })}
          />
        </label>
        <label className="field">
          <span>Ubicazione di destinazione *</span>
          <LocationSelect locations={locations} value={line.locationId} onChange={(id) => onChange({ locationId: id })} />
        </label>

        {p && p.tracking !== 'NONE' && (
          <div className="field span-all">
            <span>
              Lotto
              {p.requires_expiry ? ' (obbligatorio, con scadenza)' : p.tracking === 'LOT' ? ' (obbligatorio)' : ''}
            </span>
            <div className="radio-row">
              {p.tracking === 'SERIAL' && !p.requires_expiry && (
                <label>
                  <input type="radio" checked={line.lotMode === 'none'} onChange={() => onChange({ lotMode: 'none' })} />{' '}
                  Nessun lotto
                </label>
              )}
              <label>
                <input
                  type="radio"
                  checked={line.lotMode === 'existing'}
                  onChange={() => onChange({ lotMode: 'existing' })}
                />{' '}
                Lotto esistente
              </label>
              <label>
                <input type="radio" checked={line.lotMode === 'new'} onChange={() => onChange({ lotMode: 'new' })} /> Nuovo
                lotto
              </label>
            </div>
          </div>
        )}
        {p && line.lotMode === 'existing' && (
          <label className="field span-2">
            <span>Lotto esistente *</span>
            <LotSelect
              productId={p.id}
              value={line.lotId}
              emptyLabel="— scegli lotto —"
              onChange={(id) => onChange({ lotId: id })}
            />
          </label>
        )}
        {p && line.lotMode === 'new' && (
          <>
            <label className="field">
              <span>Codice lotto *</span>
              <input value={line.lotCode} maxLength={64} onChange={(e) => onChange({ lotCode: e.target.value })} />
            </label>
            <label className="field">
              <span>Data di produzione</span>
              <input type="date" value={line.productionDate} onChange={(e) => onChange({ productionDate: e.target.value })} />
            </label>
            <label className="field">
              <span>Data di scadenza{p.requires_expiry ? ' *' : ''}</span>
              <input
                type="date"
                value={line.expiryDate}
                required={p.requires_expiry}
                onChange={(e) => onChange({ expiryDate: e.target.value })}
              />
            </label>
          </>
        )}
        {p?.tracking === 'SERIAL' && (
          <label className="field span-all">
            <span>
              Numeri di serie * (uno per riga) — {serialCount} inseriti
              {line.quantity ? ` su ${line.quantity}` : ''}
            </span>
            <textarea rows={4} value={line.serials} onChange={(e) => onChange({ serials: e.target.value })} />
          </label>
        )}
        {order && (
          <label className="field span-2">
            <span>Riga dell'ordine fornitore</span>
            <select value={line.poLineId} onChange={(e) => onChange({ poLineId: e.target.value })}>
              <option value="">— nessuna —</option>
              {poLines.map((l) => (
                <option key={l.id} value={l.id}>
                  {l.line_no}. {l.sku} — ricevuti {fmtQty(l.quantity_received)} su {fmtQty(l.quantity_ordered)}
                </option>
              ))}
            </select>
          </label>
        )}
      </div>
    </fieldset>
  )
}

function ReceiptForm({ warehouseId }: { warehouseId: string }) {
  const { locations, loading: locationsLoading } = useLocations(warehouseId)
  const [lines, setLines] = useState<Line[]>(() => [emptyLine()])
  const [orderId, setOrderId] = useState('')
  const [externalRef, setExternalRef] = useState('')
  const [note, setNote] = useState('')
  const [result, setResult] = useState<OperationResult | null>(null)
  const submit = useSubmit()

  // Ordini fornitore ancora da ricevere: OPEN e PARTIALLY_RECEIVED (il filtro accetta un solo stato).
  const orders = useAsync(async () => {
    const [open, partial] = await Promise.all(
      (['OPEN', 'PARTIALLY_RECEIVED'] as const).map((status) =>
        unwrap(
          api.GET('/purchase-orders', {
            params: { query: { warehouse_id: warehouseId, status, page_size: 200 } },
          }),
        ),
      ),
    )
    return [...open.items, ...partial.items]
  }, [warehouseId])
  const order = orders.data?.find((o) => o.id === orderId)

  const updateLine = (key: string, patch: Partial<Line>) =>
    setLines((ls) => ls.map((l) => (l.key === key ? { ...l, ...patch } : l)))

  const onSubmit = async (e: FormEvent) => {
    e.preventDefault()
    setResult(null)
    const idempotencyKey = newIdempotencyKey()
    const done = await submit.run(() =>
      unwrap(
        api.POST('/operations/receipts', {
          params: { header: writeHeaders(idempotencyKey) },
          body: {
            warehouse_id: warehouseId,
            purchase_order_id: orderId || null,
            external_ref: orNull(externalRef),
            note: orNull(note),
            lines: lines.map(toReceiptLine),
          },
        }),
      ),
    )
    if (done) {
      setResult(done)
      setLines([emptyLine()])
      setExternalRef('')
      setNote('')
      if (orderId) orders.reload()
    }
  }

  if (locationsLoading) return <Loading />

  return (
    <>
      <form className="card" onSubmit={onSubmit}>
        <OperatorNotice />
        <div className="form-grid">
          <label className="field span-2">
            <span>Ordine fornitore (facoltativo)</span>
            <select
              value={orderId}
              onChange={(e) => {
                setOrderId(e.target.value)
                setLines((ls) => ls.map((l) => ({ ...l, poLineId: '' })))
              }}
            >
              <option value="">— nessuno —</option>
              {(orders.data ?? []).map((o) => (
                <option key={o.id} value={o.id}>
                  {o.number} · {o.supplier_name} · {purchaseOrderStatusLabel[o.status]}
                  {o.expected_date ? ` · atteso ${fmtDate(o.expected_date)}` : ''}
                </option>
              ))}
            </select>
          </label>
          <label className="field">
            <span>Riferimento esterno (es. DDT)</span>
            <input value={externalRef} onChange={(e) => setExternalRef(e.target.value)} />
          </label>
          <label className="field">
            <span>Note</span>
            <input value={note} onChange={(e) => setNote(e.target.value)} />
          </label>
        </div>
        <ProblemAlert problem={orders.error} />

        {lines.map((line, index) => (
          <LineEditor
            key={line.key}
            line={line}
            index={index}
            warehouseId={warehouseId}
            locations={locations}
            order={order}
            canRemove={lines.length > 1}
            onChange={(patch) => updateLine(line.key, patch)}
            onRemove={() => setLines((ls) => ls.filter((l) => l.key !== line.key))}
          />
        ))}

        <div className="actions">
          <button type="button" onClick={() => setLines((ls) => [...ls, emptyLine()])}>
            + Aggiungi riga
          </button>
          <button type="submit" className="primary" disabled={submit.busy}>
            Registra carico
          </button>
        </div>
        <ProblemAlert problem={submit.problem} />
      </form>
      <OperationResultView title="Carico registrato" result={result} />
    </>
  )
}

export function ReceiptPage() {
  const { warehouseId } = useApp()
  return (
    <>
      <PageHeader title="Carico (ingresso merci)" />
      <RequireWarehouse>
        <ReceiptForm key={warehouseId} warehouseId={warehouseId} />
      </RequireWarehouse>
    </>
  )
}
