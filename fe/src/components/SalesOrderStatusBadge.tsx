import type { SalesOrderStatus } from '../api/types'
import { salesOrderStatusLabel } from '../lib/labels'
import { Badge } from './common'

const tone: Record<SalesOrderStatus, 'neutral' | 'ok' | 'warn' | 'info' | 'danger'> = {
  OPEN: 'neutral',
  PARTIALLY_RESERVED: 'warn',
  RESERVED: 'info',
  PARTIALLY_FULFILLED: 'warn',
  FULFILLED: 'ok',
  CANCELLED: 'danger',
}

export function SalesOrderStatusBadge({ status }: { status: SalesOrderStatus }) {
  return <Badge tone={tone[status]}>{salesOrderStatusLabel[status]}</Badge>
}
