import { type FormEvent, useState } from 'react'
import { api, newIdempotencyKey, writeHeaders } from '../api/client'
import { clientProblem, unwrap } from '../api/errors'
import type { OperationResult, ProductListItem } from '../api/types'
import { Badge, OperatorNotice, PageHeader, RequireWarehouse } from '../components/common'
import { OperationResultView } from '../components/OperationResultView'
import { ProblemAlert } from '../components/ProblemAlert'
import { ProductPicker } from '../components/ProductPicker'
import { LocationSelect, StockRowSelect } from '../components/selects'
import { useLocations } from '../hooks/useLocations'
import { useProductStock } from '../hooks/useProductStock'
import { useSubmit } from '../hooks/useSubmit'
import { stockRowKey } from '../lib/describe'
import { fmtQty, orNull, parseNumber, parseSerials } from '../lib/format'
import { useApp } from '../state/appContext'

function TransferForm({ warehouseId }: { warehouseId: string }) {
  const { locations } = useLocations(warehouseId)
  const [product, setProduct] = useState<ProductListItem | null>(null)
  const [fromKey, setFromKey] = useState('')
  const [toLocationId, setToLocationId] = useState('')
  const [quantity, setQuantity] = useState('')
  const [serials, setSerials] = useState('')
  const [note, setNote] = useState('')
  const [result, setResult] = useState<OperationResult | null>(null)
  const submit = useSubmit()

  const stock = useProductStock(warehouseId, product?.id ?? '')
  const rows = stock.data?.items ?? []
  const from = rows.find((r) => stockRowKey(r) === fromKey)
  const isSerial = product?.tracking === 'SERIAL'

  const onSubmit = async (e: FormEvent) => {
    e.preventDefault()
    setResult(null)
    const done = await submit.run(() => {
      if (!product) throw clientProblem('Scegli il prodotto.')
      if (!from) throw clientProblem("Scegli l'ubicazione (e il lotto) di origine.")
      if (!toLocationId) throw clientProblem("Scegli l'ubicazione di destinazione.")
      const qty = parseNumber(quantity)
      if (!(qty > 0)) throw clientProblem('La quantità deve essere maggiore di zero.')
      return unwrap(
        api.POST('/operations/transfers', {
          params: { header: writeHeaders(newIdempotencyKey()) },
          body: {
            note: orNull(note),
            lines: [
              {
                product_id: product.id,
                lot_id: from.lot_id ?? null,
                from_location_id: from.location_id,
                to_location_id: toLocationId,
                quantity: qty,
                serials: isSerial ? parseSerials(serials) : undefined,
              },
            ],
          },
        }),
      )
    })
    if (done) {
      setResult(done)
      setFromKey('')
      setQuantity('')
      setSerials('')
      stock.reload()
    }
  }

  return (
    <>
      <form className="card" onSubmit={onSubmit}>
        <OperatorNotice />
        <div className="form-grid">
          <div className="field span-2">
            <span>Prodotto *</span>
            <ProductPicker
              value={product}
              warehouseId={warehouseId}
              onChange={(p) => {
                setProduct(p)
                setFromKey('')
              }}
            />
          </div>
          <label className="field span-2">
            <span>Da (ubicazione · lotto) *</span>
            <StockRowSelect rows={rows} loading={stock.loading} value={fromKey} onChange={(key) => setFromKey(key)} />
          </label>
          <label className="field">
            <span>A (ubicazione di destinazione) *</span>
            <LocationSelect
              locations={locations}
              value={toLocationId}
              exclude={from?.location_id}
              onChange={setToLocationId}
            />
          </label>
          <label className="field">
            <span>Quantità *{product ? ` (${product.uom})` : ''}</span>
            <input type="number" step="any" min="0" value={quantity} onChange={(e) => setQuantity(e.target.value)} />
            {from && (
              <small className="muted">
                Presenti: {fmtQty(from.quantity, from.uom)}{' '}
                <button type="button" className="link" onClick={() => setQuantity(String(from.quantity))}>
                  tutto
                </button>
              </small>
            )}
          </label>
          {from && !from.usable && (
            <div className="span-all">
              <Badge tone="warn">Lotto scaduto o bloccato</Badge>{' '}
              <span className="muted">il trasferimento è ammesso (es. verso un'area di quarantena).</span>
            </div>
          )}
          {isSerial && (
            <label className="field span-all">
              <span>Seriali da trasferire * (uno per riga)</span>
              <textarea rows={4} value={serials} onChange={(e) => setSerials(e.target.value)} />
              {from?.serials && from.serials.length > 0 && (
                <small className="muted">Presenti nell'origine: {from.serials.join(', ')}</small>
              )}
            </label>
          )}
          <label className="field span-all">
            <span>Note</span>
            <input value={note} onChange={(e) => setNote(e.target.value)} />
          </label>
        </div>
        <div className="actions">
          <button type="submit" className="primary" disabled={submit.busy}>
            Registra trasferimento
          </button>
        </div>
        <ProblemAlert problem={submit.problem} />
      </form>
      <OperationResultView title="Trasferimento registrato" result={result} />
    </>
  )
}

export function TransferPage() {
  const { warehouseId } = useApp()
  return (
    <>
      <PageHeader title="Trasferimento interno" />
      <RequireWarehouse>
        <TransferForm key={warehouseId} warehouseId={warehouseId} />
      </RequireWarehouse>
    </>
  )
}
