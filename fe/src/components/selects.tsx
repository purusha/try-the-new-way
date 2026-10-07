import { api } from '../api/client'
import { unwrap } from '../api/errors'
import type { Location, Lot, StockItem } from '../api/types'
import { useAsync } from '../hooks/useAsync'
import { locationLabel, lotLabel, stockRowKey, stockRowLabel } from '../lib/describe'

interface LocationSelectProps {
  locations: Location[]
  value: string
  onChange: (id: string) => void
  /** Ubicazione da escludere (es. origine di un trasferimento). */
  exclude?: string
  emptyLabel?: string
  required?: boolean
}

export function LocationSelect({ locations, value, onChange, exclude, emptyLabel, required }: LocationSelectProps) {
  return (
    <select value={value} onChange={(e) => onChange(e.target.value)} required={required}>
      <option value="">{emptyLabel ?? '— scegli ubicazione —'}</option>
      {locations
        .filter((l) => l.id !== exclude)
        .map((l) => (
          <option key={l.id} value={l.id}>
            {locationLabel(l)}
          </option>
        ))}
    </select>
  )
}

interface LotSelectProps {
  productId: string
  value: string
  onChange: (id: string, lot: Lot | undefined) => void
  emptyLabel?: string
  withStock?: boolean
}

/** Lotti di un prodotto (GET /products/{id}/lots). */
export function LotSelect({ productId, value, onChange, emptyLabel, withStock }: LotSelectProps) {
  const lots = useAsync(
    () =>
      unwrap(
        api.GET('/products/{productId}/lots', {
          params: { path: { productId }, query: { page_size: 200, with_stock: withStock || undefined } },
        }),
      ),
    [productId, withStock],
    Boolean(productId),
  )
  const items = lots.data?.items ?? []
  return (
    <select
      value={value}
      onChange={(e) =>
        onChange(
          e.target.value,
          items.find((l) => l.id === e.target.value),
        )
      }
    >
      <option value="">{lots.loading ? 'Caricamento lotti…' : (emptyLabel ?? '— nessun lotto —')}</option>
      {items.map((lot) => (
        <option key={lot.id} value={lot.id}>
          {lotLabel(lot)}
        </option>
      ))}
    </select>
  )
}

interface StockRowSelectProps {
  rows: StockItem[]
  loading?: boolean
  value: string
  onChange: (key: string, row: StockItem | undefined) => void
}

/** Scelta di una riga di giacenza (ubicazione + lotto) di un prodotto. */
export function StockRowSelect({ rows, loading, value, onChange }: StockRowSelectProps) {
  return (
    <select
      value={value}
      onChange={(e) =>
        onChange(
          e.target.value,
          rows.find((r) => stockRowKey(r) === e.target.value),
        )
      }
    >
      <option value="">{loading ? 'Caricamento giacenze…' : rows.length ? '— scegli da dove —' : 'Nessuna giacenza'}</option>
      {rows.map((row) => (
        <option key={row.id} value={stockRowKey(row)}>
          {stockRowLabel(row)}
        </option>
      ))}
    </select>
  )
}
