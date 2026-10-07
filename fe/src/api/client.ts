import createClient, { type Middleware } from 'openapi-fetch'
import { readStored, writeStored } from '../lib/storage'
import { ApiError } from './errors'
import type { paths } from './schema'

const OPERATOR_KEY = 'operator'

export function getOperator(): string {
  return readStored(OPERATOR_KEY).trim()
}

export function setOperator(name: string): void {
  writeStored(OPERATOR_KEY, name)
}

/** Aggiunge X-Operator a tutte le richieste di scrittura, se non già presente. */
const operatorMiddleware: Middleware = {
  onRequest({ request }) {
    if (request.method === 'GET' || request.headers.has('X-Operator')) return undefined
    const operator = getOperator()
    if (operator) request.headers.set('X-Operator', operator)
    return request
  },
}

export const api = createClient<paths>({ baseUrl: '/api/v1' })
api.use(operatorMiddleware)

/**
 * Header richiesti dalle scritture. Lancia un errore lato client se l'operatore non è impostato,
 * così la richiesta non parte e l'utente vede il messaggio OPERATOR_NOT_SET.
 */
export function operatorHeader(): { 'X-Operator': string } {
  const operator = getOperator()
  if (!operator) {
    throw new ApiError({
      status: 0,
      code: 'OPERATOR_NOT_SET',
      title: 'Operatore non impostato',
    })
  }
  return { 'X-Operator': operator }
}

/** Header per le POST idempotenti: la chiave va generata una volta per ogni invio dell'utente. */
export function writeHeaders(idempotencyKey: string): { 'X-Operator': string; 'Idempotency-Key': string } {
  return { ...operatorHeader(), 'Idempotency-Key': idempotencyKey }
}

export function newIdempotencyKey(): string {
  if (typeof crypto.randomUUID === 'function') return crypto.randomUUID()
  // crypto.randomUUID esiste solo in contesti sicuri (https o localhost).
  const bytes = crypto.getRandomValues(new Uint8Array(16))
  bytes[6] = (bytes[6] & 0x0f) | 0x40
  bytes[8] = (bytes[8] & 0x3f) | 0x80
  const hex = Array.from(bytes, (b) => b.toString(16).padStart(2, '0')).join('')
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`
}
