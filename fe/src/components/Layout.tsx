import { useState } from 'react'
import { NavLink, Outlet } from 'react-router'
import { useApp } from '../state/appContext'

const links: [string, string][] = [
  ['/products', 'Prodotti'],
  ['/stock', 'Giacenze'],
  ['/receipt', 'Carico'],
  ['/shipment', 'Prelievo / Scarico'],
  ['/transfer', 'Trasferimento'],
  ['/adjustment', 'Rettifica'],
  ['/sales-orders', 'Ordini cliente'],
  ['/movements', 'Movimenti'],
  ['/trace', 'Tracciabilità'],
  ['/warehouse', 'Magazzino'],
]

export function Layout() {
  const { warehouses, warehouseId, setWarehouseId, warehousesError, operator, setOperator } = useApp()
  const [operatorText, setOperatorText] = useState(operator)

  return (
    <div className="app">
      <header className="topbar">
        <div className="brand">Magazzino</div>
        <div className="topbar-controls">
          <label>
            <span>Magazzino</span>
            <select value={warehouseId} onChange={(e) => setWarehouseId(e.target.value)}>
              {warehouses.length === 0 && <option value="">{warehousesError ? 'Errore' : 'Nessuno'}</option>}
              {warehouses.map((w) => (
                <option key={w.id} value={w.id}>
                  {w.code} · {w.name}
                  {w.active ? '' : ' (non attivo)'}
                </option>
              ))}
            </select>
          </label>
          <label className={operator ? '' : 'missing'}>
            <span>Operatore</span>
            <input
              value={operatorText}
              placeholder="Nome operatore"
              maxLength={100}
              onChange={(e) => {
                setOperatorText(e.target.value)
                setOperator(e.target.value)
              }}
            />
          </label>
        </div>
        <nav className="nav">
          {links.map(([to, label]) => (
            <NavLink key={to} to={to}>
              {label}
            </NavLink>
          ))}
        </nav>
      </header>
      <main className="content">
        <Outlet />
      </main>
    </div>
  )
}
