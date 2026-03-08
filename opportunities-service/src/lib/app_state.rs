use std::str::FromStr;

use sqlx::{
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
    SqlitePool,
};

#[derive(Clone)]
pub struct AppState {
    pub(crate) pool: SqlitePool,
}

impl AppState {
    pub async fn from_database_url(database_url: &str) -> Result<Self, sqlx::Error> {
        let opts = SqliteConnectOptions::from_str(database_url)?.create_if_missing(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(opts)
            .await?;

        sqlx::migrate!("./migrations").run(&pool).await?;

        Ok(Self { pool })
    }
}
