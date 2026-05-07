use std::{env, net::SocketAddr};

use reporting_service::{build_router, AppState};
use opentelemetry::global;
use opentelemetry_gcloud_trace::GcpCloudTraceExporterBuilder;
use tracing_subscriber::layer::SubscriberExt;

#[tokio::main]
// Initialises tracing with OpenTelemetry, reads environment config,
// sets up the database, and starts the HTTP server.
async fn main() {
    // Initialize OpenTelemetry tracer for Cloud Trace
    if let Ok(exporter) = GcpCloudTraceExporterBuilder::for_default_project_id().await {
        if let Ok(provider) = exporter.create_provider().await {
            if let Ok(_tracer) = exporter.install(&provider).await {
                global::set_tracer_provider(provider.clone());
                let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| "reporting_service=info,tower_http=info".into());
                let subscriber = tracing_subscriber::fmt()
                    .with_env_filter(env_filter)
                    .finish();
                tracing::subscriber::set_default(subscriber);
            } else {
                eprintln!("Failed to install OpenTelemetry tracer");
                init_basic_tracing("reporting_service");
            }
        } else {
            eprintln!("Failed to create OpenTelemetry provider");
            init_basic_tracing("reporting_service");
        }
    } else {
        eprintln!("Failed to initialize OpenTelemetry");
        init_basic_tracing("reporting_service");
    }

    let host = env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
    let port = env::var("PORT")
        .ok()
        .and_then(|v| v.parse::<u16>().ok())
        .unwrap_or(8080);
    let database_url =
        env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite://reporting.db".to_string());

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

    println!("reporting-service listening on http://{addr}");
    axum::serve(listener, app).await.expect("server failed");
}

fn init_basic_tracing(service_name: &str) {
    let subscriber = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| format!("{service_name}=info,tower_http=info").into()),
        )
        .finish();
    tracing::subscriber::set_default(subscriber);
}
