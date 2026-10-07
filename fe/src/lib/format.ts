const LOCALE = 'it-IT'

const qtyFormat = new Intl.NumberFormat(LOCALE, { maximumFractionDigits: 3 })
const pctFormat = new Intl.NumberFormat(LOCALE, { maximumFractionDigits: 1 })
const dateFormat = new Intl.DateTimeFormat(LOCALE, { dateStyle: 'short' })
const dateTimeFormat = new Intl.DateTimeFormat(LOCALE, { dateStyle: 'short', timeStyle: 'short' })

export function fmtQty(value: number | null | undefined, uom?: string | null): string {
  if (value === null || value === undefined) return '—'
  const text = qtyFormat.format(value)
  return uom ? `${text} ${uom}` : text
}

export function fmtMoney(value: number | null | undefined, currency = 'EUR'): string {
  if (value === null || value === undefined) return '—'
  try {
    return new Intl.NumberFormat(LOCALE, { style: 'currency', currency }).format(value)
  } catch {
    return `${qtyFormat.format(value)} ${currency}`
  }
}

export function fmtPct(value: number | null | undefined): string {
  if (value === null || value === undefined) return '—'
  return `${pctFormat.format(value)}%`
}

/** Date senza orario (YYYY-MM-DD): interpretate come data locale, senza slittamenti di fuso. */
export function fmtDate(value: string | null | undefined): string {
  if (!value) return '—'
  const [y, m, d] = value.slice(0, 10).split('-').map(Number)
  if (!y || !m || !d) return value
  return dateFormat.format(new Date(y, m - 1, d))
}

export function fmtDateTime(value: string | null | undefined): string {
  if (!value) return '—'
  const date = new Date(value)
  return Number.isNaN(date.getTime()) ? value : dateTimeFormat.format(date)
}

/** Converte un testo inserito dall'utente (anche con la virgola) in numero; NaN se vuoto o non valido. */
export function parseNumber(text: string): number {
  const trimmed = text.trim().replace(',', '.')
  return trimmed === '' ? Number.NaN : Number(trimmed)
}

/** Seriali da una textarea: uno per riga, senza righe vuote. */
export function parseSerials(text: string): string[] {
  return text
    .split(/\r?\n/)
    .map((s) => s.trim())
    .filter(Boolean)
}

/** Inizio del giorno locale in ISO (per i filtri date-time). */
export function dayStartIso(date: string, addDays = 0): string {
  const [y, m, d] = date.split('-').map(Number)
  return new Date(y, m - 1, d + addDays).toISOString()
}

/** Restituisce undefined per le stringhe vuote (parametri di query facoltativi). */
export function opt(value: string): string | undefined {
  const trimmed = value.trim()
  return trimmed ? trimmed : undefined
}

/** Restituisce null per le stringhe vuote (campi nullable del body). */
export function orNull(value: string): string | null {
  const trimmed = value.trim()
  return trimmed ? trimmed : null
}
