import { api } from '../api/client'
import { unwrap } from '../api/errors'
import type { Location } from '../api/types'
import { useAsync } from './useAsync'

/**
 * Ubicazioni del magazzino per i menu a tendina (massimo 200, il limite di page_size).
 */
export function useLocations(warehouseId: string, onlyActive = true) {
  const state = useAsync(
    () =>
      unwrap(
        api.GET('/warehouses/{warehouseId}/locations', {
          params: {
            path: { warehouseId },
            query: { page_size: 200, active: onlyActive ? true : undefined },
          },
        }),
      ),
    [warehouseId, onlyActive],
    Boolean(warehouseId),
  )
  const locations: Location[] = state.data?.items ?? []
  return { ...state, locations }
}
