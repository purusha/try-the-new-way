// Errori RFC 9457 (application/problem+json) e messaggi in italiano per ogni ErrorCode del contratto.
import type { ErrorCode } from './types'

/** Codici prodotti dal client stesso (non dal BE). */
export type ClientErrorCode = 'NETWORK_ERROR' | 'OPERATOR_NOT_SET' | 'CLIENT_VALIDATION' | 'HTTP_ERROR'

export interface Problem {
  type?: string
  title: string
  status: number
  detail?: string
  code: ErrorCode | ClientErrorCode | (string & {})
  details?: Record<string, unknown>
}

/** Elemento di `details.earlier_lots` in FEFO_VIOLATION. */
export interface EarlierLot {
  lot_id?: string
  lot_code?: string
  expiry_date?: string
  quantity?: number
}

export function earlierLotsOf(problem: Problem): EarlierLot[] {
  const lots = problem.details?.earlier_lots
  return Array.isArray(lots) ? (lots as EarlierLot[]) : []
}

export class ApiError extends Error {
  readonly problem: Problem

  constructor(problem: Problem) {
    super(problem.detail ?? problem.title)
    this.name = 'ApiError'
    this.problem = problem
  }
}

const serverMessages: Record<ErrorCode, string> = {
  VALIDATION_ERROR: 'I dati inviati non sono validi.',
  NOT_FOUND: 'La risorsa richiesta non esiste.',
  OPERATOR_MISSING: "Il nome dell'operatore è obbligatorio per le operazioni di scrittura.",
  METHOD_NOT_ALLOWED: 'Operazione non consentita.',
  VERSION_CONFLICT:
    'Il dato è stato modificato da un altro utente nel frattempo. Ricarica la pagina e ripeti la modifica.',
  IDEMPOTENCY_KEY_REUSED: 'La stessa richiesta è stata inviata di nuovo con dati diversi.',
  IDEMPOTENCY_IN_PROGRESS: 'La stessa richiesta è ancora in elaborazione. Attendi qualche secondo.',
  DUPLICATE_CODE: 'Esiste già un elemento con questo codice.',
  DUPLICATE_SKU: 'Esiste già un prodotto con questo SKU.',
  DUPLICATE_EAN: 'Esiste già un prodotto con questo EAN.',
  DUPLICATE_LOT: 'Esiste già un lotto con questo codice per il prodotto.',
  DUPLICATE_ORDER_NUMBER: 'Esiste già un ordine con questo numero.',
  PRODUCT_INACTIVE: 'Il prodotto è archiviato: non accetta carichi né prenotazioni.',
  PRODUCT_HAS_STOCK: 'Il prodotto ha ancora giacenza o impegni aperti e non può essere archiviato.',
  WAREHOUSE_INACTIVE: 'Il magazzino non è attivo.',
  LOCATION_INACTIVE: "L'ubicazione non è attiva.",
  LOCATION_NOT_EMPTY: "L'ubicazione contiene merce e non può essere disattivata.",
  LOCATION_WAREHOUSE_MISMATCH: "L'ubicazione non appartiene al magazzino indicato.",
  STORAGE_TYPE_MISMATCH: "Il tipo di stoccaggio dell'ubicazione non è adatto al prodotto.",
  LOCATION_CAPACITY_EXCEEDED: "L'ubicazione supererebbe la sua capienza massima.",
  INSUFFICIENT_STOCK: "Giacenza insufficiente nell'ubicazione o nel lotto indicato.",
  INSUFFICIENT_AVAILABILITY: 'Disponibilità insufficiente nel magazzino.',
  LOT_EXPIRED: 'Il lotto è scaduto.',
  LOT_BLOCKED: 'Il lotto è bloccato.',
  LOT_REQUIRED: 'Per questo prodotto il lotto è obbligatorio.',
  LOT_NOT_ALLOWED: 'Questo prodotto non è gestito a lotti.',
  EXPIRY_DATE_REQUIRED: 'Per questo prodotto la data di scadenza del lotto è obbligatoria.',
  LOT_PRODUCT_MISMATCH: 'Il lotto indicato appartiene a un altro prodotto.',
  LOT_DATA_MISMATCH: 'Le date indicate non coincidono con quelle del lotto già registrato.',
  FEFO_VIOLATION: 'Esistono lotti con scadenza anteriore da prelevare prima (FEFO).',
  SERIALS_REQUIRED: 'Per questo prodotto vanno indicati i numeri di serie.',
  SERIALS_NOT_ALLOWED: 'Questo prodotto non è gestito a seriale.',
  SERIAL_COUNT_MISMATCH: 'Il numero di seriali non coincide con la quantità.',
  SERIAL_ALREADY_EXISTS: 'Uno o più seriali sono già registrati.',
  SERIAL_NOT_AVAILABLE: "Uno o più seriali non sono presenti nell'ubicazione indicata.",
  SAME_LOCATION_TRANSFER: "L'ubicazione di origine e quella di destinazione coincidono.",
  CROSS_WAREHOUSE_TRANSFER_NOT_SUPPORTED: 'I trasferimenti tra magazzini diversi non sono supportati.',
  ADJUSTMENT_REASON_REQUIRED: 'La causale è obbligatoria (con la nota, se la causale è "Altro").',
  MOVEMENT_IMMUTABLE: 'I movimenti confermati non si possono modificare né eliminare.',
  MOVEMENT_ALREADY_REVERSED: 'Il movimento è già stato stornato.',
  MOVEMENT_NOT_REVERSIBLE: 'Questo movimento non può essere stornato.',
  ORDER_STATE_INVALID: "L'operazione non è ammessa nello stato attuale dell'ordine.",
  ORDER_LINE_NOT_FOUND: "La riga indicata non appartiene all'ordine.",
  RESERVATION_EXCEEDS_ORDERED: 'La quantità da prenotare supera il residuo ordinato.',
  RELEASE_EXCEEDS_RESERVED: 'La quantità da rilasciare supera quella impegnata.',
  FULFILLMENT_EXCEEDS_RESERVED: 'Si può evadere solo la quantità già impegnata.',
  QUANTITY_BELOW_FULFILLED: 'La quantità non può scendere sotto quella già evasa.',
  RECEIPT_EXCEEDS_ORDERED: "La quantità ricevuta supera quella dell'ordine fornitore.",
  PURCHASE_ORDER_MISMATCH: "La riga non corrisponde all'ordine fornitore o al prodotto indicato.",
  INTERNAL_ERROR: 'Errore interno del server. Riprova più tardi.',
}

const clientMessages: Record<ClientErrorCode, string> = {
  NETWORK_ERROR: 'Impossibile contattare il server.',
  OPERATOR_NOT_SET: "Imposta il nome dell'operatore in alto a destra prima di registrare operazioni.",
  CLIENT_VALIDATION: 'Controlla i dati inseriti.',
  HTTP_ERROR: 'Il server ha risposto con un errore.',
}

const allMessages: Record<string, string> = { ...serverMessages, ...clientMessages }

/** Messaggio principale in italiano; se il codice non è noto usa detail o title. */
export function problemMessage(problem: Problem): string {
  return allMessages[problem.code] ?? problem.detail ?? problem.title
}

function isProblemLike(value: unknown): value is Problem {
  return typeof value === 'object' && value !== null && 'code' in value && 'title' in value
}

/** Normalizza il corpo d'errore di openapi-fetch in un Problem. */
export function toProblem(error: unknown, response: Response): Problem {
  if (isProblemLike(error)) return { ...error, status: error.status ?? response.status }
  return {
    status: response.status,
    code: response.status >= 500 ? 'INTERNAL_ERROR' : 'HTTP_ERROR',
    title: `Errore HTTP ${response.status}`,
    detail: typeof error === 'string' && error.trim() ? error.slice(0, 300) : response.statusText || undefined,
  }
}

/** Converte qualunque eccezione in un Problem da mostrare. */
export function asProblem(error: unknown): Problem {
  if (error instanceof ApiError) return error.problem
  return {
    status: 0,
    code: 'NETWORK_ERROR',
    title: 'Errore di rete',
    detail: error instanceof Error ? error.message : String(error),
  }
}

export function clientProblem(detail: string): ApiError {
  return new ApiError({ status: 0, code: 'CLIENT_VALIDATION', title: 'Dati non validi', detail })
}

interface FetchResult<T> {
  data?: T
  error?: unknown
  response: Response
}

/** Attende una chiamata openapi-fetch: restituisce i dati o lancia ApiError. */
export async function unwrap<T>(promise: Promise<FetchResult<T>>): Promise<T> {
  const result = await unwrapResponse(promise)
  return result.data
}

/** Come unwrap, ma restituisce anche la Response (es. per leggere l'ETag). */
export async function unwrapResponse<T>(
  promise: Promise<FetchResult<T>>,
): Promise<{ data: T; response: Response }> {
  let result: FetchResult<T>
  try {
    result = await promise
  } catch (error) {
    if (error instanceof ApiError) throw error
    throw new ApiError(asProblem(error))
  }
  if (!result.response.ok || result.error !== undefined) {
    throw new ApiError(toProblem(result.error, result.response))
  }
  return { data: result.data as T, response: result.response }
}
