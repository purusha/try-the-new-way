use axum::Router;

#[tokio::main]
async fn main() {
    // Le rotte API verranno aggiunte sotto /api con le specifiche funzionali.
    let api = Router::new();
    let app = Router::new().nest("/api", api);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000")
        .await
        .expect("impossibile aprire la porta 3000");
    println!("BE in ascolto su http://127.0.0.1:3000");
    axum::serve(listener, app).await.expect("errore del server");
}
