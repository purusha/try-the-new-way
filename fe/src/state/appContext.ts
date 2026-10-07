import { createContext, useContext } from 'react'
import type { Problem } from '../api/errors'
import type { Warehouse } from '../api/types'

export interface AppState {
  /** Magazzini noti (tutti, anche non attivi). */
  warehouses: Warehouse[]
  warehousesLoading: boolean
  warehousesError: Problem | undefined
  reloadWarehouses: () => void
  /** Magazzino selezionato nell'intestazione: default di tutte le pagine. */
  warehouseId: string
  warehouse: Warehouse | undefined
  setWarehouseId: (id: string) => void
  /** Nome dell'operatore, inviato come X-Operator. */
  operator: string
  setOperator: (name: string) => void
}

export const AppContext = createContext<AppState | null>(null)

export function useApp(): AppState {
  const value = useContext(AppContext)
  if (!value) throw new Error('useApp deve essere usato dentro AppProvider')
  return value
}
