import { BrowserRouter, Link, Navigate, Route, Routes } from 'react-router'
import { Layout } from './components/Layout'
import { AdjustmentPage } from './pages/AdjustmentPage'
import { LotTracePage } from './pages/LotTracePage'
import { MovementsPage } from './pages/MovementsPage'
import { ProductDetailPage } from './pages/ProductDetailPage'
import { ProductsPage } from './pages/ProductsPage'
import { ReceiptPage } from './pages/ReceiptPage'
import { SalesOrderDetailPage } from './pages/SalesOrderDetailPage'
import { SalesOrdersPage } from './pages/SalesOrdersPage'
import { SerialTracePage } from './pages/SerialTracePage'
import { ShipmentPage } from './pages/ShipmentPage'
import { StockPage } from './pages/StockPage'
import { TracePage } from './pages/TracePage'
import { TransferPage } from './pages/TransferPage'
import { WarehousePage } from './pages/WarehousePage'
import { AppProvider } from './state/AppProvider'

function NotFound() {
  return (
    <section className="card">
      <h1>Pagina non trovata</h1>
      <Link to="/products">Torna ai prodotti</Link>
    </section>
  )
}

export default function App() {
  return (
    <AppProvider>
      <BrowserRouter>
        <Routes>
          <Route element={<Layout />}>
            <Route index element={<Navigate to="/products" replace />} />
            <Route path="products" element={<ProductsPage />} />
            <Route path="products/:id" element={<ProductDetailPage />} />
            <Route path="warehouse" element={<WarehousePage />} />
            <Route path="stock" element={<StockPage />} />
            <Route path="receipt" element={<ReceiptPage />} />
            <Route path="shipment" element={<ShipmentPage />} />
            <Route path="transfer" element={<TransferPage />} />
            <Route path="adjustment" element={<AdjustmentPage />} />
            <Route path="sales-orders" element={<SalesOrdersPage />} />
            <Route path="sales-orders/:id" element={<SalesOrderDetailPage />} />
            <Route path="movements" element={<MovementsPage />} />
            <Route path="trace" element={<TracePage />} />
            <Route path="lots/:id" element={<LotTracePage />} />
            <Route path="serials/:id" element={<SerialTracePage />} />
            <Route path="*" element={<NotFound />} />
          </Route>
        </Routes>
      </BrowserRouter>
    </AppProvider>
  )
}
