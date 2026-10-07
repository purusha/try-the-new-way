import { type FormEvent, useState } from 'react'
import { api, newIdempotencyKey, writeHeaders } from '../api/client'
import { clientProblem, unwrap } from '../api/errors'
import type { AdjustmentReason, OperationResult, ProductListItem } from '../api/types'
import { Loading, OperatorNotice, PageHeader, RequireWarehouse } from '../components/common'
import { OperationResultView } from '../components/OperationResultView'
import { ProblemAlert } from '../components/ProblemAlert'
import { ProductPicker } from '../components/ProductPicker'
import { LocationSelect, LotSelect } from '../components/selects'
import { useAsync } from '../hooks/useAsync'
import { useLocations } from '../hooks/useLocations'
import { useSubmit } from '../hooks/useSubmit'
import { fmtQty, orNull, parseNumber, parseSerials } from '../lib/format'
import { adjustmentReasonLabel, options } from '../lib/labels'
import { useApp } from '../state/appContext'

type LotMode = 'none' | 'existing' | 'new'

function AdjustmentForm({ warehouseId }: { warehouseId: string }) {
  const { locations } = useLocations(warehouseId, false)
  const [locationId, setLocationId] = useState('')
  const [product, setProduct] = useState<ProductListItem | null>(null)
  const [lotMode, setLotMode] = useState<LotMode>('none')
  const [lotId, setLotId] = useState('')
  const [lotCode, setLotCode] = useState('')
  const [expiryDate, setExpiryDate] = useState('')
  const [counted, setCounted] = useState('')
  const [reason, setReason] = useState<AdjustmentReason>('COUNT')
  const [note, setNote] = useState('')
  const [serials, setSerials] = useState('')
  const [result, setResult] = useState<OperationResult | null>(null)
  const submit = useSubmit()

  const productId = product?.id ?? ''
  const tracked = product !== null && product.tracking !== 'NONE'

  // Quantità a sistema nella cella (ubicazione, prodotto, lotto).
  const stock = useAsync(
    () =>
      unwrap(
        api.GET('/stock', {
          params: {
            query: {
              warehouse_id: warehouseId,
              location_id: locationId,
              product_id: productId,
              lot_id: lotMode === 'existing' && lotId ? lotId : undefined,
              page_size: 200,
            },
          },
        }),
      ),
    [warehouseId, locationId, productId, lotMode, lotId, result],
    Boolean(locationId && productId && lotMode !== 'new' && (lotMode !== 'existing' || lotId)),
  )
  const rows = (stock.data?.items ?? []).filter((r) =>
    lotMode === 'existing' ? r.lot_id === lotId : lotMode === 'none' ? !r.lot_id : false,
  )
  const systemQty = lotMode === 'new' ? 0 : rows.reduce((sum, r) => sum + r.quantity, 0)
  const systemSerials = rows.flatMap((r) => r.serials ?? [])
  const countedQty = parseNumber(counted)
  const delta = Number.isNaN(countedQty) ? null : countedQty - systemQty

  const onSubmit = async (e: FormEvent) => {
    e.preventDefault()
    setResult(null)
    const done = await submit.run(() => {
      if (!locationId) throw clientProblem("Scegli l'ubicazione.")
      if (!product) throw clientProblem('Scegli il prodotto.')
      if (Number.isNaN(countedQty) || countedQty < 0) throw clientProblem('Indica la quantità contata (zero o più).')
      if (reason === 'OTHER' && !note.trim()) throw clientProblem('Con la causale "Altro" la nota è obbligatoria.')
      if (lotMode === 'existing' && !lotId) throw clientProblem('Scegli il lotto.')
      if (lotMode === 'new' && !lotCode.trim()) throw clientProblem('Indica il codice del lotto.')
      return unwrap(
        api.POST('/operations/adjustments', {
          params: { header: writeHeaders(newIdempotencyKey()) },
          body: {
            location_id: locationId,
            product_id: product.id,
            lot_id: lotMode === 'existing' ? lotId : null,
            lot:
              lotMode === 'new' ? { lot_code: lotCode.trim(), expiry_date: orNull(expiryDate) } : undefined,
            counted_quantity: countedQty,
            reason_code: reason,
            note: orNull(note),
            serials: product.tracking === 'SERIAL' ? parseSerials(serials) : undefined,
          },
        }),
      )
    })
    if (done) {
      setResult(done)
      setCounted('')
      setSerials('')
    }
  }

  return (
    <>
      <form className="card" onSubmit={onSubmit}>
        <OperatorNotice />
        <div className="form-grid">
          <label className="field">
            <span>Ubicazione *</span>
            <LocationSelect locations={locations} value={locationId} onChange={setLocationId} />
          </label>
          <div className="field span-2">
            <span>Prodotto *</span>
            <ProductPicker
              value={product}
              warehouseId={warehouseId}
              onChange={(p) => {
                setProduct(p)
                setLotMode(p && p.tracking !== 'NONE' ? 'existing' : 'none')
                setLotId('')
              }}
            />
          </div>
          {tracked && (
            <div className="field span-all">
              <span>Lotto</span>
              <div className="radio-row">
                {product.tracking === 'SERIAL' && (
                  <label>
                    <input type="radio" checked={lotMode === 'none'} onChange={() => setLotMode('none')} /> Nessun lotto
                  </label>
                )}
                <label>
                  <input type="radio" checked={lotMode === 'existing'} onChange={() => setLotMode('existing')} /> Lotto
                  esistente
                </label>
                <label>
                  <input type="radio" checked={lotMode === 'new'} onChange={() => setLotMode('new')} /> Nuovo lotto
                  (merce ritrovata)
                </label>
              </div>
            </div>
          )}
          {tracked && lotMode === 'existing' && (
            <label className="field span-2">
              <span>Lotto *</span>
              <LotSelect productId={productId} value={lotId} emptyLabel="— scegli lotto —" onChange={setLotId} />
            </label>
          )}
          {tracked && lotMode === 'new' && (
            <>
              <label className="field">
                <span>Codice lotto *</span>
                <input value={lotCode} maxLength={64} onChange={(e) => setLotCode(e.target.value)} />
              </label>
              <label className="field">
                <span>Scadenza{product.requires_expiry ? ' *' : ''}</span>
                <input type="date" value={expiryDate} onChange={(e) => setExpiryDate(e.target.value)} />
              </label>
            </>
          )}

          <div className="field span-all system-qty">
            {stock.loading ? (
              <Loading what="Lettura della giacenza…" />
            ) : (
              locationId &&
              product && (
                <p>
                  Quantità a sistema: <strong>{fmtQty(systemQty, product.uom)}</strong>
                  {delta !== null && delta !== 0 && (
                    <>
                      {' '}
                      · differenza{' '}
                      <strong className={delta < 0 ? 'neg' : 'pos'}>
                        {delta > 0 ? '+' : ''}
                        {fmtQty(delta, product.uom)}
                      </strong>
                    </>
                  )}
                  {delta === 0 && ' · nessuna differenza'}
                </p>
              )
            )}
            <ProblemAlert problem={stock.error} />
          </div>

          <label className="field">
            <span>Quantità contata *</span>
            <input type="number" step="any" min="0" value={counted} onChange={(e) => setCounted(e.target.value)} />
          </label>
          <label className="field">
            <span>Causale *</span>
            <select value={reason} onChange={(e) => setReason(e.target.value as AdjustmentReason)}>
              {options(adjustmentReasonLabel).map(([value, label]) => (
                <option key={value} value={value}>
                  {label}
                </option>
              ))}
            </select>
          </label>
          <label className="field span-2">
            <span>Note{reason === 'OTHER' ? ' *' : ''}</span>
            <input value={note} required={reason === 'OTHER'} onChange={(e) => setNote(e.target.value)} />
          </label>
          {product?.tracking === 'SERIAL' && (
            <label className="field span-all">
              <span>
                Seriali {delta !== null && delta < 0 ? 'mancanti' : delta !== null && delta > 0 ? 'trovati' : 'mancanti o trovati'}{' '}
                (uno per riga)
              </span>
              <textarea rows={4} value={serials} onChange={(e) => setSerials(e.target.value)} />
              {systemSerials.length > 0 && (
                <small className="muted">A sistema in questa ubicazione: {systemSerials.join(', ')}</small>
              )}
            </label>
          )}
        </div>
        <div className="actions">
          <button type="submit" className="primary" disabled={submit.busy}>
            Registra rettifica
          </button>
        </div>
        <ProblemAlert problem={submit.problem} />
      </form>
      <OperationResultView
        title="Rettifica"
        result={result}
        emptyText="Il conteggio coincide con il sistema: nessun movimento registrato."
      />
    </>
  )
}

export function AdjustmentPage() {
  const { warehouseId } = useApp()
  return (
    <>
      <PageHeader title="Rettifica inventario" />
      <RequireWarehouse>
        <AdjustmentForm key={warehouseId} warehouseId={warehouseId} />
      </RequireWarehouse>
    </>
  )
}
