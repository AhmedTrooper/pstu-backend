use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct MoneyRequestRow {
    pub id: Uuid,
    pub requester_id: Uuid,
    pub debtor_id: Uuid,
    pub amount_paisa: i64,
    pub note: String,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub resolved_at: Option<DateTime<Utc>>,
}
