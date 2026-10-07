import { useState } from 'react'
import { api } from '../api/client'
import { unwrap } from '../api/errors'
import type { Movement, MovementType, OperationResult, ProductListItem } from '../api/types'
import { Loading, PageHeader, Pagination, RequireWarehouse } from '../components/common'
import { MovementsTable } from '../components/MovementsTable'
import { OperationResultView } from '../components/OperationResultView'
import { ProblemAlert } from '../components/ProblemAlert'
import { ProductPicker } from '../components/ProductPicker'
import { ReverseMovementDialog } from '../components/ReverseMovementDialog'
import { LocationSelect, LotSelect } from '../components/selects'
import { useAsync } from '../hooks/useAsync'
import { useDebounced } from '../hooks/useDebounced'
import { useLocations } from '../hooks/useLocations'
import { dayStartIso, opt } from '../lib/format'
import { movementTypeLabel, options } from '../lib/labels'
import { useApp } from '../state/appContext'

const PAGE_SIZE = 50

interface Filters {
  type: MovementType | ''
  lotId: string
  locationId: string
  orderRef: string
  operator: string
  from: string
  to: string
}

const emptyFilters: Filters = { type: '', lotId: '', locationId: '', orderRef: '', operator: '', from: '', to: '' }

function MovementsHistory({ warehouseId }: { warehouseId: string }) {
  const { locations } = useLocations(warehouseId, false)
  const [filters, setFilters] = useState<Filters>(emptyFilters)
  const [product, setProduct] = useState<ProductListItem | null>(null)
  const [page, setPage] = useState(1)
  const [toReverse, setToReverse] = useState<Movement | null>(null)
  const [reversal, setReversal] = useState<OperationResult | null>(null)

  const orderRef = useDebounced(filters.orderRef)
  const operator = useDebounced(filters.operator)
  const productId = product?.id ?? ''

  const movements = useAsync(
    () =>
      unwrap(
        api.GET('/movements', {
          params: {
            query: {
              warehouse_id: warehouseId,
              type: filters.type || undefined,
              product_id: productId || undefined,
              lot_id: filters.lotId || undefined,
              location_id: filters.locationId || undefined,
              order_ref: opt(orderRef),
              operator: opt(operator),
              // "to" è esclusivo: per includere il giorno scelto si usa l'inizio del giorno dopo.
              from: filters.from ? dayStartIso(filters.from) : undefined,
              to: filters.to ? dayStartIso(filters.to, 1) : undefined,
              page,
              page_size: PAGE_SIZE,
            },
          },
        }),
      ),
    [warehouseId, filters.type, productId, filters.lotId, filters.locationId, orderRef, operator, filters.from, filters.to, page],
  )

  const update = <K extends keyof Filters>(key: K, value: Filters[K]) => {
    setFilters((f) => ({ ...f, [key]: value }))
    setPage(1)
  }

  return (
    <>
      <section className="card">
        <div className="filters">
          <label className="field">
            <span>Tipo</span>
            <select value={filters.type} onChange={(e) => update('type', e.target.value as MovementType | '')}>
              <option value="">Tutti</option>
              {options(movementTypeLabel).map(([value, label]) => (
                <option key={value} value={value}>
                  {label}
                </option>
              ))}
            </select>
          </label>
          <div className="field">
            <span>Prodotto</span>
            <ProductPicker
              value={product}
              onChange={(p) => {
                setProduct(p)
                update('lotId', '')
              }}
            />
          </div>
          {product && product.tracking !== 'NONE' && (
            <label className="field">
              <span>Lotto</span>
              <LotSelect
                productId={product.id}
                value={filters.lotId}
                emptyLabel="Tutti i lotti"
                onChange={(id) => update('lotId', id)}
              />
            </label>
          )}
          <label className="field">
            <span>Ubicazione</span>
            <LocationSelect
              locations={locations}
              value={filters.locationId}
              emptyLabel="Tutte"
              onChange={(id) => update('locationId', id)}
            />
          </label>
          <label className="field">
            <span>Ordine / riferimento</span>
            <input value={filters.orderRef} onChange={(e) => update('orderRef', e.target.value)} />
          </label>
          <label className="field">
            <span>Operatore</span>
            <input value={filters.operator} onChange={(e) => update('operator', e.target.value)} />
          </label>
          <label className="field">
            <span>Dal</span>
            <input type="date" value={filters.from} onChange={(e) => update('from', e.target.value)} />
          </label>
          <label className="field">
            <span>Al</span>
            <input type="date" value={filters.to} onChange={(e) => update('to', e.target.value)} />
          </label>
          <div className="field">
            <span>&nbsp;</span>
            <button
              type="button"
              onClick={() => {
                setFilters(emptyFilters)
                setProduct(null)
                setPage(1)
              }}
            >
              Azzera filtri
            </button>
          </div>
        </div>
        <ProblemAlert problem={movements.error} />
        {movements.loading && <Loading />}
        {movements.data && (
          <>
            <MovementsTable movements={movements.data.items} onReverse={setToReverse} />
            <Pagination page={page} pageSize={PAGE_SIZE} total={movements.data.total} onPage={setPage} />
          </>
        )}
      </section>

      <OperationResultView title="Storno registrato" result={reversal} />

      <ReverseMovementDialog
        movement={toReverse}
        onClose={() => setToReverse(null)}
        onDone={(result) => {
          setToReverse(null)
          setReversal(result)
          movements.reload()
        }}
      />
    </>
  )
}

export function MovementsPage() {
  const { warehouseId } = useApp()
  return (
    <>
      <PageHeader title="Movimenti" />
      <p className="muted">
        I movimenti sono immutabili: un errore si corregge con uno storno, che crea il movimento inverso collegato.
      </p>
      <RequireWarehouse>
        <MovementsHistory key={warehouseId} warehouseId={warehouseId} />
      </RequireWarehouse>
    </>
  )
}
