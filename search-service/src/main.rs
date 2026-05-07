use std::{env, net::SocketAddr};

use search_service::{build_router, AppState};
use tracing_subscriber::layer::SubscriberExt;

#[tokio::main]
// Initialises tracing with OpenTelemetry, reads environment config,
// sets up the database, and starts the HTTP server.
async fn main() {
    // Initialize OpenTelemetry tracer for Cloud Trace
    let tracer = match opentelemetry_gcp::CloudTraceExporter::new()
        .install_batch(opentelemetry::runtime::Tokio)
    {
        Ok(t) => Some(t),
        Err(e) => {
            eprintln!("Failed to initialize OpenTelemetry: {}", e);
            None
        }
    };

    let tracing_layer = if let Some(t) = tracer {
        Some(tracing_opentelemetry::layer().with_tracer(t))
    } else {
        None
    };

    let subscriber = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "search_service=info,tower_http=info".into()),
        );

    if let Some(layer) = tracing_layer {
        tracing::subscriber::set_default(subscriber.with(layer).finish());
    } else {
        tracing::subscriber::set_default(subscriber.finish());
    }

    let host = env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
    let port = env::var("PORT")
        .ok()
        .and_then(|v| v.parse::<u16>().ok())
        .unwrap_or(8080);
    let database_url =
        env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite://search.db".to_string());

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

    println!("search-service listening on http://{addr}");
    axum::serve(listener, app).await.expect("server failed");
}
