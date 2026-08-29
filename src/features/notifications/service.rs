use crate::core::error::AppError;
use crate::core::state::AppState;
use crate::features::notifications::dto::{
    NotifQuery, NotifReadReq, NotificationItemDto, NotificationsResponse, OkRes,
};
use chrono::Utc;
use sqlx::Row;
use uuid::Uuid;

pub async fn get_notifications(
    state: &AppState,
    user_id: Uuid,
    query: NotifQuery,
) -> Result<NotificationsResponse, AppError> {
    let limit = query.limit.unwrap_or(20).clamp(1, 100);
    let fetch_limit = limit + 1;
    let unread_only = query.unread_only.unwrap_or(false);

    let rows = sqlx::query(
        r#"
        SELECT id, kind, title, body, entity_type, entity_id, read_at, created_at
        FROM notifications
        WHERE user_id = $1
          AND ($2::boolean IS FALSE OR read_at IS NULL)
          AND ($3::bigint IS NULL OR id < $3)
        ORDER BY id DESC
        LIMIT $4
        "#,
    )
    .bind(user_id)
    .bind(unread_only)
    .bind(query.cursor)
    .bind(fetch_limit)
    .fetch_all(&state.db)
    .await?;

    let unread_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM notifications WHERE user_id = $1 AND read_at IS NULL",
    )
    .bind(user_id)
    .fetch_one(&state.db)
    .await?;

    let mut items = Vec::new();
    let has_more = rows.len() as i64 == fetch_limit;
    let take_count = if has_more { limit as usize } else { rows.len() };

    for r in rows.into_iter().take(take_count) {
        items.push(NotificationItemDto {
            id: r.get("id"),
            kind: r.get("kind"),
            title: r.get("title"),
            body: r.get("body"),
            entity_type: r.get("entity_type"),
            entity_id: r.get("entity_id"),
            read_at: r.get("read_at"),
            created_at: r.get("created_at"),
        });
    }

    let next_cursor = if has_more {
        items.last().map(|it| it.id)
    } else {
        None
    };

    Ok(NotificationsResponse {
        items,
        unread_count,
        next_cursor,
    })
}

pub async fn mark_notifications_read(
    state: &AppState,
    user_id: Uuid,
    req: NotifReadReq,
) -> Result<OkRes, AppError> {
    let now = Utc::now();

    if req.all.unwrap_or(false) {
        sqlx::query("UPDATE notifications SET read_at = $1 WHERE user_id = $2 AND read_at IS NULL")
            .bind(now)
            .bind(user_id)
            .execute(&state.db)
            .await?;
    } else if let Some(ref ids) = req.ids.filter(|ids| !ids.is_empty()) {
        sqlx::query("UPDATE notifications SET read_at = $1 WHERE user_id = $2 AND id = ANY($3)")
            .bind(now)
            .bind(user_id)
            .bind(ids)
            .execute(&state.db)
            .await?;
    }

    Ok(OkRes { ok: true })
}
