import { type FormEvent, useState } from 'react'
import { Link } from 'react-router'
import { api, operatorHeader } from '../api/client'
import { clientProblem, unwrap } from '../api/errors'
import type { Location, StorageType } from '../api/types'
import { Badge, Empty, Loading, OperatorNotice, PageHeader, Pagination, RequireWarehouse } from '../components/common'
import { ProblemAlert } from '../components/ProblemAlert'
import { useAsync } from '../hooks/useAsync'
import { useSubmit } from '../hooks/useSubmit'
import { fmtDate, fmtPct, fmtQty, orNull, parseNumber } from '../lib/format'
import { options, storageTypeLabel } from '../lib/labels'
import { useApp } from '../state/appContext'

const PAGE_SIZE = 50

function WarehousesSection() {
  const { warehouses, warehouseId, setWarehouseId, reloadWarehouses, warehousesError } = useApp()
  const [form, setForm] = useState({ code: '', name: '', address: '' })
  const create = useSubmit()

  const submit = async (e: FormEvent) => {
    e.preventDefault()
    const created = await create.run(() =>
      unwrap(
        api.POST('/warehouses', {
          params: { header: operatorHeader() },
          body: { code: form.code.trim(), name: form.name.trim(), address: orNull(form.address) },
        }),
      ),
    )
    if (created) {
      setForm({ code: '', name: '', address: '' })
      reloadWarehouses()
      setWarehouseId(created.id)
    }
  }

  return (
    <section className="card">
      <h2>Magazzini</h2>
      <ProblemAlert problem={warehousesError} />
      {warehouses.length === 0 ? (
        <Empty>Nessun magazzino.</Empty>
      ) : (
        <div className="table-wrap">
          <table className="table">
            <thead>
              <tr>
                <th>Codice</th>
                <th>Nome</th>
                <th>Indirizzo</th>
                <th>Stato</th>
                <th />
              </tr>
            </thead>
            <tbody>
              {warehouses.map((w) => (
                <tr key={w.id} className={w.id === warehouseId ? 'selected' : ''}>
                  <td>{w.code}</td>
                  <td>{w.name}</td>
                  <td>{w.address ?? '—'}</td>
                  <td>{w.active ? <Badge tone="ok">Attivo</Badge> : <Badge>Non attivo</Badge>}</td>
                  <td>
                    {w.id === warehouseId ? (
                      <span className="muted">Selezionato</span>
                    ) : (
                      <button type="button" className="small" onClick={() => setWarehouseId(w.id)}>
                        Seleziona
                      </button>
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
      <h3>Nuovo magazzino</h3>
      <OperatorNotice />
      <form className="inline-form" onSubmit={submit}>
        <input
          placeholder="Codice *"
          value={form.code}
          maxLength={20}
          required
          onChange={(e) => setForm({ ...form, code: e.target.value })}
        />
        <input placeholder="Nome *" value={form.name} required onChange={(e) => setForm({ ...form, name: e.target.value })} />
        <input placeholder="Indirizzo" value={form.address} onChange={(e) => setForm({ ...form, address: e.target.value })} />
        <button type="submit" className="primary" disabled={create.busy}>
          Crea
        </button>
      </form>
      <ProblemAlert problem={create.problem} />
    </section>
  )
}

function ZonesSection({ warehouseId, onChange }: { warehouseId: string; onChange: () => void }) {
  const zones = useAsync(
    () => unwrap(api.GET('/warehouses/{warehouseId}/zones', { params: { path: { warehouseId } } })),
    [warehouseId],
  )
  const [form, setForm] = useState({ code: '', name: '' })
  const create = useSubmit()

  const submit = async (e: FormEvent) => {
    e.preventDefault()
    const created = await create.run(() =>
      unwrap(
        api.POST('/warehouses/{warehouseId}/zones', {
          params: { path: { warehouseId }, header: operatorHeader() },
          body: { code: form.code.trim(), name: form.name.trim() },
        }),
      ),
    )
    if (created) {
      setForm({ code: '', name: '' })
      zones.reload()
      onChange()
    }
  }

  return (
    <section className="card">
      <h2>Zone</h2>
      <ProblemAlert problem={zones.error} />
      {zones.loading && <Loading />}
      {zones.data && zones.data.items.length === 0 && <Empty>Nessuna zona.</Empty>}
      {zones.data && zones.data.items.length > 0 && (
        <ul className="chips">
          {zones.data.items.map((z) => (
            <li key={z.id}>
              <strong>{z.code}</strong> · {z.name}
            </li>
          ))}
        </ul>
      )}
      <h3>Nuova zona</h3>
      <form className="inline-form" onSubmit={submit}>
        <input
          placeholder="Codice *"
          value={form.code}
          maxLength={20}
          required
          onChange={(e) => setForm({ ...form, code: e.target.value })}
        />
        <input placeholder="Nome *" value={form.name} required onChange={(e) => setForm({ ...form, name: e.target.value })} />
        <button type="submit" className="primary" disabled={create.busy}>
          Crea
        </button>
      </form>
      <ProblemAlert problem={create.problem} />
    </section>
  )
}

interface LocationForm {
  zone_id: string
  code: string
  aisle: string
  shelf: string
  level: string
  storage_type: StorageType
  max_weight_kg: string
  max_volume_m3: string
}

const emptyLocation: LocationForm = {
  zone_id: '',
  code: '',
  aisle: '',
  shelf: '',
  level: '',
  storage_type: 'AMBIENT',
  max_weight_kg: '',
  max_volume_m3: '',
}

/** Capienza facoltativa: vuoto = illimitata. */
function capacity(text: string, label: string): number | null {
  if (!text.trim()) return null
  const value = parseNumber(text)
  if (Number.isNaN(value) || value <= 0) throw clientProblem(`${label}: inserire un numero maggiore di zero o lasciare vuoto.`)
  return value
}

function LocationsSection({ warehouseId, zonesVersion }: { warehouseId: string; zonesVersion: number }) {
  const [page, setPage] = useState(1)
  const [zoneFilter, setZoneFilter] = useState('')
  const [selected, setSelected] = useState<Location | null>(null)
  const [form, setForm] = useState<LocationForm>(emptyLocation)
  const create = useSubmit()

  const zones = useAsync(
    () => unwrap(api.GET('/warehouses/{warehouseId}/zones', { params: { path: { warehouseId } } })),
    [warehouseId, zonesVersion],
  )
  const locations = useAsync(
    () =>
      unwrap(
        api.GET('/warehouses/{warehouseId}/locations', {
          params: { path: { warehouseId }, query: { page, page_size: PAGE_SIZE, zone_id: zoneFilter || undefined } },
        }),
      ),
    [warehouseId, page, zoneFilter],
  )

  const submit = async (e: FormEvent) => {
    e.preventDefault()
    const created = await create.run(() =>
      unwrap(
        api.POST('/warehouses/{warehouseId}/locations', {
          params: { path: { warehouseId }, header: operatorHeader() },
          body: {
            zone_id: form.zone_id,
            code: form.code.trim(),
            aisle: orNull(form.aisle),
            shelf: orNull(form.shelf),
            level: orNull(form.level),
            storage_type: form.storage_type,
            max_weight_kg: capacity(form.max_weight_kg, 'Peso massimo'),
            max_volume_m3: capacity(form.max_volume_m3, 'Volume massimo'),
          },
        }),
      ),
    )
    if (created) {
      setForm({ ...emptyLocation, zone_id: form.zone_id, storage_type: form.storage_type })
      locations.reload()
    }
  }

  const zoneItems = zones.data?.items ?? []

  return (
    <>
      <section className="card">
        <div className="card-head">
          <h2>Ubicazioni</h2>
          <label className="field inline">
            <span>Zona</span>
            <select
              value={zoneFilter}
              onChange={(e) => {
                setZoneFilter(e.target.value)
                setPage(1)
              }}
            >
              <option value="">Tutte</option>
              {zoneItems.map((z) => (
                <option key={z.id} value={z.id}>
                  {z.code} · {z.name}
                </option>
              ))}
            </select>
          </label>
        </div>
        <ProblemAlert problem={locations.error} />
        {locations.loading && <Loading />}
        {locations.data && locations.data.items.length === 0 && <Empty>Nessuna ubicazione.</Empty>}
        {locations.data && locations.data.items.length > 0 && (
          <>
            <div className="table-wrap">
              <table className="table">
                <thead>
                  <tr>
                    <th>Codice</th>
                    <th>Zona</th>
                    <th>Corsia / ripiano / livello</th>
                    <th>Stoccaggio</th>
                    <th className="num">Peso (kg)</th>
                    <th className="num">Volume (m³)</th>
                    <th>Stato</th>
                  </tr>
                </thead>
                <tbody>
                  {locations.data.items.map((l) => (
                    <tr key={l.id} className={selected?.id === l.id ? 'selected' : ''}>
                      <td>
                        <button type="button" className="link" onClick={() => setSelected(l)}>
                          {l.code}
                        </button>
                      </td>
                      <td>{l.zone_code ?? '—'}</td>
                      <td>{[l.aisle, l.shelf, l.level].map((x) => x ?? '—').join(' / ')}</td>
                      <td>{storageTypeLabel[l.storage_type]}</td>
                      <td className="num">
                        {fmtQty(l.occupancy.weight_kg)} / {l.max_weight_kg == null ? '∞' : fmtQty(l.max_weight_kg)}
                        <div className="tiny muted">{fmtPct(l.occupancy.weight_pct)}</div>
                      </td>
                      <td className="num">
                        {fmtQty(l.occupancy.volume_m3)} / {l.max_volume_m3 == null ? '∞' : fmtQty(l.max_volume_m3)}
                        <div className="tiny muted">{fmtPct(l.occupancy.volume_pct)}</div>
                      </td>
                      <td className="flags">
                        {l.occupancy.full && <Badge tone="danger">Piena</Badge>}
                        {!l.active && <Badge>Non attiva</Badge>}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
            <Pagination page={page} pageSize={PAGE_SIZE} total={locations.data.total} onPage={setPage} />
          </>
        )}

        <h3>Nuova ubicazione</h3>
        {zoneItems.length === 0 ? (
          <p className="muted">Crea prima una zona.</p>
        ) : (
          <form className="form-grid" onSubmit={submit}>
            <label className="field">
              <span>Zona *</span>
              <select value={form.zone_id} required onChange={(e) => setForm({ ...form, zone_id: e.target.value })}>
                <option value="">— scegli —</option>
                {zoneItems.map((z) => (
                  <option key={z.id} value={z.id}>
                    {z.code} · {z.name}
                  </option>
                ))}
              </select>
            </label>
            <label className="field">
              <span>Codice * (es. A-03-02)</span>
              <input
                value={form.code}
                maxLength={40}
                required
                onChange={(e) => setForm({ ...form, code: e.target.value })}
              />
            </label>
            <label className="field">
              <span>Corsia</span>
              <input value={form.aisle} onChange={(e) => setForm({ ...form, aisle: e.target.value })} />
            </label>
            <label className="field">
              <span>Ripiano</span>
              <input value={form.shelf} onChange={(e) => setForm({ ...form, shelf: e.target.value })} />
            </label>
            <label className="field">
              <span>Livello</span>
              <input value={form.level} onChange={(e) => setForm({ ...form, level: e.target.value })} />
            </label>
            <label className="field">
              <span>Stoccaggio *</span>
              <select
                value={form.storage_type}
                onChange={(e) => setForm({ ...form, storage_type: e.target.value as StorageType })}
              >
                {options(storageTypeLabel).map(([value, label]) => (
                  <option key={value} value={value}>
                    {label}
                  </option>
                ))}
              </select>
            </label>
            <label className="field">
              <span>Peso massimo (kg, vuoto = illimitato)</span>
              <input
                type="number"
                step="any"
                min="0"
                value={form.max_weight_kg}
                onChange={(e) => setForm({ ...form, max_weight_kg: e.target.value })}
              />
            </label>
            <label className="field">
              <span>Volume massimo (m³, vuoto = illimitato)</span>
              <input
                type="number"
                step="any"
                min="0"
                value={form.max_volume_m3}
                onChange={(e) => setForm({ ...form, max_volume_m3: e.target.value })}
              />
            </label>
            <div className="actions span-all">
              <button type="submit" className="primary" disabled={create.busy}>
                Crea ubicazione
              </button>
            </div>
          </form>
        )}
        <ProblemAlert problem={create.problem} />
      </section>

      {selected && <LocationDetail location={selected} onClose={() => setSelected(null)} />}
    </>
  )
}

function LocationDetail({ location, onClose }: { location: Location; onClose: () => void }) {
  const content = useAsync(
    () => unwrap(api.GET('/locations/{locationId}/stock', { params: { path: { locationId: location.id } } })),
    [location.id],
  )
  const loc = content.data?.location ?? location

  return (
    <section className="card">
      <div className="card-head">
        <h2>Contenuto di {loc.code}</h2>
        <button type="button" onClick={onClose}>
          Chiudi
        </button>
      </div>
      <p className="muted">
        Occupazione: {fmtQty(loc.occupancy.weight_kg)} kg ({fmtPct(loc.occupancy.weight_pct)}) ·{' '}
        {fmtQty(loc.occupancy.volume_m3)} m³ ({fmtPct(loc.occupancy.volume_pct)})
        {loc.occupancy.full ? ' · PIENA' : ''}
      </p>
      <ProblemAlert problem={content.error} />
      {content.loading && <Loading />}
      {content.data && content.data.items.length === 0 && <Empty>Ubicazione vuota.</Empty>}
      {content.data && content.data.items.length > 0 && (
        <div className="table-wrap">
          <table className="table">
            <thead>
              <tr>
                <th>SKU</th>
                <th>Prodotto</th>
                <th>Lotto</th>
                <th>Scadenza</th>
                <th className="num">Quantità</th>
                <th>Seriali</th>
                <th />
              </tr>
            </thead>
            <tbody>
              {content.data.items.map((row) => (
                <tr key={row.id}>
                  <td>
                    <Link to={`/products/${row.product_id}`}>{row.sku}</Link>
                  </td>
                  <td>{row.product_name}</td>
                  <td>{row.lot_id ? <Link to={`/lots/${row.lot_id}`}>{row.lot_code}</Link> : '—'}</td>
                  <td>{fmtDate(row.expiry_date)}</td>
                  <td className="num">{fmtQty(row.quantity, row.uom)}</td>
                  <td className="serials">{row.serials?.join(', ') || '—'}</td>
                  <td>{!row.usable && <Badge tone="danger">Non utilizzabile</Badge>}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </section>
  )
}

export function WarehousePage() {
  const { warehouseId, warehouse } = useApp()
  const [zonesVersion, setZonesVersion] = useState(0)

  return (
    <>
      <PageHeader title="Magazzino" />
      <WarehousesSection />
      <RequireWarehouse>
        <h2 className="section-title">
          Mappa di {warehouse?.code} · {warehouse?.name}
        </h2>
        <ZonesSection key={`z-${warehouseId}`} warehouseId={warehouseId} onChange={() => setZonesVersion((v) => v + 1)} />
        <LocationsSection key={`l-${warehouseId}`} warehouseId={warehouseId} zonesVersion={zonesVersion} />
      </RequireWarehouse>
    </>
  )
}
