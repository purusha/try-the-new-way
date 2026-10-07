import { useState } from 'react'
import { Link, useNavigate } from 'react-router'
import { api, operatorHeader } from '../api/client'
import { unwrap } from '../api/errors'
import type { ProductCreate, ProductListItem, ProductStatus } from '../api/types'
import { Badge, Empty, Loading, OperatorNotice, PageHeader, Pagination } from '../components/common'
import { ProblemAlert } from '../components/ProblemAlert'
import { ProductForm } from '../components/ProductForm'
import { useAsync } from '../hooks/useAsync'
import { useDebounced } from '../hooks/useDebounced'
import { useSubmit } from '../hooks/useSubmit'
import { fmtQty, opt } from '../lib/format'
import { options, productStatusLabel, trackingLabel } from '../lib/labels'
import { useApp } from '../state/appContext'

const PAGE_SIZE = 25

interface Filters {
  q: string
  category: string
  status: ProductStatus | ''
  belowMin: boolean
  expiringDays: string
}

function StockBadges({ p }: { p: ProductListItem }) {
  return (
    <>
      {p.status === 'INACTIVE' && <Badge>Archiviato</Badge>}
      {p.stock.below_min_stock && <Badge tone="warn">Sotto scorta</Badge>}
      {p.stock.expiring_quantity > 0 && (
        <Badge tone="warn" title={`${fmtQty(p.stock.expiring_quantity, p.uom)} in scadenza`}>
          In scadenza
        </Badge>
      )}
      {p.stock.expired_quantity > 0 && (
        <Badge tone="danger" title={`${fmtQty(p.stock.expired_quantity, p.uom)} scaduti`}>
          Scaduti
        </Badge>
      )}
      {p.stock.available < 0 && <Badge tone="danger">Impegni a rischio</Badge>}
    </>
  )
}

export function ProductsPage() {
  const { warehouseId, warehouse } = useApp()
  const navigate = useNavigate()
  const [filters, setFilters] = useState<Filters>({ q: '', category: '', status: '', belowMin: false, expiringDays: '' })
  const [page, setPage] = useState(1)
  const [creating, setCreating] = useState(false)
  const create = useSubmit()

  const q = useDebounced(filters.q)
  const category = useDebounced(filters.category)
  const expiringDays = Number.parseInt(filters.expiringDays, 10)

  const list = useAsync(
    () =>
      unwrap(
        api.GET('/products', {
          params: {
            query: {
              page,
              page_size: PAGE_SIZE,
              q: opt(q),
              category: opt(category),
              status: filters.status || undefined,
              below_min_stock: filters.belowMin || undefined,
              expiring_within_days: Number.isNaN(expiringDays) ? undefined : expiringDays,
              warehouse_id: warehouseId || undefined,
            },
          },
        }),
      ),
    [page, q, category, filters.status, filters.belowMin, expiringDays, warehouseId],
  )

  const update = <K extends keyof Filters>(key: K, value: Filters[K]) => {
    setFilters((f) => ({ ...f, [key]: value }))
    setPage(1)
  }

  const submitCreate = async (body: ProductCreate) => {
    const product = await create.run(() =>
      unwrap(api.POST('/products', { params: { header: operatorHeader() }, body })),
    )
    if (product) navigate(`/products/${product.id}`)
  }

  return (
    <>
      <PageHeader title="Prodotti">
        <button type="button" className="primary" onClick={() => setCreating((c) => !c)}>
          {creating ? 'Chiudi' : 'Nuovo prodotto'}
        </button>
      </PageHeader>

      {creating && (
        <section className="card">
          <h2>Nuovo prodotto</h2>
          <OperatorNotice />
          <ProductForm mode="create" busy={create.busy} onSubmit={submitCreate} onCancel={() => setCreating(false)} />
          <ProblemAlert problem={create.problem} />
        </section>
      )}

      <section className="card">
        <div className="filters">
          <label className="field">
            <span>Cerca</span>
            <input
              type="search"
              placeholder="SKU, EAN o nome"
              value={filters.q}
              onChange={(e) => update('q', e.target.value)}
            />
          </label>
          <label className="field">
            <span>Categoria</span>
            <input value={filters.category} onChange={(e) => update('category', e.target.value)} />
          </label>
          <label className="field">
            <span>Stato</span>
            <select value={filters.status} onChange={(e) => update('status', e.target.value as ProductStatus | '')}>
              <option value="">Tutti</option>
              {options(productStatusLabel).map(([value, label]) => (
                <option key={value} value={value}>
                  {label}
                </option>
              ))}
            </select>
          </label>
          <label className="field">
            <span>In scadenza entro (giorni)</span>
            <input
              type="number"
              min="0"
              step="1"
              value={filters.expiringDays}
              onChange={(e) => update('expiringDays', e.target.value)}
            />
          </label>
          <label className="field checkbox">
            <input type="checkbox" checked={filters.belowMin} onChange={(e) => update('belowMin', e.target.checked)} />
            <span>Solo sotto scorta</span>
          </label>
        </div>
        <p className="tiny muted">
          Giacenze riferite a {warehouse ? `${warehouse.code} · ${warehouse.name}` : 'tutti i magazzini'}.
        </p>

        <ProblemAlert problem={list.error} />
        {list.loading && <Loading />}
        {list.data && list.data.items.length === 0 && <Empty>Nessun prodotto trovato.</Empty>}
        {list.data && list.data.items.length > 0 && (
          <>
            <div className="table-wrap">
              <table className="table">
                <thead>
                  <tr>
                    <th>SKU</th>
                    <th>Nome</th>
                    <th>Categoria</th>
                    <th>UdM</th>
                    <th>Tracciabilità</th>
                    <th className="num">Fisica</th>
                    <th className="num">Impegnata</th>
                    <th className="num">Disponibile</th>
                    <th />
                  </tr>
                </thead>
                <tbody>
                  {list.data.items.map((p) => (
                    <tr key={p.id} className={p.status === 'INACTIVE' ? 'dim' : ''}>
                      <td>
                        <Link to={`/products/${p.id}`}>{p.sku}</Link>
                      </td>
                      <td>{p.name}</td>
                      <td>{p.category ?? '—'}</td>
                      <td>{p.uom}</td>
                      <td>{trackingLabel[p.tracking]}</td>
                      <td className="num">{fmtQty(p.stock.physical)}</td>
                      <td className="num">{fmtQty(p.stock.reserved)}</td>
                      <td className="num">
                        <strong>{fmtQty(p.stock.available)}</strong>
                      </td>
                      <td className="flags">
                        <StockBadges p={p} />
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
            <Pagination page={page} pageSize={PAGE_SIZE} total={list.data.total} onPage={setPage} />
          </>
        )}
      </section>
    </>
  )
}
