use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct UserRow {
    pub id: Uuid,
    pub account_number: String,
    pub name: String,
    pub phone: String,
    pub password_hash: String,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct BalanceRow {
    pub user_id: Uuid,
    pub amount_paisa: i64,
    pub version: i64,
    pub updated_at: DateTime<Utc>,
}
