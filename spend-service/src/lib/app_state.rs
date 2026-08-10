use sqlx::{postgres::PgPoolOptions, PgPool};

#[derive(Clone)]
pub struct AppState {
    pub(crate) pool: PgPool,
    pub(crate) http_client: reqwest::Client,
}

/// Builds the outbound HTTP client the billing-sync paths share.
fn build_http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent(concat!(
            env!("CARGO_PKG_NAME"),
            "/",
            env!("CARGO_PKG_VERSION")
        ))
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .expect("failed to build HTTP client")
}

impl AppState {
    pub async fn from_database_url(database_url: &str) -> Result<Self, sqlx::Error> {
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(database_url)
            .await?;

        sqlx::migrate!("./migrations").run(&pool).await?;

        Ok(Self::from_pool(pool))
    }

    /// Builds state around a pool the caller already owns.
    ///
    /// Production goes through [`AppState::from_database_url`], which connects
    /// eagerly and runs migrations before calling this. The seam exists so a
    /// caller can supply a pool that has not connected: `tests/role_gating.rs`
    /// passes a lazily-created pool, which lets the router-level authorization
    /// gate — which rejects before any query runs — be exercised for real
    /// without a database.
    pub fn from_pool(pool: PgPool) -> Self {
        Self {
            pool,
            http_client: build_http_client(),
        }
    }
}
