import { useState } from 'react'
import { api } from '../api/client'
import { unwrap } from '../api/errors'
import type { ProductListItem } from '../api/types'
import { useAsync } from '../hooks/useAsync'
import { useDebounced } from '../hooks/useDebounced'
import { trackingLabel } from '../lib/labels'
import { fmtQty } from '../lib/format'
import { Badge } from './common'

interface Props {
  value: ProductListItem | null
  onChange: (product: ProductListItem | null) => void
  /** Per mostrare il disponibile del magazzino accanto ai risultati. */
  warehouseId?: string
  onlyActive?: boolean
  placeholder?: string
}

/** Ricerca di un prodotto per SKU, EAN o nome (GET /products?q=). */
export function ProductPicker({ value, onChange, warehouseId, onlyActive, placeholder }: Props) {
  const [query, setQuery] = useState('')
  const q = useDebounced(query.trim(), 250)

  const results = useAsync(
    () =>
      unwrap(
        api.GET('/products', {
          params: {
            query: {
              q,
              page_size: 10,
              warehouse_id: warehouseId || undefined,
              status: onlyActive ? 'ACTIVE' : undefined,
            },
          },
        }),
      ),
    [q, warehouseId, onlyActive],
    !value && q.length > 0,
  )

  if (value) {
    return (
      <div className="picker-value">
        <span>
          <strong>{value.sku}</strong> · {value.name}{' '}
          {value.tracking !== 'NONE' && <Badge tone="info">{trackingLabel[value.tracking]}</Badge>}
        </span>
        <button
          type="button"
          className="link"
          onClick={() => {
            onChange(null)
            setQuery('')
          }}
        >
          Cambia
        </button>
      </div>
    )
  }

  const items = results.data?.items ?? []
  return (
    <div className="picker">
      <input
        type="search"
        value={query}
        placeholder={placeholder ?? 'Cerca per SKU, EAN o nome…'}
        onChange={(e) => setQuery(e.target.value)}
      />
      {q.length > 0 && (
        <ul className="picker-results">
          {results.loading && <li className="muted">Ricerca…</li>}
          {results.error && <li className="error-text">Errore nella ricerca</li>}
          {!results.loading && !results.error && items.length === 0 && <li className="muted">Nessun prodotto</li>}
          {!results.loading &&
            items.map((p) => (
              <li key={p.id}>
                <button type="button" onClick={() => onChange(p)}>
                  <strong>{p.sku}</strong> · {p.name}
                  <span className="muted">
                    {' '}
                    — disp. {fmtQty(p.stock.available, p.uom)}
                    {p.status === 'INACTIVE' ? ' · archiviato' : ''}
                  </span>
                </button>
              </li>
            ))}
        </ul>
      )}
    </div>
  )
}
