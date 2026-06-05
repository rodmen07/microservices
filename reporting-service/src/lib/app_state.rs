use axum_jwt_auth::Decoder;
use sqlx::{postgres::PgPoolOptions, PgPool};
use crate::auth::AuthClaims;
use shared_auth::build_decoder_from_env;
#[derive(Clone)]
pub struct AppState {
    pub(crate) pool: PgPool,
    pub(crate) decoder: Decoder<AuthClaims>,
}
impl axum::extract::FromRef<AppState> for Decoder<AuthClaims> {
    fn from_ref(state: &AppState) -> Self {
        state.decoder.clone()
    }
}
impl AppState {
    pub async fn from_database_url(database_url: &str) -> Result<Self, sqlx::Error> {
        let pool = PgPoolOptions::new().max_connections(5).connect(database_url).await?;
        sqlx::migrate!("./migrations").run(&pool).await?;
        Ok(Self { pool, decoder: build_decoder_from_env() })
    }
    pub async fn with_read_replica(write_url: &str, read_url: Option<&str>) -> Result<Self, sqlx::Error> {
        let migrate_pool = PgPoolOptions::new().max_connections(1).connect(write_url).await?;
        sqlx::migrate!("./migrations").run(&migrate_pool).await?;
        drop(migrate_pool);
        let pool = PgPoolOptions::new().max_connections(5).connect(read_url.unwrap_or(write_url)).await?;
        Ok(Self { pool, decoder: build_decoder_from_env() })
    }

    pub async fn clear_reports_for_tests(&self) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM reports").execute(&self.pool).await?;
        Ok(())
    }
}