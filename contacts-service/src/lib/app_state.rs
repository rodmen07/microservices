use std::str::FromStr;

use sqlx::{
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
    SqlitePool,
};

#[derive(Clone)]
pub struct AppState {
    pub(crate) pool: SqlitePool,
    /// HTTP client for cross-service calls (e.g. account validation).
    pub(crate) http_client: reqwest::Client,
}

impl AppState {
    // Creates a SQLite connection pool, runs migrations, and builds an HTTP client for cross-service calls
    pub async fn from_database_url(database_url: &str) -> Result<Self, sqlx::Error> {
        let opts = SqliteConnectOptions::from_str(database_url)?.create_if_missing(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(opts)
            .await?;

        sqlx::migrate!("./migrations").run(&pool).await?;

        let http_client = reqwest::Client::builder()
            .user_agent(concat!(
                env!("CARGO_PKG_NAME"),
                "/",
                env!("CARGO_PKG_VERSION")
            ))
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .expect("failed to build HTTP client");

        Ok(Self { pool, http_client })
    }
}
