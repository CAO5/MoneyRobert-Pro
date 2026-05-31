use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};

use crate::config::SecurityConfig;
use crate::error::{AppError, Result};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims {
    pub sub: String,
    pub user_id: Option<i64>,
    pub username: Option<String>,
    pub role: Option<String>,
    pub exp: i64,
    #[serde(default)]
    pub iat: i64,
    #[serde(default)]
    pub r#type: Option<String>,
}

impl Claims {
    pub fn new(user_id: i64, username: String, role: String, expires_in: Duration) -> Self {
        let now = Utc::now();
        Self {
            sub: user_id.to_string(),
            user_id: Some(user_id),
            username: Some(username),
            role: Some(role),
            exp: (now + expires_in).timestamp(),
            iat: now.timestamp(),
            r#type: Some("access".to_string()),
        }
    }

    pub fn get_user_id(&self) -> i64 {
        self.user_id
            .or_else(|| self.sub.parse().ok())
            .unwrap_or(0)
    }

    pub fn get_username(&self) -> String {
        self.username.clone().unwrap_or_default()
    }

    pub fn get_role(&self) -> String {
        self.role.clone().unwrap_or_else(|| "VIEWER".to_string())
    }

    pub fn generate_token(&self, secret: &str) -> Result<String> {
        encode(
            &Header::default(),
            self,
            &EncodingKey::from_secret(secret.as_bytes()),
        )
        .map_err(|e| AppError::Jwt(e))
    }

    pub fn from_token(token: &str, secret: &str) -> Result<Self> {
        let mut validation = Validation::default();
        validation.validate_exp = true;

        let token_data = decode::<Claims>(
            token,
            &DecodingKey::from_secret(secret.as_bytes()),
            &validation,
        )
        .map_err(|e| AppError::Authentication(format!("Invalid token: {}", e)))?;

        Ok(token_data.claims)
    }
}

pub fn hash_password(password: &str) -> Result<String> {
    bcrypt::hash(password, bcrypt::DEFAULT_COST)
        .map_err(|e| AppError::Internal(format!("Failed to hash password: {}", e)))
}

pub fn verify_password(password: &str, hash: &str) -> Result<bool> {
    bcrypt::verify(password, hash)
        .map_err(|e| AppError::Internal(format!("Failed to verify password: {}", e)))
}
