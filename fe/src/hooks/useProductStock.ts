import { api } from '../api/client'
import { unwrap } from '../api/errors'
import { useAsync } from './useAsync'

/** Righe di giacenza di un prodotto nel magazzino. */
export function useProductStock(warehouseId: string, productId: string) {
  return useAsync(
    () =>
      unwrap(
        api.GET('/stock', {
          params: { query: { warehouse_id: warehouseId, product_id: productId, page_size: 200 } },
        }),
      ),
    [warehouseId, productId],
    Boolean(warehouseId && productId),
  )
}
