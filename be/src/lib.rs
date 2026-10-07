//! Backend dell'API di magazzino (spec 007).

use axum::Router;
use axum::middleware;
use sqlx::PgPool;
use tower_http::trace::TraceLayer;

pub mod domain;
pub mod error;
pub mod http;
pub mod idempotency;
pub mod inventory;
pub mod model;
pub mod pagination;
pub mod routes;

pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

/// Applicazione completa: API sotto `/api/v1`, con idempotenza e tracing delle richieste.
pub fn app(pool: PgPool) -> Router {
    let api = routes::router()
        .layer(middleware::from_fn_with_state(pool.clone(), idempotency::layer))
        .with_state(pool);
    Router::new().nest("/api/v1", api).layer(TraceLayer::new_for_http())
}
