use std::env;

use sqlx::postgres::PgPoolOptions;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() {
    // Configurazione da variabili d'ambiente o da be/.env (vedi .env.example).
    let _ = dotenvy::dotenv();
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("be=info,tower_http=info")),
        )
        .init();

    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL non impostata (vedi .env.example)");
    let bind_addr = env::var("BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:3000".into());

    let pool = PgPoolOptions::new()
        .max_connections(20)
        .connect(&database_url)
        .await
        .expect("connessione al database fallita");
    be::MIGRATOR.run(&pool).await.expect("migrazioni del database fallite");

    let listener = tokio::net::TcpListener::bind(&bind_addr)
        .await
        .unwrap_or_else(|e| panic!("impossibile aprire {bind_addr}: {e}"));
    tracing::info!("BE in ascolto su http://{bind_addr}/api/v1");
    axum::serve(listener, be::app(pool)).await.expect("errore del server");
}
