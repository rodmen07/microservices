use sqlx::{postgres::PgPoolOptions, PgPool};

#[derive(Clone)]
pub struct AppState {
    pub(crate) pool: PgPool,
}

impl AppState {
    pub async fn from_database_url(database_url: &str) -> Result<Self, sqlx::Error> {
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(database_url)
            .await?;

        sqlx::migrate!("./migrations").run(&pool).await?;

        Ok(Self { pool })
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

        Ok(Self { pool })
    }
}
