import { type DependencyList, useCallback, useEffect, useState } from 'react'
import { asProblem, type Problem } from '../api/errors'

export interface AsyncState<T> {
  data: T | undefined
  error: Problem | undefined
  loading: boolean
  reload: () => void
  setData: (data: T) => void
}

/**
 * Esegue una lettura asincrona quando cambiano le dipendenze.
 * Con `enabled = false` non chiama nulla (es. magazzino non ancora scelto).
 */
export function useAsync<T>(fn: () => Promise<T>, deps: DependencyList, enabled = true): AsyncState<T> {
  const [data, setData] = useState<T>()
  const [error, setError] = useState<Problem>()
  const [loading, setLoading] = useState(enabled)
  const [nonce, setNonce] = useState(0)

  useEffect(() => {
    if (!enabled) {
      setLoading(false)
      return undefined
    }
    let cancelled = false
    setLoading(true)
    setError(undefined)
    fn().then(
      (result) => {
        if (cancelled) return
        setData(result)
        setLoading(false)
      },
      (err: unknown) => {
        if (cancelled) return
        setError(asProblem(err))
        setLoading(false)
      },
    )
    return () => {
      cancelled = true
    }
    // fn cambia a ogni render: contano solo le dipendenze dichiarate dal chiamante.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [...deps, enabled, nonce])

  const reload = useCallback(() => setNonce((n) => n + 1), [])
  return { data, error, loading, reload, setData }
}
