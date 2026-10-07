import { api, newIdempotencyKey, writeHeaders } from '../api/client'
import { unwrap } from '../api/errors'
import type { Movement, OperationResult } from '../api/types'
import { useSubmit } from '../hooks/useSubmit'
import { fmtDateTime, fmtQty } from '../lib/format'
import { movementTypeLabel } from '../lib/labels'
import { ProblemAlert } from './ProblemAlert'
import { ReasonDialog } from './ReasonDialog'

interface Props {
  movement: Movement | null
  onClose: () => void
  onDone: (result: OperationResult) => void
}

/** Chiede la motivazione e registra lo storno (POST /movements/{id}/reversal). */
export function ReverseMovementDialog({ movement, onClose, onDone }: Props) {
  const { run, busy, problem, setProblem } = useSubmit()

  const confirm = async (reason: string) => {
    if (!movement) return
    const result = await run(() =>
      unwrap(
        api.POST('/movements/{movementId}/reversal', {
          params: { path: { movementId: movement.id }, header: writeHeaders(newIdempotencyKey()) },
          body: { reason },
        }),
      ),
    )
    if (result) onDone(result)
  }

  return (
    <ReasonDialog
      open={movement !== null}
      title="Storno del movimento"
      confirmLabel="Registra storno"
      busy={busy}
      onConfirm={confirm}
      onCancel={() => {
        setProblem(null)
        onClose()
      }}
    >
      {movement && (
        <p>
          Movimento n° {movement.number} — {movementTypeLabel[movement.type]} di {fmtQty(movement.quantity)}{' '}
          {movement.sku} del {fmtDateTime(movement.occurred_at)}. Verrà creato il movimento inverso collegato;
          l'operazione non è ripetibile.
        </p>
      )}
      <ProblemAlert problem={problem} />
    </ReasonDialog>
  )
}
