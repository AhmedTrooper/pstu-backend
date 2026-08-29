use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use validator::Validate;

#[derive(Debug, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct CreateMoneyRequest {
    #[validate(length(min = 1, message = "debtor identifier cannot be empty"))]
    pub debtor: String,

    #[validate(length(min = 1, message = "amount_paisa cannot be empty"))]
    pub amount_paisa: String,

    #[validate(length(max = 200, message = "note cannot exceed 200 characters"))]
    pub note: Option<String>,
}

#[derive(Debug, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct AcceptMoneyRequest {
    pub idempotency_key: Uuid,
}

#[derive(Debug, Deserialize)]
pub struct GetRequestsQuery {
    pub status: Option<String>,
    pub role: Option<String>, // "incoming" or "outgoing"
    pub cursor: Option<DateTime<Utc>>,
    pub limit: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CounterpartyDto {
    pub id: Uuid,
    pub name: String,
    pub account_number: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MoneyRequestDto {
    pub id: Uuid,
    pub requester_id: Uuid,
    pub debtor_id: Uuid,
    pub amount_paisa: String,
    pub note: String,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub resolved_at: Option<DateTime<Utc>>,
    pub requester: Option<CounterpartyDto>,
    pub debtor: Option<CounterpartyDto>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PaginatedRequestsResponse {
    pub items: Vec<MoneyRequestDto>,
    pub next_cursor: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AcceptRequestResponse {
    pub request: MoneyRequestDto,
    pub transfer: crate::features::transfers::dto::TransferResponse,
}
