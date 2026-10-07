import type { Location, Lot, Movement, StockItem } from '../api/types'
import { fmtDate, fmtPct, fmtQty } from './format'

// Descrizioni testuali brevi per i menu a tendina.

export function locationLabel(l: Location): string {
  const parts = [l.code]
  if (l.occupancy.full) parts.push('PIENA')
  else if (l.occupancy.weight_pct != null || l.occupancy.volume_pct != null) {
    parts.push(`peso ${fmtPct(l.occupancy.weight_pct)}, vol. ${fmtPct(l.occupancy.volume_pct)}`)
  }
  return parts.join(' · ')
}

export function lotLabel(lot: Lot): string {
  const parts = [lot.lot_code]
  if (lot.expiry_date) parts.push(`scad. ${fmtDate(lot.expiry_date)}`)
  if (lot.expired) parts.push('SCADUTO')
  if (lot.status === 'BLOCKED') parts.push('BLOCCATO')
  parts.push(`giac. ${fmtQty(lot.on_hand)}`)
  return parts.join(' · ')
}

export function stockRowKey(row: { location_id: string; lot_id?: string | null }): string {
  return `${row.location_id}|${row.lot_id ?? ''}`
}

export function stockRowLabel(row: StockItem): string {
  const parts = [row.location_code]
  if (row.lot_code) parts.push(`lotto ${row.lot_code}`)
  if (row.expiry_date) parts.push(`scad. ${fmtDate(row.expiry_date)}`)
  parts.push(fmtQty(row.quantity, row.uom))
  if (!row.usable) parts.push('NON UTILIZZABILE')
  return parts.join(' · ')
}

/** Storno ammesso: non per impegni/disimpegni, non per gli storni, non due volte. */
export function isReversible(m: Movement): boolean {
  return (
    m.type !== 'RESERVATION' && m.type !== 'RELEASE' && !m.reverses_movement_id && !m.reversed_by_movement_id
  )
}
