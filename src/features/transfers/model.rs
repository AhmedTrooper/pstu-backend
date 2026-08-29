use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct TransferRow {
    pub id: Uuid,
    pub sender_id: Uuid,
    pub recipient_id: Uuid,
    pub amount_paisa: i64,
    pub note: String,
    pub status: String,
    pub idempotency_key: Uuid,
    pub created_at: DateTime<Utc>,
}
