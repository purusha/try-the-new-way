import type { Alert, Movement } from '../api/types'
import { AlertsList } from './AlertsList'
import { MovementsTable } from './MovementsTable'

interface Props {
  title?: string
  result: { movements: Movement[]; alerts: Alert[] } | null | undefined
  emptyText?: string
}

/** Esito di un'operazione: movimenti generati e alert. */
export function OperationResultView({ title, result, emptyText }: Props) {
  if (!result) return null
  return (
    <section className="card result">
      <h2>{title ?? 'Operazione registrata'}</h2>
      <MovementsTable movements={result.movements} emptyText={emptyText ?? 'Nessun movimento generato.'} />
      {result.alerts.length > 0 && (
        <>
          <h3>Alert</h3>
          <AlertsList alerts={result.alerts} />
        </>
      )}
    </section>
  )
}
