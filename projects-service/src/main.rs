use std::{env, net::SocketAddr};

use projects_service::{build_router, AppState};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "projects_service=info,tower_http=info".into()),
        )
        .init();

    let host = env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
    let port = env::var("PORT")
        .ok()
        .and_then(|v| v.parse::<u16>().ok())
        .unwrap_or(3014);
    let database_url =
        env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite://projects.db".to_string());

    let addr: SocketAddr = format!("{host}:{port}")
        .parse()
        .expect("invalid HOST/PORT combination");

    let state = match AppState::from_database_url(&database_url).await {
        Ok(state) => state,
        Err(err) => {
            eprintln!(
                "failed to initialise database at {database_url}: {err}; falling back to in-memory sqlite"
            );
            AppState::from_database_url("sqlite::memory:")
                .await
                .expect("failed to initialise fallback in-memory database")
        }
    };

    let app = build_router(state);

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("failed to bind TCP listener");

    println!("projects-service listening on http://{addr}");
    axum::serve(listener, app).await.expect("server failed");
}
