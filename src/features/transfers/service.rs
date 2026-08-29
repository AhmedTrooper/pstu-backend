use crate::core::error::AppError;
use crate::core::money::Paisa;
use crate::core::state::AppState;
use crate::features::events::model::ProcessEventDto;
use crate::features::events::service::log_event_txn;
use crate::features::transfers::dto::{
    CounterpartyInfo, CreateTransferRequest, TransferDetailResponse, TransferResponse,
};
use chrono::Utc;
use redis::AsyncCommands;
use sqlx::Row;
use std::str::FromStr;
use tracing::info;
use uuid::Uuid;

// Caps per rules (§0 R13, §6 C35, C36)
pub const PER_TRANSFER_CAP_PAISA: i64 = 5_000_000; // ৳50,000 max single transfer
pub const DAILY_OUTFLOW_CAP_PAISA: i64 = 20_000_000; // ৳200,000 max daily transfer

pub async fn resolve_recipient(state: &AppState, identifier: &str) -> Result<Uuid, AppError> {
    let trimmed = identifier.trim();

    // 1. Try parsing as UUID
    if let Ok(id) = Uuid::from_str(trimmed) {
        let exists: Option<Uuid> = sqlx::query_scalar("SELECT id FROM users WHERE id = $1")
            .bind(id)
            .fetch_optional(&state.db)
            .await?;
        if let Some(user_id) = exists {
            return Ok(user_id);
        }
    }

    // 2. Try lookup by phone or account_number
    let user_id: Option<Uuid> =
        sqlx::query_scalar("SELECT id FROM users WHERE phone = $1 OR account_number = $1")
            .bind(trimmed)
            .fetch_optional(&state.db)
            .await?;

    user_id.ok_or_else(|| AppError::NotFound("Recipient not found".to_string()))
}

pub async fn process_transfer(
    state: &AppState,
    sender_id: Uuid,
    req: CreateTransferRequest,
) -> Result<(TransferResponse, bool), AppError> {
    // 1. Parse and validate amount (R1, C02, C13, C18)
    let amount = Paisa::parse_positive_from_str(&req.amount_paisa)
        .map_err(|e| AppError::BadRequest(e.to_string()))?;

    // 2. Resolve recipient (T12, C03)
    let recipient_id = resolve_recipient(state, &req.recipient).await?;

    // 3. Self-transfer check (C01)
    if sender_id == recipient_id {
        return Err(AppError::BadRequest(
            "Self transfers are not permitted".to_string(),
        ));
    }

    let note = req.note.unwrap_or_default().trim().to_string();

    // 4. Permanent Idempotency Key Check (R4, C06, C07, C34)
    let existing_row = sqlx::query(
        r#"
        SELECT id, sender_id, recipient_id, amount_paisa, note, status, created_at
        FROM transfers
        WHERE idempotency_key = $1
        "#,
    )
    .bind(req.idempotency_key)
    .fetch_optional(&state.db)
    .await?;

    if let Some(row) = existing_row {
        let ex_sender: Uuid = row.get("sender_id");
        let ex_recipient: Uuid = row.get("recipient_id");
        let ex_amount: i64 = row.get("amount_paisa");

        // Same key with matching payload -> 200 replay (C06, C34)
        if ex_sender == sender_id && ex_recipient == recipient_id && ex_amount == amount.0 {
            let resp = TransferResponse {
                id: row.get("id"),
                sender_id: ex_sender,
                recipient_id: ex_recipient,
                amount_paisa: ex_amount.to_string(),
                note: row.get("note"),
                status: row.get("status"),
                created_at: row.get("created_at"),
            };
            return Ok((resp, true)); // true indicates idempotent replay
        } else {
            // Same key with different payload -> 409 Conflict (C07)
            return Err(AppError::Conflict(
                "Idempotency key was previously used with different parameters".to_string(),
            ));
        }
    }

    // 5. Per-Transfer Cap Check (R13, C35)
    if amount.0 > PER_TRANSFER_CAP_PAISA {
        let transfer_id = Uuid::new_v4();
        let mut tx = state.db.begin().await?;

        sqlx::query(
            r#"
            INSERT INTO transfers (id, sender_id, recipient_id, amount_paisa, note, status, idempotency_key, created_at)
            VALUES ($1, $2, $3, $4, $5, 'rejected', $6, now())
            "#,
        )
        .bind(transfer_id)
        .bind(sender_id)
        .bind(recipient_id)
        .bind(amount.0)
        .bind(&note)
        .bind(req.idempotency_key)
        .execute(&mut *tx)
        .await?;

        log_event_txn(
            &mut tx,
            "transfer",
            transfer_id,
            "rejected",
            Some(sender_id),
            "Exceeded single transfer velocity cap",
            serde_json::json!({ "amount_paisa": amount.0, "cap": PER_TRANSFER_CAP_PAISA }),
        )
        .await?;

        tx.commit().await?;

        return Err(AppError::Unprocessable(format!(
            "Amount exceeds maximum per-transfer cap of {} paisa",
            PER_TRANSFER_CAP_PAISA
        )));
    }

    // 6. Daily Outflow Velocity Cap Check (R13, W10, C36)
    let today = Utc::now().format("%Y%m%d").to_string();
    let velocity_key = format!("daily_outflow:{}:{}", sender_id, today);
    let mut redis_conn = state.redis.clone();

    let new_daily_total: i64 = redis_conn.incr(&velocity_key, amount.0).await.unwrap_or(0);
    let _: Result<(), _> = redis_conn.expire(&velocity_key, 86400).await;

    if new_daily_total > DAILY_OUTFLOW_CAP_PAISA {
        let transfer_id = Uuid::new_v4();
        let mut tx = state.db.begin().await?;

        sqlx::query(
            r#"
            INSERT INTO transfers (id, sender_id, recipient_id, amount_paisa, note, status, idempotency_key, created_at)
            VALUES ($1, $2, $3, $4, $5, 'flagged', $6, now())
            "#,
        )
        .bind(transfer_id)
        .bind(sender_id)
        .bind(recipient_id)
        .bind(amount.0)
        .bind(&note)
        .bind(req.idempotency_key)
        .execute(&mut *tx)
        .await?;

        log_event_txn(
            &mut tx,
            "transfer",
            transfer_id,
            "flagged",
            Some(sender_id),
            "Exceeded daily cumulative outflow limit (held for review)",
            serde_json::json!({ "daily_total": new_daily_total, "cap": DAILY_OUTFLOW_CAP_PAISA }),
        )
        .await?;

        tx.commit().await?;

        let resp = TransferResponse {
            id: transfer_id,
            sender_id,
            recipient_id,
            amount_paisa: amount.0.to_string(),
            note,
            status: "flagged".to_string(),
            created_at: Utc::now(),
        };

        return Ok((resp, false));
    }

    // 7. Atomic Transaction with Deadlock-Free Ascending Lock Order (R5, W2)
    let mut tx = state.db.begin().await?;

    let (first_id, second_id) = if sender_id < recipient_id {
        (sender_id, recipient_id)
    } else {
        (recipient_id, sender_id)
    };

    // Lock balances in ascending user_id order (R5)
    let balance_rows = sqlx::query(
        r#"
        SELECT user_id, amount_paisa, version
        FROM balances
        WHERE user_id IN ($1, $2)
        ORDER BY user_id ASC
        FOR UPDATE
        "#,
    )
    .bind(first_id)
    .bind(second_id)
    .fetch_all(&mut *tx)
    .await?;

    if balance_rows.len() < 2 {
        return Err(AppError::NotFound(
            "One or both user balances not found".to_string(),
        ));
    }

    let mut sender_balance: Option<i64> = None;
    let mut recipient_balance: Option<i64> = None;

    for row in balance_rows {
        let uid: Uuid = row.get("user_id");
        let bal: i64 = row.get("amount_paisa");
        if uid == sender_id {
            sender_balance = Some(bal);
        } else if uid == recipient_id {
            recipient_balance = Some(bal);
        }
    }

    let sender_bal =
        sender_balance.ok_or_else(|| AppError::NotFound("Sender balance missing".to_string()))?;
    let recipient_bal = recipient_balance
        .ok_or_else(|| AppError::NotFound("Recipient balance missing".to_string()))?;

    // Check sender sufficient funds (C04, atomic rollback, no ledger entries)
    if sender_bal < amount.0 {
        return Err(AppError::Unprocessable("Insufficient balance".to_string()));
    }

    let new_sender_bal = sender_bal - amount.0;
    let new_recipient_bal = recipient_bal + amount.0;

    // 8. Update balances (version + 1)
    sqlx::query(
        r#"
        UPDATE balances
        SET amount_paisa = $1, version = version + 1, updated_at = now()
        WHERE user_id = $2
        "#,
    )
    .bind(new_sender_bal)
    .bind(sender_id)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        r#"
        UPDATE balances
        SET amount_paisa = $1, version = version + 1, updated_at = now()
        WHERE user_id = $2
        "#,
    )
    .bind(new_recipient_bal)
    .bind(recipient_id)
    .execute(&mut *tx)
    .await?;

    // 9. Insert completed transfer
    let transfer_id = Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO transfers (id, sender_id, recipient_id, amount_paisa, note, status, idempotency_key, created_at)
        VALUES ($1, $2, $3, $4, $5, 'completed', $6, now())
        "#,
    )
    .bind(transfer_id)
    .bind(sender_id)
    .bind(recipient_id)
    .bind(amount.0)
    .bind(&note)
    .bind(req.idempotency_key)
    .execute(&mut *tx)
    .await?;

    // 10. Append 2 double-entry ledger rows (R3)
    // Sender debit (-1)
    sqlx::query(
        r#"
        INSERT INTO ledger (txn_id, user_id, counterparty_id, direction, amount_paisa, running_balance, kind, idempotency_key, created_at)
        VALUES ($1, $2, $3, -1, $4, $5, 'transfer_sent', $6, now())
        "#,
    )
    .bind(transfer_id)
    .bind(sender_id)
    .bind(recipient_id)
    .bind(amount.0)
    .bind(new_sender_bal)
    .bind(req.idempotency_key)
    .execute(&mut *tx)
    .await?;

    // Recipient credit (+1)
    sqlx::query(
        r#"
        INSERT INTO ledger (txn_id, user_id, counterparty_id, direction, amount_paisa, running_balance, kind, idempotency_key, created_at)
        VALUES ($1, $2, $3, 1, $4, $5, 'transfer_received', $6, now())
        "#,
    )
    .bind(transfer_id)
    .bind(recipient_id)
    .bind(sender_id)
    .bind(amount.0)
    .bind(new_recipient_bal)
    .bind(req.idempotency_key)
    .execute(&mut *tx)
    .await?;

    // 11. Record process event (W9)
    log_event_txn(
        &mut tx,
        "transfer",
        transfer_id,
        "completed",
        Some(sender_id),
        "Transfer successfully executed",
        serde_json::json!({
            "sender_id": sender_id,
            "recipient_id": recipient_id,
            "amount_paisa": amount.0,
            "idempotency_key": req.idempotency_key
        }),
    )
    .await?;

    tx.commit().await?;

    // 12. Asynchronous notification fanout (T14)
    if let Some(nats) = &state.nats {
        let event_payload = serde_json::json!({
            "event": "transfer_completed",
            "transfer_id": transfer_id,
            "sender_id": sender_id,
            "recipient_id": recipient_id,
            "amount_paisa": amount.0
        });
        if let Ok(bytes) = serde_json::to_vec(&event_payload) {
            let _ = nats.publish("events.transfer", bytes.into()).await;
        }
    }

    info!(
        transfer_id = %transfer_id,
        sender_id = %sender_id,
        recipient_id = %recipient_id,
        amount_paisa = amount.0,
        "Transfer successfully executed and committed"
    );

    let resp = TransferResponse {
        id: transfer_id,
        sender_id,
        recipient_id,
        amount_paisa: amount.0.to_string(),
        note,
        status: "completed".to_string(),
        created_at: Utc::now(),
    };

    Ok((resp, false))
}

pub async fn get_transfer_detail(
    state: &AppState,
    actor_id: Uuid,
    transfer_id: Uuid,
) -> Result<TransferDetailResponse, AppError> {
    let row = sqlx::query(
        r#"
        SELECT t.id, t.sender_id, t.recipient_id, t.amount_paisa, t.note, t.status, t.created_at,
               s.name as sender_name, s.account_number as sender_account,
               r.name as recipient_name, r.account_number as recipient_account
        FROM transfers t
        JOIN users s ON s.id = t.sender_id
        JOIN users r ON r.id = t.recipient_id
        WHERE t.id = $1
        "#,
    )
    .bind(transfer_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("Transfer not found".to_string()))?;

    let sender_id: Uuid = row.get("sender_id");
    let recipient_id: Uuid = row.get("recipient_id");

    // Object-level authz (R11, C41): only sender or recipient can view
    if actor_id != sender_id && actor_id != recipient_id {
        return Err(AppError::NotFound("Transfer not found".to_string()));
    }

    let transfer = TransferResponse {
        id: row.get("id"),
        sender_id,
        recipient_id,
        amount_paisa: row.get::<i64, _>("amount_paisa").to_string(),
        note: row.get("note"),
        status: row.get("status"),
        created_at: row.get("created_at"),
    };

    let sender = CounterpartyInfo {
        id: sender_id,
        name: row.get("sender_name"),
        account_number: row.get("sender_account"),
    };

    let recipient = CounterpartyInfo {
        id: recipient_id,
        name: row.get("recipient_name"),
        account_number: row.get("recipient_account"),
    };

    Ok(TransferDetailResponse {
        transfer,
        sender,
        recipient,
    })
}

pub async fn get_transfer_events(
    state: &AppState,
    actor_id: Uuid,
    transfer_id: Uuid,
) -> Result<Vec<ProcessEventDto>, AppError> {
    // Verify actor is participant (R11, C38)
    let transfer = sqlx::query("SELECT sender_id, recipient_id FROM transfers WHERE id = $1")
        .bind(transfer_id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("Transfer not found".to_string()))?;

    let sender_id: Uuid = transfer.get("sender_id");
    let recipient_id: Uuid = transfer.get("recipient_id");

    if actor_id != sender_id && actor_id != recipient_id {
        return Err(AppError::NotFound("Transfer not found".to_string()));
    }

    let rows = sqlx::query(
        r#"
        SELECT pe.id, pe.entity_type, pe.entity_id, pe.event, pe.actor_id, pe.reason, pe.meta, pe.created_at,
               u.name as actor_name, u.account_number as actor_account
        FROM process_events pe
        LEFT JOIN users u ON u.id = pe.actor_id
        WHERE pe.entity_type = 'transfer' AND pe.entity_id = $1
        ORDER BY pe.id ASC
        "#,
    )
    .bind(transfer_id)
    .fetch_all(&state.db)
    .await?;

    let events = rows
        .into_iter()
        .map(|r| {
            let actor = r.get::<Option<String>, _>("actor_name").map(|name| {
                crate::features::events::model::CounterpartyDto {
                    name,
                    account_number: r.get("actor_account"),
                }
            });

            ProcessEventDto {
                id: r.get("id"),
                entity_type: r.get("entity_type"),
                entity_id: r.get("entity_id"),
                event: r.get("event"),
                actor,
                reason: r.get("reason"),
                meta: r.get("meta"),
                created_at: r.get("created_at"),
            }
        })
        .collect();

    Ok(events)
}
