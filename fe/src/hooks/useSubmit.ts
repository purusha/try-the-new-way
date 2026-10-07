import { useCallback, useState } from 'react'
import { asProblem, type Problem } from '../api/errors'

/** Stato di un invio (busy + errore) con cattura delle eccezioni in un Problem. */
export function useSubmit() {
  const [busy, setBusy] = useState(false)
  const [problem, setProblem] = useState<Problem | null>(null)

  const run = useCallback(async <T>(action: () => Promise<T>): Promise<T | undefined> => {
    setBusy(true)
    setProblem(null)
    try {
      return await action()
    } catch (error) {
      setProblem(asProblem(error))
      return undefined
    } finally {
      setBusy(false)
    }
  }, [])

  return { run, busy, problem, setProblem }
}
