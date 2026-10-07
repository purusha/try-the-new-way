import { useState } from 'react'
import { Link, useParams } from 'react-router'
import { api, operatorHeader } from '../api/client'
import { unwrap, unwrapResponse } from '../api/errors'
import type { ProductUpdate } from '../api/types'
import { AlertsList } from '../components/AlertsList'
import { Badge, Empty, Loading, OperatorNotice, PageHeader } from '../components/common'
import { ProblemAlert } from '../components/ProblemAlert'
import { ProductForm } from '../components/ProductForm'
import { useAsync } from '../hooks/useAsync'
import { useSubmit } from '../hooks/useSubmit'
import { fmtDate, fmtMoney, fmtQty } from '../lib/format'
import { lotStatusLabel, productStatusLabel, storageTypeLabel, trackingLabel } from '../lib/labels'

export function ProductDetailPage() {
  const { id = '' } = useParams()
  const [editing, setEditing] = useState(false)
  const save = useSubmit()
  const status = useSubmit()

  // Il GET restituisce l'ETag da rimandare in If-Match al PATCH.
  const product = useAsync(async () => {
    const { data, response } = await unwrapResponse(
      api.GET('/products/{productId}', { params: { path: { productId: id } } }),
    )
    return { product: data, etag: response.headers.get('ETag') ?? undefined }
  }, [id])

  const availability = useAsync(
    () => unwrap(api.GET('/products/{productId}/availability', { params: { path: { productId: id } } })),
    [id],
  )
  const lots = useAsync(
    () =>
      unwrap(api.GET('/products/{productId}/lots', { params: { path: { productId: id }, query: { page_size: 200 } } })),
    [id],
  )

  if (product.loading && !product.data) return <Loading />
  if (product.error && !product.data) return <ProblemAlert problem={product.error} />
  if (!product.data) return null
  const { product: p, etag } = product.data

  const submitEdit = async (body: ProductUpdate) => {
    const result = await save.run(() =>
      unwrapResponse(
        api.PATCH('/products/{productId}', {
          params: { path: { productId: id }, header: { ...operatorHeader(), 'If-Match': etag } },
          body,
        }),
      ),
    )
    if (result) {
      product.setData({ product: result.data, etag: result.response.headers.get('ETag') ?? undefined })
      setEditing(false)
    }
  }

  const changeStatus = async (action: 'archive' | 'reactivate') => {
    const params = { params: { path: { productId: id }, header: operatorHeader() } }
    const done = await status.run(() =>
      action === 'archive'
        ? unwrap(api.POST('/products/{productId}/archive', params))
        : unwrap(api.POST('/products/{productId}/reactivate', params)),
    )
    // Ricarico per avere il nuovo ETag.
    if (done) {
      product.reload()
      availability.reload()
    }
  }

  const reloadAll = () => {
    save.setProblem(null)
    product.reload()
    availability.reload()
    lots.reload()
  }

  return (
    <>
      <PageHeader title={`${p.sku} · ${p.name}`}>
        <Link to="/products">← Prodotti</Link>
      </PageHeader>

      <section className="card">
        <div className="card-head">
          <h2>
            Anagrafica{' '}
            <Badge tone={p.status === 'ACTIVE' ? 'ok' : 'neutral'}>{productStatusLabel[p.status]}</Badge>
          </h2>
          <div className="actions">
            {!editing && (
              <button type="button" onClick={() => setEditing(true)}>
                Modifica
              </button>
            )}
            {p.status === 'ACTIVE' ? (
              <button type="button" disabled={status.busy} onClick={() => changeStatus('archive')}>
                Archivia
              </button>
            ) : (
              <button type="button" disabled={status.busy} onClick={() => changeStatus('reactivate')}>
                Riattiva
              </button>
            )}
          </div>
        </div>
        <ProblemAlert problem={status.problem} onDismiss={() => status.setProblem(null)} />

        {editing ? (
          <>
            <OperatorNotice />
            <ProductForm
              key={`${p.id}-${p.version}`}
              mode="edit"
              product={p}
              busy={save.busy}
              onSubmit={submitEdit}
              onCancel={() => {
                save.setProblem(null)
                setEditing(false)
              }}
            />
            <ProblemAlert problem={save.problem} />
            {save.problem?.status === 412 && (
              <button type="button" onClick={reloadAll}>
                Ricarica il prodotto
              </button>
            )}
          </>
        ) : (
          <dl className="props">
            <div>
              <dt>SKU</dt>
              <dd>{p.sku}</dd>
            </div>
            <div>
              <dt>EAN</dt>
              <dd>{p.ean ?? '—'}</dd>
            </div>
            <div>
              <dt>Categoria</dt>
              <dd>{p.category ?? '—'}</dd>
            </div>
            <div>
              <dt>Unità di misura</dt>
              <dd>{p.uom}</dd>
            </div>
            <div>
              <dt>Prezzo unitario</dt>
              <dd>{fmtMoney(p.unit_price, p.currency)}</dd>
            </div>
            <div>
              <dt>Scorta minima</dt>
              <dd>{fmtQty(p.min_stock, p.uom)}</dd>
            </div>
            <div>
              <dt>Tracciabilità</dt>
              <dd>
                {trackingLabel[p.tracking]}
                {p.requires_expiry ? ', con scadenza' : ''}
              </dd>
            </div>
            <div>
              <dt>Preavviso scadenza</dt>
              <dd>{p.expiry_warning_days} giorni</dd>
            </div>
            <div>
              <dt>Peso / volume per unità</dt>
              <dd>
                {fmtQty(p.unit_weight_kg)} kg / {fmtQty(p.unit_volume_m3)} m³
              </dd>
            </div>
            <div>
              <dt>Stoccaggio richiesto</dt>
              <dd>{p.required_storage_type ? storageTypeLabel[p.required_storage_type] : 'Qualsiasi'}</dd>
            </div>
            <div className="span-2">
              <dt>Descrizione</dt>
              <dd>{p.description ?? '—'}</dd>
            </div>
          </dl>
        )}
      </section>

      <section className="card">
        <h2>Disponibilità per magazzino</h2>
        <ProblemAlert problem={availability.error} />
        {availability.loading && <Loading />}
        {availability.data && (
          <div className="table-wrap">
            <table className="table">
              <thead>
                <tr>
                  <th>Magazzino</th>
                  <th className="num">Fisica</th>
                  <th className="num">Non utilizzabile</th>
                  <th className="num">Impegnata</th>
                  <th className="num">Disponibile</th>
                  <th className="num">In scadenza</th>
                  <th className="num">Scaduta</th>
                  <th>Alert</th>
                </tr>
              </thead>
              <tbody>
                {availability.data.warehouses.map((w) => (
                  <tr key={w.warehouse_id}>
                    <td>{w.warehouse_code}</td>
                    <td className="num">{fmtQty(w.physical)}</td>
                    <td className="num">{fmtQty(w.unusable)}</td>
                    <td className="num">{fmtQty(w.reserved)}</td>
                    <td className="num">
                      <strong>{fmtQty(w.available)}</strong>{' '}
                      {w.below_min_stock && <Badge tone="warn">Sotto scorta</Badge>}
                    </td>
                    <td className="num">{fmtQty(w.expiring_quantity)}</td>
                    <td className="num">{fmtQty(w.expired_quantity)}</td>
                    <td>
                      <AlertsList alerts={w.alerts} />
                    </td>
                  </tr>
                ))}
              </tbody>
              <tfoot>
                <tr>
                  <th>Totale</th>
                  <th className="num">{fmtQty(availability.data.totals.physical)}</th>
                  <th className="num">{fmtQty(availability.data.totals.unusable)}</th>
                  <th className="num">{fmtQty(availability.data.totals.reserved)}</th>
                  <th className="num">{fmtQty(availability.data.totals.available)}</th>
                  <th className="num">{fmtQty(availability.data.totals.expiring_quantity)}</th>
                  <th className="num">{fmtQty(availability.data.totals.expired_quantity)}</th>
                  <th />
                </tr>
              </tfoot>
            </table>
          </div>
        )}
      </section>

      <section className="card">
        <h2>Lotti</h2>
        <ProblemAlert problem={lots.error} />
        {lots.loading && <Loading />}
        {lots.data && lots.data.items.length === 0 && <Empty>Nessun lotto registrato.</Empty>}
        {lots.data && lots.data.items.length > 0 && (
          <div className="table-wrap">
            <table className="table">
              <thead>
                <tr>
                  <th>Lotto</th>
                  <th>Produzione</th>
                  <th>Scadenza</th>
                  <th className="num">Giorni</th>
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
                    <td>{fmtDate(lot.production_date)}</td>
                    <td>{fmtDate(lot.expiry_date)}</td>
                    <td className="num">{lot.days_to_expiry ?? '—'}</td>
                    <td className="flags">
                      <Badge tone={lot.status === 'BLOCKED' ? 'danger' : 'ok'}>{lotStatusLabel[lot.status]}</Badge>
                      {lot.expired && <Badge tone="danger">Scaduto</Badge>}
                    </td>
                    <td className="num">{fmtQty(lot.on_hand, p.uom)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </section>
    </>
  )
}
