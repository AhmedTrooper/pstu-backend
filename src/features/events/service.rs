use crate::core::state::AppState;
use sqlx::{PgConnection, Postgres, Transaction};
use uuid::Uuid;

pub async fn log_event_txn(
    tx: &mut Transaction<'_, Postgres>,
    entity_type: &str,
    entity_id: Uuid,
    event: &str,
    actor_id: Option<Uuid>,
    reason: &str,
    meta: serde_json::Value,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO process_events (entity_type, entity_id, event, actor_id, reason, meta, created_at)
        VALUES ($1, $2, $3, $4, $5, $6, now())
        "#,
    )
    .bind(entity_type)
    .bind(entity_id)
    .bind(event)
    .bind(actor_id)
    .bind(reason)
    .bind(meta)
    .execute(&mut **tx)
    .await?;

    Ok(())
}

pub async fn log_event_conn(
    conn: &mut PgConnection,
    entity_type: &str,
    entity_id: Uuid,
    event: &str,
    actor_id: Option<Uuid>,
    reason: &str,
    meta: serde_json::Value,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO process_events (entity_type, entity_id, event, actor_id, reason, meta, created_at)
        VALUES ($1, $2, $3, $4, $5, $6, now())
        "#,
    )
    .bind(entity_type)
    .bind(entity_id)
    .bind(event)
    .bind(actor_id)
    .bind(reason)
    .bind(meta)
    .execute(conn)
    .await?;

    Ok(())
}

pub async fn log_event(
    state: &AppState,
    entity_type: &str,
    entity_id: Uuid,
    event: &str,
    actor_id: Option<Uuid>,
    reason: &str,
    meta: serde_json::Value,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO process_events (entity_type, entity_id, event, actor_id, reason, meta, created_at)
        VALUES ($1, $2, $3, $4, $5, $6, now())
        "#,
    )
    .bind(entity_type)
    .bind(entity_id)
    .bind(event)
    .bind(actor_id)
    .bind(reason)
    .bind(meta)
    .execute(&state.db)
    .await?;

    Ok(())
}
