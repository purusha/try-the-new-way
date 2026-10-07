// Etichette italiane degli enum del contratto.
import type {
  AdjustmentReason,
  AlertSeverity,
  AlertType,
  LotStatus,
  MovementType,
  PickStrategy,
  ProductStatus,
  PurchaseOrderStatus,
  SalesOrderStatus,
  SerialStatus,
  StorageType,
  Tracking,
  Uom,
} from '../api/types'

export const movementTypeLabel: Record<MovementType, string> = {
  INBOUND: 'Carico',
  OUTBOUND: 'Scarico',
  TRANSFER: 'Trasferimento',
  ADJUSTMENT: 'Rettifica',
  RESERVATION: 'Impegno',
  RELEASE: 'Disimpegno',
}

export const alertTypeLabel: Record<AlertType, string> = {
  LOW_STOCK: 'Sotto scorta',
  EXPIRING: 'In scadenza',
  EXPIRED: 'Scaduti',
  RESERVATION_AT_RISK: 'Impegni a rischio',
}

export const severityLabel: Record<AlertSeverity, string> = {
  INFO: 'Info',
  WARNING: 'Attenzione',
  CRITICAL: 'Critico',
}

export const salesOrderStatusLabel: Record<SalesOrderStatus, string> = {
  OPEN: 'Aperto',
  PARTIALLY_RESERVED: 'Prenotato in parte',
  RESERVED: 'Prenotato',
  PARTIALLY_FULFILLED: 'Evaso in parte',
  FULFILLED: 'Evaso',
  CANCELLED: 'Annullato',
}

export const purchaseOrderStatusLabel: Record<PurchaseOrderStatus, string> = {
  OPEN: 'Aperto',
  PARTIALLY_RECEIVED: 'Ricevuto in parte',
  RECEIVED: 'Ricevuto',
  CANCELLED: 'Annullato',
}

export const storageTypeLabel: Record<StorageType, string> = {
  AMBIENT: 'Ambiente',
  REFRIGERATED: 'Refrigerato',
  FROZEN: 'Surgelato',
  HAZARDOUS: 'Merci pericolose',
  BULK: 'Sfuso / ingombranti',
}

export const uomLabel: Record<Uom, string> = {
  PCS: 'Pezzi (PCS)',
  KG: 'Chilogrammi (KG)',
  M: 'Metri (M)',
  L: 'Litri (L)',
}

export const trackingLabel: Record<Tracking, string> = {
  NONE: 'Nessuna',
  LOT: 'A lotto',
  SERIAL: 'A seriale',
}

export const productStatusLabel: Record<ProductStatus, string> = {
  ACTIVE: 'Attivo',
  INACTIVE: 'Archiviato',
}

export const lotStatusLabel: Record<LotStatus, string> = {
  ACTIVE: 'Attivo',
  BLOCKED: 'Bloccato',
}

export const serialStatusLabel: Record<SerialStatus, string> = {
  IN_STOCK: 'In giacenza',
  SHIPPED: 'Spedito',
  SCRAPPED: 'Dismesso',
}

export const adjustmentReasonLabel: Record<AdjustmentReason, string> = {
  COUNT: 'Conteggio inventariale',
  DAMAGE: 'Danneggiamento',
  LOSS: 'Smarrimento',
  FOUND: 'Ritrovamento',
  EXPIRED_DISPOSAL: 'Smaltimento scaduti',
  OTHER: 'Altro',
}

export const strategyLabel: Record<PickStrategy, string> = {
  AUTO: 'Automatica (FEFO se con scadenza, altrimenti FIFO)',
  FEFO: 'FEFO (prima scade, prima esce)',
  FIFO: 'FIFO (primo entrato, primo uscito)',
}

/** Opzioni per i <select> a partire da una mappa di etichette. */
export function options<K extends string>(labels: Record<K, string>): [K, string][] {
  return Object.entries(labels) as [K, string][]
}
