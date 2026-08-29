use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use validator::Validate;

#[derive(Debug, Deserialize)]
pub struct NotifQuery {
    pub unread_only: Option<bool>,
    pub cursor: Option<i64>,
    pub limit: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct NotificationItemDto {
    pub id: i64,
    pub kind: String,
    pub title: String,
    pub body: String,
    pub entity_type: Option<String>,
    pub entity_id: Option<Uuid>,
    pub read_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct NotificationsResponse {
    pub items: Vec<NotificationItemDto>,
    pub unread_count: i64,
    pub next_cursor: Option<i64>,
}

#[derive(Debug, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct NotifReadReq {
    pub ids: Option<Vec<i64>>,
    pub all: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct OkRes {
    pub ok: bool,
}
