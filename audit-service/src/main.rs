use std::{env, net::SocketAddr};

use audit_service::{build_router, AppState};

#[tokio::main]
// Initialises tracing, reads environment config, sets up the database, and starts the HTTP server
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "audit_service=info,tower_http=info".into()),
        )
        .init();

    let host = env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
    let port = env::var("PORT")
        .ok()
        .and_then(|v| v.parse::<u16>().ok())
        .unwrap_or(8080);
    let database_url = env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/audit".to_string());
    let observaboard_ingest_url = env::var("OBSERVABOARD_INGEST_URL").ok();
    let observaboard_api_key = env::var("OBSERVABOARD_API_KEY").ok();

    let addr: SocketAddr = format!("{host}:{port}")
        .parse()
        .expect("invalid HOST/PORT combination");

    let state = AppState::new(&database_url, observaboard_ingest_url, observaboard_api_key)
        .await
        .expect("failed to initialise database");

    let app = build_router(state);
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("failed to bind TCP listener");

    println!("audit-service listening on http://{addr}");

    axum::serve(listener, app).await.expect("server failed");
}
