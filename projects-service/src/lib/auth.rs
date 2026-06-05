use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct AuthClaims {
    pub sub: String,
    #[serde(default)]
    pub roles: Vec<String>,
}

impl AuthClaims {
    pub fn has_role(&self, role: &str) -> bool {
        self.roles.iter().any(|r| r.eq_ignore_ascii_case(role))
    }
}
