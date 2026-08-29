use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct PaymentLinkRow {
    pub id: Uuid,
    pub creator_id: Uuid,
    pub amount_paisa: i64,
    pub note: String,
    pub token: String,
    pub status: String,
    pub claimer_id: Option<Uuid>,
    pub transfer_id: Option<Uuid>,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub claimed_at: Option<DateTime<Utc>>,
    pub cancelled_at: Option<DateTime<Utc>>,
}
