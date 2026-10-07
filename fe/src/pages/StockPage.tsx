import { useState } from 'react'
import { Link } from 'react-router'
import { api } from '../api/client'
import { unwrap } from '../api/errors'
import type { ProductListItem } from '../api/types'
import { GroupedAlerts } from '../components/AlertsList'
import { Badge, Empty, Loading, PageHeader, Pagination, RequireWarehouse } from '../components/common'
import { ProblemAlert } from '../components/ProblemAlert'
import { ProductPicker } from '../components/ProductPicker'
import { LocationSelect } from '../components/selects'
import { useAsync } from '../hooks/useAsync'
import { useLocations } from '../hooks/useLocations'
import { fmtDate, fmtQty } from '../lib/format'
import { lotStatusLabel } from '../lib/labels'
import { useApp } from '../state/appContext'

const PAGE_SIZE = 50

function StockTable({ warehouseId }: { warehouseId: string }) {
  const [product, setProduct] = useState<ProductListItem | null>(null)
  const [locationId, setLocationId] = useState('')
  const [page, setPage] = useState(1)
  const { locations } = useLocations(warehouseId, false)
  const productId = product?.id ?? ''

  const stock = useAsync(
    () =>
      unwrap(
        api.GET('/stock', {
          params: {
            query: {
              warehouse_id: warehouseId,
              product_id: productId || undefined,
              location_id: locationId || undefined,
              page,
              page_size: PAGE_SIZE,
            },
          },
        }),
      ),
    [warehouseId, productId, locationId, page],
  )

  return (
    <section className="card">
      <h2>Giacenze per ubicazione</h2>
      <div className="filters">
        <div className="field">
          <span>Prodotto</span>
          <ProductPicker
            value={product}
            warehouseId={warehouseId}
            onChange={(p) => {
              setProduct(p)
              setPage(1)
            }}
          />
        </div>
        <label className="field">
          <span>Ubicazione</span>
          <LocationSelect
            locations={locations}
            value={locationId}
            emptyLabel="Tutte"
            onChange={(id) => {
              setLocationId(id)
              setPage(1)
            }}
          />
        </label>
      </div>
      <ProblemAlert problem={stock.error} />
      {stock.loading && <Loading />}
      {stock.data && stock.data.items.length === 0 && <Empty>Nessuna giacenza.</Empty>}
      {stock.data && stock.data.items.length > 0 && (
        <>
          <div className="table-wrap">
            <table className="table">
              <thead>
                <tr>
                  <th>Ubicazione</th>
                  <th>SKU</th>
                  <th>Prodotto</th>
                  <th>Lotto</th>
                  <th>Scadenza</th>
                  <th className="num">Quantità</th>
                  <th>Utilizzabile</th>
                  <th>Seriali</th>
                </tr>
              </thead>
              <tbody>
                {stock.data.items.map((row) => (
                  <tr key={row.id}>
                    <td>{row.location_code}</td>
                    <td>
                      <Link to={`/products/${row.product_id}`}>{row.sku}</Link>
                    </td>
                    <td>{row.product_name}</td>
                    <td>
                      {row.lot_id ? <Link to={`/lots/${row.lot_id}`}>{row.lot_code}</Link> : '—'}
                      {row.lot_status === 'BLOCKED' && (
                        <>
                          {' '}
                          <Badge tone="danger">{lotStatusLabel.BLOCKED}</Badge>
                        </>
                      )}
                    </td>
                    <td>{fmtDate(row.expiry_date)}</td>
                    <td className="num">{fmtQty(row.quantity, row.uom)}</td>
                    <td>{row.usable ? <Badge tone="ok">Sì</Badge> : <Badge tone="danger">No</Badge>}</td>
                    <td className="serials" title={row.serials?.join(', ')}>
                      {row.serials && row.serials.length > 0 ? row.serials.join(', ') : '—'}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
          <Pagination page={page} pageSize={PAGE_SIZE} total={stock.data.total} onPage={setPage} />
        </>
      )}
    </section>
  )
}

function AlertsPanel({ warehouseId }: { warehouseId: string }) {
  const alerts = useAsync(
    () => unwrap(api.GET('/alerts', { params: { query: { warehouse_id: warehouseId } } })),
    [warehouseId],
  )
  return (
    <section className="card">
      <div className="card-head">
        <h2>Alert</h2>
        <button type="button" onClick={alerts.reload}>
          Aggiorna
        </button>
      </div>
      <ProblemAlert problem={alerts.error} />
      {alerts.loading && <Loading />}
      {alerts.data && <GroupedAlerts alerts={alerts.data.items} />}
    </section>
  )
}

export function StockPage() {
  const { warehouseId } = useApp()
  return (
    <>
      <PageHeader title="Giacenze e alert" />
      <RequireWarehouse>
        <div className="two-cols">
          <StockTable key={warehouseId} warehouseId={warehouseId} />
          <AlertsPanel warehouseId={warehouseId} />
        </div>
      </RequireWarehouse>
    </>
  )
}
