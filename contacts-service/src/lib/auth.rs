use std::env;

use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};
use serde::Deserialize;

pub const AUTH_SCHEME: &str = "Bearer";

#[derive(Debug, Deserialize, Clone)]
pub struct AuthClaims {
    pub sub: String,
    #[serde(default)]
    pub roles: Vec<String>,
}

#[derive(Debug)]
pub enum AuthError {
    MissingHeader,
    InvalidHeaderFormat,
    InvalidToken,
}

impl AuthError {
    // Returns a machine-readable error code string for the auth failure variant
    pub fn code(&self) -> &'static str {
        match self {
            Self::MissingHeader => "AUTH_REQUIRED",
            Self::InvalidHeaderFormat => "AUTH_INVALID_FORMAT",
            Self::InvalidToken => "AUTH_INVALID_TOKEN",
        }
    }

    // Returns a human-readable error message for the auth failure variant
    pub fn message(&self) -> &'static str {
        match self {
            Self::MissingHeader => "authorization header is required",
            Self::InvalidHeaderFormat => "authorization header format must be 'Bearer <token>'",
            Self::InvalidToken => "token validation failed",
        }
    }
}

// Reads the JWT secret from AUTH_JWT_SECRET env var — panics at startup if unset
fn auth_secret() -> String {
    env::var("AUTH_JWT_SECRET").expect("AUTH_JWT_SECRET must be set")
}

// Reads AUTH_JWT_ALGORITHM env var and returns the corresponding jsonwebtoken Algorithm
fn auth_algorithm() -> Algorithm {
    let configured = env::var("AUTH_JWT_ALGORITHM").unwrap_or_else(|_| "HS256".to_string());
    match configured.trim().to_uppercase().as_str() {
        "RS256" => Algorithm::RS256,
        "RS384" => Algorithm::RS384,
        "RS512" => Algorithm::RS512,
        "HS384" => Algorithm::HS384,
        "HS512" => Algorithm::HS512,
        _ => Algorithm::HS256,
    }
}

// Reads the expected JWT issuer from AUTH_ISSUER env var, defaulting to "auth-service"
fn auth_issuer() -> String {
    env::var("AUTH_ISSUER").unwrap_or_else(|_| "auth-service".to_string())
}

// Replaces literal "\n" escape sequences with real newlines for PEM key strings
fn normalise_pem(raw: &str) -> String {
    raw.replace("\\n", "\n")
}

// Constructs the JWT decoding key, using an RSA public key for RS* algorithms or the HMAC secret otherwise
fn decoding_key(algorithm: Algorithm) -> Result<DecodingKey, AuthError> {
    match algorithm {
        Algorithm::RS256 | Algorithm::RS384 | Algorithm::RS512 => {
            let raw = env::var("AUTH_JWT_PUBLIC_KEY").map_err(|_| AuthError::InvalidToken)?;
            let pem = normalise_pem(&raw);
            DecodingKey::from_rsa_pem(pem.as_bytes()).map_err(|_| AuthError::InvalidToken)
        }
        _ => Ok(DecodingKey::from_secret(auth_secret().as_bytes())),
    }
}

// Splits an Authorization header value and returns the Bearer token, or an error if malformed
fn extract_bearer_token(header_value: &str) -> Result<&str, AuthError> {
    let mut parts = header_value.split_whitespace();
    let Some(scheme) = parts.next() else {
        return Err(AuthError::InvalidHeaderFormat);
    };
    let Some(token) = parts.next() else {
        return Err(AuthError::InvalidHeaderFormat);
    };
    if parts.next().is_some() || !scheme.eq_ignore_ascii_case(AUTH_SCHEME) {
        return Err(AuthError::InvalidHeaderFormat);
    }
    Ok(token)
}

// Validates a JWT Authorization header value and returns the decoded claims on success
pub fn validate_authorization_header(header_value: Option<&str>) -> Result<AuthClaims, AuthError> {
    let raw_header = header_value.ok_or(AuthError::MissingHeader)?;
    let token = extract_bearer_token(raw_header)?;

    let algorithm = auth_algorithm();
    let mut validation = Validation::new(algorithm);
    validation.validate_exp = true;
    validation.set_issuer(&[auth_issuer()]);

    let key = decoding_key(algorithm)?;
    let token_data =
        decode::<AuthClaims>(token, &key, &validation).map_err(|_| AuthError::InvalidToken)?;

    Ok(token_data.claims)
}
