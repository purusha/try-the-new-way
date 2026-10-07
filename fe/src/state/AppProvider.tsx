import { type ReactNode, useMemo, useState } from 'react'
import { api, getOperator, setOperator as storeOperator } from '../api/client'
import { unwrap } from '../api/errors'
import { useAsync } from '../hooks/useAsync'
import { readStored, writeStored } from '../lib/storage'
import { AppContext, type AppState } from './appContext'

const WAREHOUSE_KEY = 'warehouse_id'

export function AppProvider({ children }: { children: ReactNode }) {
  const [storedWarehouseId, setStoredWarehouseId] = useState(() => readStored(WAREHOUSE_KEY))
  const [operator, setOperatorState] = useState(() => getOperator())

  const warehousesState = useAsync(
    () => unwrap(api.GET('/warehouses', { params: { query: { page_size: 200 } } })),
    [],
  )
  const warehouses = useMemo(() => warehousesState.data?.items ?? [], [warehousesState.data])

  // Se il magazzino salvato non esiste più (o non è mai stato scelto) si usa il primo attivo.
  const warehouse =
    warehouses.find((w) => w.id === storedWarehouseId) ?? warehouses.find((w) => w.active) ?? warehouses[0]
  const warehouseId = warehousesState.data ? (warehouse?.id ?? '') : storedWarehouseId

  const value: AppState = {
    warehouses,
    warehousesLoading: warehousesState.loading,
    warehousesError: warehousesState.error,
    reloadWarehouses: warehousesState.reload,
    warehouseId,
    warehouse,
    setWarehouseId: (id) => {
      writeStored(WAREHOUSE_KEY, id)
      setStoredWarehouseId(id)
    },
    operator,
    setOperator: (name) => {
      storeOperator(name)
      setOperatorState(name.trim())
    },
  }

  return <AppContext.Provider value={value}>{children}</AppContext.Provider>
}
