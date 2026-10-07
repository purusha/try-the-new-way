import { type FormEvent, useState } from 'react'
import { asProblem, clientProblem, type Problem } from '../api/errors'
import type { Product, ProductCreate, ProductUpdate, StorageType, Tracking, Uom } from '../api/types'
import { parseNumber, orNull } from '../lib/format'
import { options, storageTypeLabel, trackingLabel, uomLabel } from '../lib/labels'
import { ProblemAlert } from './ProblemAlert'

interface FormState {
  sku: string
  ean: string
  name: string
  description: string
  category: string
  uom: Uom
  unit_price: string
  currency: string
  min_stock: string
  tracking: Tracking
  requires_expiry: boolean
  expiry_warning_days: string
  unit_weight_kg: string
  unit_volume_m3: string
  required_storage_type: StorageType | ''
}

function initialState(p?: Product): FormState {
  return {
    sku: p?.sku ?? '',
    ean: p?.ean ?? '',
    name: p?.name ?? '',
    description: p?.description ?? '',
    category: p?.category ?? '',
    uom: p?.uom ?? 'PCS',
    unit_price: String(p?.unit_price ?? 0),
    currency: p?.currency ?? 'EUR',
    min_stock: String(p?.min_stock ?? 0),
    tracking: p?.tracking ?? 'NONE',
    requires_expiry: p?.requires_expiry ?? false,
    expiry_warning_days: String(p?.expiry_warning_days ?? 30),
    unit_weight_kg: String(p?.unit_weight_kg ?? 0),
    unit_volume_m3: String(p?.unit_volume_m3 ?? 0),
    required_storage_type: p?.required_storage_type ?? '',
  }
}

function num(text: string, label: string): number {
  const value = parseNumber(text)
  if (Number.isNaN(value) || value < 0) throw clientProblem(`${label}: inserire un numero maggiore o uguale a zero.`)
  return value
}

function toUpdate(f: FormState): ProductUpdate {
  return {
    ean: orNull(f.ean),
    name: f.name.trim(),
    description: orNull(f.description),
    category: orNull(f.category),
    uom: f.uom,
    unit_price: num(f.unit_price, 'Prezzo unitario'),
    currency: f.currency.trim().toUpperCase(),
    min_stock: num(f.min_stock, 'Scorta minima'),
    expiry_warning_days: Math.trunc(num(f.expiry_warning_days, 'Giorni di preavviso scadenza')),
    unit_weight_kg: num(f.unit_weight_kg, 'Peso unitario'),
    unit_volume_m3: num(f.unit_volume_m3, 'Volume unitario'),
    required_storage_type: f.required_storage_type || null,
  }
}

function toCreate(f: FormState): ProductCreate {
  return {
    ...toUpdate(f),
    sku: f.sku.trim(),
    name: f.name.trim(),
    uom: f.uom,
    tracking: f.tracking,
    requires_expiry: f.tracking === 'NONE' ? false : f.requires_expiry,
  }
}

type Props =
  | { mode: 'create'; busy: boolean; onSubmit: (body: ProductCreate) => void; onCancel?: () => void }
  | { mode: 'edit'; product: Product; busy: boolean; onSubmit: (body: ProductUpdate) => void; onCancel?: () => void }

/**
 * Anagrafica prodotto. In modifica SKU, tracciabilità e obbligo di scadenza non sono modificabili (contratto).
 * Gli errori di conversione dei numeri sono lanciati come ApiError: il chiamante li mostra con ProblemAlert.
 */
export function ProductForm(props: Props) {
  const [f, setF] = useState<FormState>(() => initialState(props.mode === 'edit' ? props.product : undefined))
  const [inputProblem, setInputProblem] = useState<Problem | null>(null)
  const isEdit = props.mode === 'edit'
  const set = <K extends keyof FormState>(key: K, value: FormState[K]) => setF((prev) => ({ ...prev, [key]: value }))

  const submit = (e: FormEvent) => {
    e.preventDefault()
    setInputProblem(null)
    try {
      if (props.mode === 'create') props.onSubmit(toCreate(f))
      else props.onSubmit(toUpdate(f))
    } catch (error) {
      setInputProblem(asProblem(error))
    }
  }

  return (
    <form className="form-grid" onSubmit={submit}>
      <label className="field">
        <span>SKU *</span>
        <input value={f.sku} onChange={(e) => set('sku', e.target.value)} required maxLength={64} disabled={isEdit} />
      </label>
      <label className="field">
        <span>EAN</span>
        <input value={f.ean} onChange={(e) => set('ean', e.target.value)} pattern="[0-9]{8,14}" title="Da 8 a 14 cifre" />
      </label>
      <label className="field span-2">
        <span>Nome *</span>
        <input value={f.name} onChange={(e) => set('name', e.target.value)} required />
      </label>
      <label className="field span-2">
        <span>Descrizione</span>
        <textarea value={f.description} rows={2} onChange={(e) => set('description', e.target.value)} />
      </label>
      <label className="field">
        <span>Categoria</span>
        <input value={f.category} onChange={(e) => set('category', e.target.value)} />
      </label>
      <label className="field">
        <span>Unità di misura *</span>
        <select value={f.uom} onChange={(e) => set('uom', e.target.value as Uom)}>
          {options(uomLabel).map(([value, label]) => (
            <option key={value} value={value}>
              {label}
            </option>
          ))}
        </select>
      </label>
      <label className="field">
        <span>Prezzo unitario</span>
        <input type="number" step="any" min="0" value={f.unit_price} onChange={(e) => set('unit_price', e.target.value)} />
      </label>
      <label className="field">
        <span>Valuta</span>
        <input value={f.currency} onChange={(e) => set('currency', e.target.value)} minLength={3} maxLength={3} />
      </label>
      <label className="field">
        <span>Scorta minima</span>
        <input type="number" step="any" min="0" value={f.min_stock} onChange={(e) => set('min_stock', e.target.value)} />
      </label>
      <label className="field">
        <span>Tracciabilità</span>
        <select value={f.tracking} onChange={(e) => set('tracking', e.target.value as Tracking)} disabled={isEdit}>
          {options(trackingLabel).map(([value, label]) => (
            <option key={value} value={value}>
              {label}
            </option>
          ))}
        </select>
      </label>
      <label className="field checkbox">
        <input
          type="checkbox"
          checked={f.tracking !== 'NONE' && f.requires_expiry}
          disabled={isEdit || f.tracking === 'NONE'}
          onChange={(e) => set('requires_expiry', e.target.checked)}
        />
        <span>Richiede data di scadenza (solo prodotti a lotto o seriale)</span>
      </label>
      <label className="field">
        <span>Preavviso scadenza (giorni)</span>
        <input
          type="number"
          min="0"
          step="1"
          value={f.expiry_warning_days}
          onChange={(e) => set('expiry_warning_days', e.target.value)}
        />
      </label>
      <label className="field">
        <span>Peso per unità (kg)</span>
        <input type="number" step="any" min="0" value={f.unit_weight_kg} onChange={(e) => set('unit_weight_kg', e.target.value)} />
      </label>
      <label className="field">
        <span>Volume per unità (m³)</span>
        <input type="number" step="any" min="0" value={f.unit_volume_m3} onChange={(e) => set('unit_volume_m3', e.target.value)} />
      </label>
      <label className="field">
        <span>Stoccaggio richiesto</span>
        <select
          value={f.required_storage_type}
          onChange={(e) => set('required_storage_type', e.target.value as StorageType | '')}
        >
          <option value="">Qualsiasi</option>
          {options(storageTypeLabel).map(([value, label]) => (
            <option key={value} value={value}>
              {label}
            </option>
          ))}
        </select>
      </label>
      <div className="span-all">
        <ProblemAlert problem={inputProblem} />
      </div>
      <div className="actions span-all">
        {props.onCancel && (
          <button type="button" onClick={props.onCancel}>
            Annulla
          </button>
        )}
        <button type="submit" className="primary" disabled={props.busy}>
          {isEdit ? 'Salva modifiche' : 'Crea prodotto'}
        </button>
      </div>
    </form>
  )
}
