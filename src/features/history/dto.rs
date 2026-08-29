use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct TransactionHistoryQuery {
    pub direction: Option<String>,
    pub kind: Option<String>,
    pub status: Option<String>,
    pub counterparty: Option<String>,
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
    pub amount_min: Option<String>,
    pub amount_max: Option<String>,
    pub q: Option<String>,
    pub cursor: Option<i64>,
    pub limit: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CounterpartyDto {
    pub name: String,
    pub account_number: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TransactionItemDto {
    pub id: i64,
    pub reference: String,
    pub kind: String,
    pub direction: String,
    pub status: String,
    #[serde(rename = "amount", alias = "amount_paisa")]
    pub amount_paisa: String,
    #[serde(alias = "balance_after")]
    pub running_balance: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub counterparty: Option<CounterpartyDto>,
    pub note: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PaginatedTransactionsResponse {
    pub items: Vec<TransactionItemDto>,
    pub next_cursor: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct StatementQuery {
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize)]
pub struct ActivityQuery {
    pub cursor: Option<i64>,
    pub limit: Option<i64>,
}
