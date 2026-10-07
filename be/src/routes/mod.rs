//! Router dell'API `/api/v1` (contratto in `api/openapi.yaml`).

use axum::Router;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, patch, post};
use sqlx::PgPool;

use crate::error::ApiError;

pub mod lots;
pub mod movements;
pub mod operations;
pub mod products;
pub mod purchase_orders;
pub mod sales_orders;
pub mod stock;
pub mod warehouses;

pub fn router() -> Router<PgPool> {
    Router::new()
        .route(
            "/warehouses",
            get(warehouses::list_warehouses).post(warehouses::create_warehouse),
        )
        .route(
            "/warehouses/{id}",
            get(warehouses::get_warehouse).patch(warehouses::update_warehouse),
        )
        .route(
            "/warehouses/{id}/zones",
            get(warehouses::list_zones).post(warehouses::create_zone),
        )
        .route("/zones/{id}", patch(warehouses::update_zone))
        .route(
            "/warehouses/{id}/locations",
            get(warehouses::list_locations).post(warehouses::create_location),
        )
        .route(
            "/locations/{id}",
            get(warehouses::get_location).patch(warehouses::update_location),
        )
        .route("/locations/{id}/stock", get(warehouses::location_stock))
        .route("/products", get(products::list_products).post(products::create_product))
        .route(
            "/products/{id}",
            get(products::get_product).patch(products::update_product),
        )
        .route("/products/{id}/archive", post(products::archive_product))
        .route("/products/{id}/reactivate", post(products::reactivate_product))
        .route("/products/{id}/availability", get(products::product_availability))
        .route(
            "/products/{id}/lots",
            get(lots::list_product_lots).post(lots::create_lot),
        )
        .route("/lots", get(lots::search_lots))
        .route("/lots/{id}", get(lots::get_lot).patch(lots::update_lot))
        .route("/lots/{id}/trace", get(lots::trace_lot))
        .route("/serials", get(lots::search_serials))
        .route("/serials/{id}/trace", get(lots::trace_serial))
        .route("/stock", get(stock::list_stock))
        .route("/alerts", get(stock::list_alerts))
        .route("/operations/receipts", post(operations::receipt))
        .route("/operations/picking-suggestions", post(operations::suggest))
        .route("/operations/shipments", post(operations::shipment))
        .route("/operations/transfers", post(operations::transfer))
        .route("/operations/adjustments", post(operations::adjustment))
        .route("/movements", get(movements::list))
        .route(
            "/movements/{id}",
            get(movements::get)
                .put(movements::immutable)
                .patch(movements::immutable)
                .delete(movements::immutable),
        )
        .route("/movements/{id}/reversal", post(movements::reverse))
        .route("/sales-orders", get(sales_orders::list).post(sales_orders::create))
        .route("/sales-orders/{id}", get(sales_orders::get))
        .route("/sales-orders/{id}/reserve", post(sales_orders::reserve))
        .route("/sales-orders/{id}/release", post(sales_orders::release))
        .route("/sales-orders/{id}/lines/{line_id}", patch(sales_orders::update_line))
        .route("/sales-orders/{id}/cancel", post(sales_orders::cancel))
        .route("/sales-orders/{id}/fulfill", post(sales_orders::fulfill))
        .route(
            "/purchase-orders",
            get(purchase_orders::list).post(purchase_orders::create),
        )
        .route("/purchase-orders/{id}", get(purchase_orders::get))
        .route("/purchase-orders/{id}/cancel", post(purchase_orders::cancel))
        .method_not_allowed_fallback(|| async {
            ApiError::new(
                StatusCode::METHOD_NOT_ALLOWED,
                "METHOD_NOT_ALLOWED",
                "Metodo non ammesso su questa risorsa",
            )
            .into_response()
        })
        .fallback(|| async {
            ApiError::new(StatusCode::NOT_FOUND, "NOT_FOUND", "Endpoint inesistente").into_response()
        })
}
