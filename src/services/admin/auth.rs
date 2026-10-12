// JWT authentication module

use anyhow::{anyhow, Result};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};

/// JWT Claims
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    pub user_id: i64,
    pub org_id: i64,
    pub role: String, // super_admin / org_admin / user
    pub exp: usize,
    pub iat: usize,
}

/// Default JWT secret (overridden by config or env)
const DEFAULT_JWT_SECRET: &str = "mrouter-secret-key";

/// Get the JWT secret from environment or fallback to config/default
pub fn get_jwt_secret(config_secret: Option<&str>) -> String {
    std::env::var("MROUTER_JWT_SECRET")
        .unwrap_or_else(|_| {
            config_secret
                .unwrap_or(DEFAULT_JWT_SECRET)
                .to_string()
        })
}

/// Create a JWT token for the given user
pub fn create_token(user_id: i64, org_id: i64, role: &str, secret: &str, expiry_hours: u64) -> Result<String> {
    let now = chrono::Utc::now().timestamp_millis() as usize;
    let exp = now + (expiry_hours as usize * 3600);

    let claims = Claims {
        user_id,
        org_id,
        role: role.to_string(),
        exp,
        iat: now,
    };

    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .map_err(|e| anyhow!("Failed to create JWT token: {}", e))
}

/// Verify a JWT token and return claims
pub fn verify_token(token: &str, secret: &str) -> Result<Claims> {
    let token_data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::default(),
    )
    .map_err(|e| anyhow!("Invalid token: {}", e))?;

    Ok(token_data.claims)
}
/// Hash a password using bcrypt
pub fn hash_password(password: &str) -> Result<String> {
    bcrypt::hash(password, bcrypt::DEFAULT_COST)
        .map_err(|e| anyhow!("Failed to hash password: {}", e))
}

/// Verify a password against a bcrypt hash
pub fn verify_password(password: &str, hash: &str) -> Result<bool> {
    bcrypt::verify(password, hash)
        .map_err(|e| anyhow!("Failed to verify password: {}", e))
}
