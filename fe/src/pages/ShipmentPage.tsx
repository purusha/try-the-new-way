import { useState } from 'react'
import { api, newIdempotencyKey, writeHeaders } from '../api/client'
import { clientProblem, earlierLotsOf, unwrap } from '../api/errors'
import type {
  OperationResult,
  PickingSuggestion,
  PickStrategy,
  ProductListItem,
  ShipmentLine,
  ShipmentRequest as ShipmentBody,
} from '../api/types'
import { Empty, OperatorNotice, PageHeader, RequireWarehouse } from '../components/common'
import { OperationResultView } from '../components/OperationResultView'
import { EarlierLots, ProblemAlert } from '../components/ProblemAlert'
import { ProductPicker } from '../components/ProductPicker'
import { ReasonDialog } from '../components/ReasonDialog'
import { StockRowSelect } from '../components/selects'
import { PickingSuggestionView } from '../components/PickingSuggestionView'
import { useProductStock } from '../hooks/useProductStock'
import { useSubmit } from '../hooks/useSubmit'
import { stockRowKey } from '../lib/describe'
import { orNull, parseNumber, parseSerials } from '../lib/format'
import { options, strategyLabel } from '../lib/labels'
import { useApp } from '../state/appContext'

interface Line {
  key: string
  product: ProductListItem | null
  quantity: string
}

interface ManualPick {
  key: string
  /** location_id|lot_id della riga di giacenza scelta */
  stockKey: string
  quantity: string
  serials: string
}


let seq = 0
const nextKey = () => `k${++seq}`
const emptyLine = (): Line => ({ key: nextKey(), product: null, quantity: '' })
const emptyPick = (): ManualPick => ({ key: nextKey(), stockKey: '', quantity: '', serials: '' })

interface ManualPicksProps {
  warehouseId: string
  product: ProductListItem
  picks: ManualPick[]
  onChange: (picks: ManualPick[]) => void
}

function ManualPicksEditor({ warehouseId, product, picks, onChange }: ManualPicksProps) {
  const stock = useProductStock(warehouseId, product.id)
  const rows = stock.data?.items ?? []
  const update = (key: string, patch: Partial<ManualPick>) =>
    onChange(picks.map((p) => (p.key === key ? { ...p, ...patch } : p)))

  return (
    <div className="picks">
      {picks.map((pick) => {
        const row = rows.find((r) => stockRowKey(r) === pick.stockKey)
        return (
          <div key={pick.key} className="pick-row">
            <label className="field">
              <span>Da (ubicazione · lotto)</span>
              <StockRowSelect
                rows={rows}
                loading={stock.loading}
                value={pick.stockKey}
                onChange={(stockKey) => update(pick.key, { stockKey })}
              />
            </label>
            <label className="field">
              <span>Quantità</span>
              <input
                type="number"
                step="any"
                min="0"
                value={pick.quantity}
                onChange={(e) => update(pick.key, { quantity: e.target.value })}
              />
            </label>
            {product.tracking === 'SERIAL' && (
              <label className="field">
                <span>Seriali (uno per riga)</span>
                <textarea
                  rows={2}
                  value={pick.serials}
                  placeholder={row?.serials?.slice(0, 3).join('\n')}
                  onChange={(e) => update(pick.key, { serials: e.target.value })}
                />
              </label>
            )}
            <button type="button" className="link" onClick={() => onChange(picks.filter((p) => p.key !== pick.key))}>
              Rimuovi
            </button>
          </div>
        )
      })}
      <button type="button" className="small" onClick={() => onChange([...picks, emptyPick()])}>
        + Aggiungi prelievo
      </button>
    </div>
  )
}

function ShipmentForm({ warehouseId }: { warehouseId: string }) {
  const [lines, setLines] = useState<Line[]>(() => [emptyLine()])
  const [strategy, setStrategy] = useState<PickStrategy>('AUTO')
  const [externalRef, setExternalRef] = useState('')
  const [note, setNote] = useState('')
  const [suggestion, setSuggestion] = useState<PickingSuggestion | null>(null)
  const [manual, setManual] = useState(false)
  const [manualPicks, setManualPicks] = useState<Record<string, ManualPick[]>>({})
  const [result, setResult] = useState<OperationResult | null>(null)
  const [pendingBody, setPendingBody] = useState<ShipmentBody | null>(null)
  const suggest = useSubmit()
  const ship = useSubmit()

  const resetPlan = () => {
    setSuggestion(null)
    setManual(false)
    setManualPicks({})
  }

  const updateLine = (key: string, patch: Partial<Line>) => {
    setLines((ls) => ls.map((l) => (l.key === key ? { ...l, ...patch } : l)))
    resetPlan()
  }

  const requestLines = () =>
    lines.map((l, i) => {
      if (!l.product) throw clientProblem(`Riga ${i + 1}: scegli il prodotto.`)
      const quantity = parseNumber(l.quantity)
      if (!(quantity > 0)) throw clientProblem(`Riga ${i + 1}: la quantità deve essere maggiore di zero.`)
      return { product_id: l.product.id, quantity }
    })

  const askSuggestion = async () => {
    setResult(null)
    const data = await suggest.run(() =>
      unwrap(
        api.POST('/operations/picking-suggestions', {
          body: { warehouse_id: warehouseId, strategy, lines: requestLines() },
        }),
      ),
    )
    if (data) {
      setSuggestion(data)
      setManual(false)
    }
  }

  const switchToManual = () => {
    const prefill: Record<string, ManualPick[]> = {}
    lines.forEach((line, i) => {
      const suggested = suggestion?.lines[i]
      prefill[line.key] =
        suggested && suggested.product_id === line.product?.id && suggested.picks.length > 0
          ? suggested.picks.map((p) => ({
              key: nextKey(),
              stockKey: stockRowKey(p),
              quantity: String(p.quantity),
              serials: (p.serials ?? []).join('\n'),
            }))
          : [emptyPick()]
    })
    setManualPicks(prefill)
    setManual(true)
  }

  const manualLines = (): ShipmentLine[] =>
    lines.map((line, i) => {
      if (!line.product) throw clientProblem(`Riga ${i + 1}: scegli il prodotto.`)
      const picks = (manualPicks[line.key] ?? []).map((p, j) => {
        const quantity = parseNumber(p.quantity)
        if (!p.stockKey) throw clientProblem(`Riga ${i + 1}, prelievo ${j + 1}: scegli ubicazione e lotto.`)
        if (!(quantity > 0)) throw clientProblem(`Riga ${i + 1}, prelievo ${j + 1}: quantità non valida.`)
        const [locationId, lotId] = p.stockKey.split('|')
        return {
          location_id: locationId,
          lot_id: lotId || null,
          quantity,
          serials: line.product?.tracking === 'SERIAL' ? parseSerials(p.serials) : undefined,
        }
      })
      if (picks.length === 0) throw clientProblem(`Riga ${i + 1}: indica almeno un prelievo.`)
      return { product_id: line.product.id, picks }
    })

  /** Invia lo scarico; il corpo è costruito dentro run così gli errori di validazione finiscono nel ProblemAlert. */
  const send = async (build: () => ShipmentBody) => {
    setResult(null)
    const sent: { body?: ShipmentBody } = {}
    const done = await ship.run(() => {
      sent.body = build()
      // Nuova chiave a ogni invio: il reinvio con fefo_override ha un corpo diverso.
      return unwrap(
        api.POST('/operations/shipments', { params: { header: writeHeaders(newIdempotencyKey()) }, body: sent.body }),
      )
    })
    setPendingBody(done ? null : (sent.body ?? null))
    if (done) {
      setResult(done)
      setLines([emptyLine()])
      resetPlan()
    }
  }

  const shipAuto = () =>
    send(() => ({
      warehouse_id: warehouseId,
      mode: 'AUTO',
      strategy,
      external_ref: orNull(externalRef),
      note: orNull(note),
      lines: requestLines(),
    }))

  const shipManual = () =>
    send(() => ({
      warehouse_id: warehouseId,
      mode: 'MANUAL',
      external_ref: orNull(externalRef),
      note: orNull(note),
      lines: manualLines(),
    }))

  const fefoProblem = ship.problem?.code === 'FEFO_VIOLATION' && pendingBody ? ship.problem : null

  return (
    <>
      <section className="card">
        <OperatorNotice />
        {lines.map((line, i) => (
          <div key={line.key} className="line-row">
            <div className="field grow">
              <span>Prodotto {i + 1}</span>
              <ProductPicker
                value={line.product}
                warehouseId={warehouseId}
                onChange={(product) => updateLine(line.key, { product })}
              />
            </div>
            <label className="field">
              <span>Quantità{line.product ? ` (${line.product.uom})` : ''}</span>
              <input
                type="number"
                step="any"
                min="0"
                value={line.quantity}
                onChange={(e) => updateLine(line.key, { quantity: e.target.value })}
              />
            </label>
            {lines.length > 1 && (
              <button
                type="button"
                className="link"
                onClick={() => {
                  setLines((ls) => ls.filter((l) => l.key !== line.key))
                  resetPlan()
                }}
              >
                Rimuovi
              </button>
            )}
          </div>
        ))}
        <div className="form-grid">
          <label className="field span-2">
            <span>Strategia</span>
            <select
              value={strategy}
              onChange={(e) => {
                setStrategy(e.target.value as PickStrategy)
                resetPlan()
              }}
            >
              {options(strategyLabel).map(([value, label]) => (
                <option key={value} value={value}>
                  {label}
                </option>
              ))}
            </select>
          </label>
          <label className="field">
            <span>Riferimento esterno</span>
            <input value={externalRef} onChange={(e) => setExternalRef(e.target.value)} />
          </label>
          <label className="field">
            <span>Note</span>
            <input value={note} onChange={(e) => setNote(e.target.value)} />
          </label>
        </div>
        <div className="actions">
          <button
            type="button"
            onClick={() => {
              setLines((ls) => [...ls, emptyLine()])
              resetPlan()
            }}
          >
            + Aggiungi riga
          </button>
          <button type="button" onClick={askSuggestion} disabled={suggest.busy}>
            Suggerisci prelievo
          </button>
          <button type="button" className="primary" onClick={shipAuto} disabled={ship.busy}>
            Scarica (automatico)
          </button>
        </div>
        <ProblemAlert problem={suggest.problem} />
        {!fefoProblem && <ProblemAlert problem={ship.problem} />}
      </section>

      {suggestion && !manual && (
        <section className="card">
          <div className="card-head">
            <h2>Prelievo suggerito</h2>
            <button type="button" onClick={switchToManual}>
              Modifica i prelievi (manuale)
            </button>
          </div>
          <PickingSuggestionView suggestion={suggestion} />
        </section>
      )}

      {manual && (
        <section className="card">
          <div className="card-head">
            <h2>Prelievo manuale</h2>
            <button type="button" onClick={() => setManual(false)}>
              Torna al suggerimento
            </button>
          </div>
          {lines.map((line, i) =>
            line.product ? (
              <fieldset key={line.key} className="line">
                <legend>
                  Riga {i + 1}: {line.product.sku} — richiesti {line.quantity || '—'} {line.product.uom}
                </legend>
                <ManualPicksEditor
                  warehouseId={warehouseId}
                  product={line.product}
                  picks={manualPicks[line.key] ?? []}
                  onChange={(picks) => setManualPicks((m) => ({ ...m, [line.key]: picks }))}
                />
              </fieldset>
            ) : (
              <Empty key={line.key}>Riga {i + 1}: nessun prodotto.</Empty>
            ),
          )}
          <div className="actions">
            <button type="button" className="primary" onClick={shipManual} disabled={ship.busy}>
              Scarica (manuale)
            </button>
          </div>
        </section>
      )}

      <ReasonDialog
        open={fefoProblem !== null}
        title="Deroga FEFO"
        label="Motivo della deroga"
        confirmLabel="Conferma e scarica"
        busy={ship.busy}
        onCancel={() => {
          setPendingBody(null)
          ship.setProblem(null)
        }}
        onConfirm={(reason) => {
          const body = pendingBody
          if (body) void send(() => ({ ...body, fefo_override: { reason } }))
        }}
      >
        <p>
          I prelievi scelti saltano lotti con scadenza anteriore. Per procedere comunque indica il motivo: verrà
          registrato sui movimenti.
        </p>
        {fefoProblem?.detail && <p className="muted">{fefoProblem.detail}</p>}
        {fefoProblem && <EarlierLots lots={earlierLotsOf(fefoProblem)} />}
      </ReasonDialog>

      <OperationResultView title="Scarico registrato" result={result} />
    </>
  )
}

export function ShipmentPage() {
  const { warehouseId } = useApp()
  return (
    <>
      <PageHeader title="Prelievo e scarico" />
      <p className="muted">
        Uscita merci senza ordine: si può scaricare solo la quantità non impegnata. Per la merce impegnata usa
        l'evasione dell'ordine cliente.
      </p>
      <RequireWarehouse>
        <ShipmentForm key={warehouseId} warehouseId={warehouseId} />
      </RequireWarehouse>
    </>
  )
}
