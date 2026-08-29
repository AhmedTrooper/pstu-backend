use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use validator::Validate;

#[derive(Debug, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct CreateTransferRequest {
    #[validate(length(min = 1, message = "recipient identifier cannot be empty"))]
    pub recipient: String,

    #[validate(length(min = 1, message = "amount_paisa cannot be empty"))]
    pub amount_paisa: String,

    #[validate(length(max = 200, message = "note cannot exceed 200 characters"))]
    pub note: Option<String>,

    pub idempotency_key: Uuid,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TransferResponse {
    pub id: Uuid,
    pub sender_id: Uuid,
    pub recipient_id: Uuid,
    pub amount_paisa: String,
    pub note: String,
    pub status: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct CounterpartyInfo {
    pub id: Uuid,
    pub name: String,
    pub account_number: String,
}

#[derive(Debug, Serialize)]
pub struct TransferDetailResponse {
    pub transfer: TransferResponse,
    pub sender: CounterpartyInfo,
    pub recipient: CounterpartyInfo,
}
