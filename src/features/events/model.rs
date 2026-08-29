use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ProcessEventRow {
    pub id: i64,
    pub entity_type: String,
    pub entity_id: Uuid,
    pub event: String,
    pub actor_id: Option<Uuid>,
    pub reason: String,
    pub meta: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CounterpartyDto {
    pub name: String,
    pub account_number: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessEventDto {
    pub id: i64,
    pub entity_type: String,
    pub entity_id: Uuid,
    pub event: String,
    pub actor: Option<CounterpartyDto>,
    pub reason: String,
    pub meta: serde_json::Value,
    pub created_at: DateTime<Utc>,
}
