use axum_jwt_auth::Decoder;
use sqlx::{postgres::PgPoolOptions, PgPool};
use crate::auth::AuthClaims;
use shared_auth::build_decoder_from_env;
#[derive(Clone)]
pub struct AppState {
    pub(crate) pool: PgPool,
    pub(crate) http_client: reqwest::Client,
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
        let http_client = reqwest::Client::builder().user_agent(concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"))).timeout(std::time::Duration::from_secs(5)).build().expect("failed to build HTTP client");
        Ok(Self { pool, http_client, decoder: build_decoder_from_env() })
    }
}