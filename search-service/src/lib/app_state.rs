use sqlx::{postgres::PgPoolOptions, PgPool};

#[derive(Clone)]
pub struct AppState {
    pub(crate) pool: PgPool,       // write pool (primary)
    pub(crate) read_pool: PgPool,  // read pool (replica, or same as primary)
}

impl AppState {
    pub async fn from_database_url(database_url: &str) -> Result<Self, sqlx::Error> {
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(database_url)
            .await?;

        sqlx::migrate!("./migrations").run(&pool).await?;

        Ok(Self {
            read_pool: pool.clone(),
            pool,
        })
    }

    /// Runs migrations against `write_url` (primary), then opens separate pools
    /// for writes (primary) and reads (replica or primary if not set).
    pub async fn with_read_replica(
        write_url: &str,
        read_url: Option<&str>,
    ) -> Result<Self, sqlx::Error> {
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(write_url)
            .await?;
        sqlx::migrate!("./migrations").run(&pool).await?;

        let read_pool = match read_url {
            Some(url) => PgPoolOptions::new()
                .max_connections(10)
                .connect(url)
                .await?,
            None => pool.clone(),
        };

        Ok(Self { pool, read_pool })
    }
}
