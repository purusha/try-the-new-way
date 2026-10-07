import { Link, useParams } from 'react-router'
import { api } from '../api/client'
import { unwrap } from '../api/errors'
import { Badge, Loading, PageHeader } from '../components/common'
import { MovementsTable } from '../components/MovementsTable'
import { ProblemAlert } from '../components/ProblemAlert'
import { ShipmentsTable } from '../components/ShipmentsTable'
import { useAsync } from '../hooks/useAsync'
import { fmtDateTime } from '../lib/format'
import { serialStatusLabel } from '../lib/labels'

export function SerialTracePage() {
  const { id = '' } = useParams()
  const trace = useAsync(
    () => unwrap(api.GET('/serials/{serialId}/trace', { params: { path: { serialId: id } } })),
    [id],
  )

  if (trace.loading && !trace.data) return <Loading />
  if (trace.error && !trace.data) return <ProblemAlert problem={trace.error} />
  if (!trace.data) return null
  const { serial, movements, shipment } = trace.data

  return (
    <>
      <PageHeader title={`Seriale ${serial.serial_number}`}>
        <Link to="/trace">← Tracciabilità</Link>
      </PageHeader>
      <section className="card">
        <h2>
          <Link to={`/products/${serial.product_id}`}>{serial.sku}</Link>{' '}
          <Badge tone={serial.status === 'IN_STOCK' ? 'ok' : 'neutral'}>{serialStatusLabel[serial.status]}</Badge>
        </h2>
        <dl className="props">
          <div>
            <dt>Lotto</dt>
            <dd>{serial.lot_id ? <Link to={`/lots/${serial.lot_id}`}>{serial.lot_code}</Link> : '—'}</dd>
          </div>
          <div>
            <dt>Ubicazione attuale</dt>
            <dd>{serial.location_code ?? '—'}</dd>
          </div>
          <div>
            <dt>Registrato</dt>
            <dd>{fmtDateTime(serial.created_at)}</dd>
          </div>
        </dl>
      </section>
      <section className="card">
        <h2>Spedizione al cliente</h2>
        <ShipmentsTable shipments={shipment ? [shipment] : []} />
      </section>
      <section className="card">
        <h2>Movimenti</h2>
        <MovementsTable movements={movements} />
      </section>
    </>
  )
}
