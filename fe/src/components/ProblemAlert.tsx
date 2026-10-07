import { type EarlierLot, earlierLotsOf, type Problem, problemMessage } from '../api/errors'
import { fmtDate, fmtQty } from '../lib/format'

const detailLabels: Record<string, string> = {
  sku: 'SKU',
  location_code: 'Ubicazione',
  lot_code: 'Lotto',
  picked_lot_code: 'Lotto scelto',
  picked_expiry_date: 'Scadenza lotto scelto',
  expiry_date: 'Scadenza',
  warehouse_code: 'Magazzino',
  dimension: 'Limite superato',
  current: 'Occupazione attuale',
  incoming: 'In arrivo',
  max: 'Massimo',
  requested: 'Richiesti',
  available: 'Disponibili',
  on_hand: 'In giacenza',
  quantity: 'Quantità',
  reserved: 'Impegnati',
  ordered: 'Ordinati',
  fulfilled: 'Evasi',
  received: 'Ricevuti',
  expected: 'Attesi',
  actual: 'Indicati',
  status: 'Stato',
  field: 'Campo',
  serials: 'Seriali',
  required_storage_type: 'Stoccaggio richiesto',
  storage_type: 'Stoccaggio ubicazione',
}

const dimensionLabels: Record<string, string> = { WEIGHT: 'Peso (kg)', VOLUME: 'Volume (m³)' }

/** I campi tecnici (id) non aiutano l'utente. */
function isHidden(key: string): boolean {
  return key === 'id' || key.endsWith('_id') || key === 'earlier_lots'
}

function formatValue(key: string, value: unknown): string {
  if (value === null || value === undefined) return '—'
  if (key === 'dimension' && typeof value === 'string') return dimensionLabels[value] ?? value
  if (typeof value === 'number') return fmtQty(value)
  if (key.endsWith('_date') && typeof value === 'string') return fmtDate(value)
  if (Array.isArray(value)) return value.map((v) => (typeof v === 'object' ? JSON.stringify(v) : String(v))).join(', ')
  if (typeof value === 'object') return JSON.stringify(value)
  return String(value)
}

export function EarlierLots({ lots }: { lots: EarlierLot[] }) {
  if (!lots.length) return null
  return (
    <table className="table compact">
      <thead>
        <tr>
          <th>Lotto da prelevare prima</th>
          <th>Scadenza</th>
          <th className="num">Quantità</th>
        </tr>
      </thead>
      <tbody>
        {lots.map((lot, i) => (
          <tr key={lot.lot_id ?? i}>
            <td>{lot.lot_code ?? '—'}</td>
            <td>{fmtDate(lot.expiry_date)}</td>
            <td className="num">{fmtQty(lot.quantity)}</td>
          </tr>
        ))}
      </tbody>
    </table>
  )
}

interface Props {
  problem: Problem | null | undefined
  onDismiss?: () => void
}

/** Mostra un errore RFC 9457: messaggio italiano, dettaglio del server e dati principali. */
export function ProblemAlert({ problem, onDismiss }: Props) {
  if (!problem) return null
  const message = problemMessage(problem)
  const entries = Object.entries(problem.details ?? {}).filter(([key]) => !isHidden(key))

  return (
    <div className="alert alert-error" role="alert">
      <div className="alert-head">
        <strong>{message}</strong>
        {onDismiss && (
          <button type="button" className="link" onClick={onDismiss} aria-label="Chiudi">
            ✕
          </button>
        )}
      </div>
      {problem.detail && problem.detail !== message && <p className="muted">{problem.detail}</p>}
      {entries.length > 0 && (
        <dl className="details">
          {entries.map(([key, value]) => (
            <div key={key}>
              <dt>{detailLabels[key] ?? key}</dt>
              <dd>{formatValue(key, value)}</dd>
            </div>
          ))}
        </dl>
      )}
      <EarlierLots lots={earlierLotsOf(problem)} />
      {problem.code && (
        <p className="tiny muted">
          Codice: {problem.code}
          {problem.status ? ` · HTTP ${problem.status}` : ''}
        </p>
      )}
    </div>
  )
}
