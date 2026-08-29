use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use validator::Validate;

#[derive(Debug, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct CreatePaymentLinkRequest {
    #[serde(alias = "amount")]
    #[validate(length(min = 1, message = "amount cannot be empty"))]
    pub amount_paisa: String,

    #[validate(length(max = 120, message = "note cannot exceed 120 characters"))]
    pub note: Option<String>,

    pub expires_in_seconds: Option<u64>,

    #[validate(regex(path = *crate::core::money::PIN_REGEX, message = "PIN must be 4 to 6 digits"))]
    pub pin: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CounterpartyDto {
    pub name: String,
    pub account_number: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PaymentLinkDto {
    pub id: Uuid,
    pub token: String,
    pub url: String,
    #[serde(alias = "amount")]
    pub amount_paisa: String,
    pub note: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub creator_name: Option<String>,
    pub status: String,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub claimed_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cancelled_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub claimer: Option<CounterpartyDto>,
}

#[derive(Debug, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct ClaimPaymentLinkRequest {
    #[validate(regex(path = *crate::core::money::PIN_REGEX, message = "PIN must be 4 to 6 digits"))]
    pub pin: String,

    pub idempotency_key: Uuid,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ClaimPaymentLinkResponse {
    pub transfer: crate::features::transfers::dto::TransferResponse,
    pub link: PaymentLinkDto,
}

#[derive(Debug, Deserialize)]
pub struct GetMyLinksQuery {
    pub status: Option<String>,
    pub cursor: Option<DateTime<Utc>>,
    pub limit: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PaginatedLinksResponse {
    pub items: Vec<PaymentLinkDto>,
    pub next_cursor: Option<DateTime<Utc>>,
}
