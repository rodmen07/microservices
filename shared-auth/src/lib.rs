use axum_jwt_auth::{Decoder, LocalDecoder};
use jsonwebtoken::{Algorithm, DecodingKey, Validation};
use serde::de::DeserializeOwned;
use std::sync::Arc;

pub fn build_decoder_from_env<T>() -> Decoder<T>
where
    T: Clone + Send + Sync + DeserializeOwned + 'static,
{
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
            let secret = std::env::var("AUTH_JWT_SECRET").expect("AUTH_JWT_SECRET must be set");
            DecodingKey::from_secret(secret.as_bytes())
        }
    };

    let mut validation = Validation::new(algorithm);
    validation.validate_exp = true;
    validation.validate_aud = false;
    validation.required_spec_claims.remove("aud");
    if let Ok(issuer) = std::env::var("AUTH_ISSUER") {
        validation.set_issuer(&[issuer]);
    }
    if let Ok(audience) = std::env::var("AUTH_AUDIENCE") {
        validation.set_audience(&[audience]);
        validation.validate_aud = true;
    }

    Arc::new(
        LocalDecoder::builder()
            .keys(vec![key])
            .validation(validation)
            .build()
            .expect("failed to build JWT decoder"),
    )
}

fn parse_algorithm(value: &str) -> Algorithm {
    match value.trim().to_uppercase().as_str() {
        "RS256" => Algorithm::RS256,
        "RS384" => Algorithm::RS384,
        "RS512" => Algorithm::RS512,
        "HS384" => Algorithm::HS384,
        "HS512" => Algorithm::HS512,
        _ => Algorithm::HS256,
    }
}
