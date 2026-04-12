use sqlx::{postgres::PgPoolOptions, PgPool};

#[derive(Clone)]
pub struct AppState {
    pub(crate) pool: PgPool,
    pub(crate) http: reqwest::Client,
    pub(crate) observaboard_ingest_url: Option<String>,
    pub(crate) observaboard_api_key: Option<String>,
}

impl AppState {
    pub async fn new(
        database_url: &str,
        observaboard_ingest_url: Option<String>,
        observaboard_api_key: Option<String>,
    ) -> Result<Self, sqlx::Error> {
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(database_url)
            .await?;

        sqlx::migrate!("./migrations").run(&pool).await?;

        let http = reqwest::Client::new();

        Ok(Self {
            pool,
            http,
            observaboard_ingest_url,
            observaboard_api_key,
        })
    }
}
