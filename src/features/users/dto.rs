use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Serialize)]
pub struct UserProfileResponse {
    pub id: Uuid,
    pub name: String,
    pub phone: String,
    pub account_number: String,
    pub balance: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct LookupQuery {
    pub q: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct UserLookupDto {
    pub id: Uuid,
    pub name: String,
    pub account_number: String,
    pub phone: String,
}

#[derive(Debug, Serialize)]
pub struct PublicUserRes {
    pub name: String,
    pub account_number: String,
}
