use sqlx::{postgres::PgPoolOptions, PgPool};

use crate::peer_total::build_peer_client;

#[derive(Clone)]
pub struct AppState {
    pub(crate) pool: PgPool,
    /// Shared client for the dashboard's sibling-service rollup.
    ///
    /// One per process rather than one per call: `fetch_service_total` built a
    /// fresh `reqwest::Client::new()` on every fan-out (four per dashboard
    /// request), which threw away connection pooling AND carried no timeout,
    /// because `reqwest` sets none by default. `build_peer_client` pins
    /// `peer_total::PEER_TIMEOUT`, matching what the CRM services already do.
    pub(crate) http_client: reqwest::Client,
}

impl AppState {
    pub async fn from_database_url(database_url: &str) -> Result<Self, sqlx::Error> {
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(database_url)
            .await?;

        sqlx::migrate!("./migrations").run(&pool).await?;

        Ok(Self {
            pool,
            http_client: build_peer_client(),
        })
    }

    /// Runs migrations against the primary (`write_url`), then opens the serving
    /// pool against `read_url` if provided (a read replica), or `write_url` as
    /// fallback. Reporting-service is read-only, so all queries hit the replica.
    pub async fn with_read_replica(
        write_url: &str,
        read_url: Option<&str>,
    ) -> Result<Self, sqlx::Error> {
        // Always migrate against the primary to avoid replication-lag races.
        let migrate_pool = PgPoolOptions::new()
            .max_connections(1)
            .connect(write_url)
            .await?;
        sqlx::migrate!("./migrations").run(&migrate_pool).await?;
        drop(migrate_pool);

        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(read_url.unwrap_or(write_url))
            .await?;

        Ok(Self {
            pool,
            http_client: build_peer_client(),
        })
    }
}
