//! Authentication data models

use serde::{Deserialize, Serialize};
use sqlx::FromRow;

/// JWT claims structure
#[derive(Serialize, Deserialize, Debug)]
pub struct Claims {
    pub sub: String,
    pub exp: usize,
}

/// User database model
#[derive(FromRow, Serialize, Deserialize, Debug)]
pub struct User {
    pub id: String,
    pub email: String,
    pub name: Option<String>,
    pub avatar: Option<String>,
    pub provider: Option<String>,
    pub provider_id: Option<String>,
    pub created_at: Option<String>,
}

/// Google ID token payload for OAuth
#[derive(Deserialize)]
pub struct GoogleIdTokenPayload {
    pub id_token: String,
}
