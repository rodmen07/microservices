use std::sync::Arc;
use axum_jwt_auth::{Decoder, LocalDecoder};
use jsonwebtoken::{Algorithm, DecodingKey, Validation};
use sqlx::{postgres::PgPoolOptions, PgPool};
use crate::auth::AuthClaims;
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
        let http_client = reqwest::Client::builder().user_agent(concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"))).timeout(std::time::Duration::from_secs(10)).build().expect("failed to build HTTP client");
        Ok(Self { pool, http_client, decoder: build_decoder() })
    }
}
fn build_decoder() -> Decoder<AuthClaims> {
    let algorithm = parse_algorithm(
        &std::env::var("AUTH_JWT_ALGORITHM").unwrap_or_else(|_| "HS256".to_string()),
    );
    let key = match algorithm {
        Algorithm::RS256 | Algorithm::RS384 | Algorithm::RS512 => {
            let raw = std::env::var("AUTH_JWT_PUBLIC_KEY")
                .expect("AUTH_JWT_PUBLIC_KEY must be set for RS* algorithms");
            DecodingKey::from_rsa_pem(raw.replace("\\n", "\n").as_bytes())
                .expect("invalid RSA public key PEM")
        }
        _ => {
            let secret =
                std::env::var("AUTH_JWT_SECRET").expect("AUTH_JWT_SECRET must be set");
            DecodingKey::from_secret(secret.as_bytes())
        }
    };
    let mut validation = Validation::new(algorithm);
    validation.validate_exp = true;
    if let Ok(issuer) = std::env::var("AUTH_ISSUER") {
        validation.set_issuer(&[issuer]);
    }
    Arc::new(
        LocalDecoder::builder()
            .keys(vec![key])
            .validation(validation)
            .build()
            .expect("failed to build JWT decoder"),
    )
}
fn parse_algorithm(s: &str) -> Algorithm {
    match s.trim().to_uppercase().as_str() {
        "RS256" => Algorithm::RS256,
        "RS384" => Algorithm::RS384,
        "RS512" => Algorithm::RS512,
        "HS384" => Algorithm::HS384,
        "HS512" => Algorithm::HS512,
        _ => Algorithm::HS256,
    }
}