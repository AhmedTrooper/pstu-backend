use crate::core::error::AppError;
use crate::core::money::Paisa;
use crate::core::pin::enforce_user_pin;
use crate::core::reference::generate_trx_reference;
use crate::core::state::AppState;
use crate::features::events::model::ProcessEventDto;
use crate::features::events::service::log_event_txn;
use crate::features::links::dto::{
    ClaimPaymentLinkRequest, ClaimPaymentLinkResponse, CounterpartyDto, CreatePaymentLinkRequest,
    GetMyLinksQuery, PaginatedLinksResponse, PaymentLinkDto,
};
use crate::features::transfers::dto::TransferResponse;
use chrono::{Duration, Utc};
use rand::RngExt;
use sqlx::Row;
use uuid::Uuid;

const MIN_LINK_TTL_SECS: u64 = 60;
const MAX_LINK_TTL_SECS: u64 = 86400; // 24 hours

pub fn generate_link_token() -> String {
    let mut bytes = [0u8; 16];
    rand::rng().fill(&mut bytes);
    hex::encode(bytes)
}

pub async fn create_link(
    state: &AppState,
    creator_id: Uuid,
    req: CreatePaymentLinkRequest,
) -> Result<PaymentLinkDto, AppError> {
    // 1. Verify creator PIN (R17, C46, C47)
    enforce_user_pin(state, creator_id, &req.pin).await?;

    let amount = Paisa::parse_positive_from_str(&req.amount_paisa)
        .map_err(|e| AppError::BadRequest(e.to_string()))?;

    let ttl_secs = req
        .expires_in_seconds
        .unwrap_or(state.config.default_link_ttl_secs);

    if !(MIN_LINK_TTL_SECS..=MAX_LINK_TTL_SECS).contains(&ttl_secs) {
        return Err(AppError::BadRequest(format!(
            "expires_in_seconds must be between {} and {} seconds",
            MIN_LINK_TTL_SECS, MAX_LINK_TTL_SECS
        )));
    }

    let note = req.note.unwrap_or_default().trim().to_string();
    let token = generate_link_token();
    let link_id = Uuid::new_v4();
    let now = Utc::now();
    let expires_at = now + Duration::seconds(ttl_secs as i64);

    let mut tx = state.db.begin().await?;

    sqlx::query(
        r#"
        INSERT INTO payment_links (id, creator_id, amount_paisa, note, token, status, expires_at, created_at)
        VALUES ($1, $2, $3, $4, $5, 'active', $6, $7)
        "#,
    )
    .bind(link_id)
    .bind(creator_id)
    .bind(amount.0)
    .bind(&note)
    .bind(&token)
    .bind(expires_at)
    .bind(now)
    .execute(&mut *tx)
    .await?;

    log_event_txn(
        &mut tx,
        "link",
        link_id,
        "created",
        Some(creator_id),
        "Payment link created",
        serde_json::json!({
            "creator_id": creator_id,
            "amount_paisa": amount.0,
            "ttl_secs": ttl_secs
        }),
    )
    .await?;

    tx.commit().await?;

    Ok(PaymentLinkDto {
        id: link_id,
        token: token.clone(),
        url: format!("/l/{}", token),
        amount_paisa: amount.0.to_string(),
        note,
        creator_name: None,
        status: "active".to_string(),
        expires_at,
        created_at: now,
        claimed_at: None,
        cancelled_at: None,
        claimer: None,
    })
}

pub async fn get_link_by_token(state: &AppState, token: &str) -> Result<PaymentLinkDto, AppError> {
    let row = sqlx::query(
        r#"
        SELECT pl.id, pl.creator_id, pl.amount_paisa, pl.note, pl.token, pl.status, pl.expires_at, pl.created_at,
               pl.claimed_at, pl.cancelled_at,
               u.name as creator_name
        FROM payment_links pl
        JOIN users u ON u.id = pl.creator_id
        WHERE pl.token = $1
        "#,
    )
    .bind(token)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("Payment link not found".to_string()))?;

    let link_id: Uuid = row.get("id");
    let mut status: String = row.get("status");
    let expires_at: chrono::DateTime<chrono::Utc> = row.get("expires_at");

    // Lazy expiry evaluation (§5 W5)
    if status == "active" && expires_at < Utc::now() {
        status = "expired".to_string();
        let _ = sqlx::query("UPDATE payment_links SET status = 'expired' WHERE id = $1")
            .bind(link_id)
            .execute(&state.db)
            .await;
    }

    Ok(PaymentLinkDto {
        id: link_id,
        token: row.get("token"),
        url: format!("/l/{}", token),
        amount_paisa: row.get::<i64, _>("amount_paisa").to_string(),
        note: row.get("note"),
        creator_name: Some(row.get("creator_name")),
        status,
        expires_at,
        created_at: row.get("created_at"),
        claimed_at: row.get("claimed_at"),
        cancelled_at: row.get("cancelled_at"),
        claimer: None,
    })
}

pub async fn claim_link(
    state: &AppState,
    claimer_id: Uuid,
    token: &str,
    payload: ClaimPaymentLinkRequest,
) -> Result<ClaimPaymentLinkResponse, AppError> {
    // 1. Verify claimer PIN (R17, C46, C47)
    enforce_user_pin(state, claimer_id, &payload.pin).await?;

    let mut tx = state.db.begin().await?;

    // Lock payment link row FOR UPDATE (§5 W5)
    let link_row = sqlx::query(
        r#"
        SELECT id, creator_id, amount_paisa, note, token, status, claimer_id, transfer_id, expires_at, created_at
        FROM payment_links
        WHERE token = $1
        FOR UPDATE
        "#,
    )
    .bind(token)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| AppError::NotFound("Payment link not found".to_string()))?;

    let link_id: Uuid = link_row.get("id");
    let creator_id: Uuid = link_row.get("creator_id");
    let amount_paisa: i64 = link_row.get("amount_paisa");
    let note: String = link_row.get("note");
    let status: String = link_row.get("status");
    let existing_claimer: Option<Uuid> = link_row.get("claimer_id");
    let existing_transfer: Option<Uuid> = link_row.get("transfer_id");
    let expires_at: chrono::DateTime<chrono::Utc> = link_row.get("expires_at");

    // Handle already claimed link replay vs conflict (C22)
    if status == "claimed" {
        if existing_claimer == Some(claimer_id) {
            let tr_id = existing_transfer.unwrap_or(link_id);
            let tr_ref: String =
                sqlx::query_scalar("SELECT reference FROM transfers WHERE id = $1")
                    .bind(tr_id)
                    .fetch_optional(&mut *tx)
                    .await?
                    .unwrap_or_else(generate_trx_reference);

            let transfer_dto = TransferResponse {
                id: tr_id,
                reference: tr_ref,
                sender_id: creator_id,
                recipient_id: claimer_id,
                amount_paisa: amount_paisa.to_string(),
                note: note.clone(),
                status: "completed".to_string(),
                created_at: link_row.get("created_at"),
            };

            let link_dto = PaymentLinkDto {
                id: link_id,
                token: token.to_string(),
                url: format!("/l/{}", token),
                amount_paisa: amount_paisa.to_string(),
                note,
                creator_name: None,
                status: "claimed".to_string(),
                expires_at,
                created_at: link_row.get("created_at"),
                claimed_at: link_row.get("created_at"),
                cancelled_at: None,
                claimer: None,
            };

            return Ok(ClaimPaymentLinkResponse {
                transfer: transfer_dto,
                link: link_dto,
            });
        } else {
            return Err(AppError::Conflict(
                "Payment link was already claimed by another user".to_string(),
            ));
        }
    }

    // Handle expired link (C21 -> 410 Gone)
    if status == "expired" || expires_at < Utc::now() {
        sqlx::query("UPDATE payment_links SET status = 'expired' WHERE id = $1")
            .bind(link_id)
            .execute(&mut *tx)
            .await?;

        log_event_txn(
            &mut tx,
            "link",
            link_id,
            "expired",
            Some(claimer_id),
            "Claim attempted on expired link",
            serde_json::json!({ "token": token }),
        )
        .await?;

        tx.commit().await?;
        return Err(AppError::Gone("Payment link has expired".to_string()));
    }

    if status == "cancelled" {
        return Err(AppError::Conflict(
            "Payment link has been cancelled by creator".to_string(),
        ));
    }

    // Self-claim check (C23 -> 400 Bad Request)
    if creator_id == claimer_id {
        return Err(AppError::BadRequest(
            "Cannot claim your own payment link".to_string(),
        ));
    }

    // Ascending balance lock order (R5)
    let (first_id, second_id) = if creator_id < claimer_id {
        (creator_id, claimer_id)
    } else {
        (claimer_id, creator_id)
    };

    let balance_rows = sqlx::query(
        r#"
        SELECT user_id, amount_paisa
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
        return Err(AppError::NotFound("Balances not found".to_string()));
    }

    let mut creator_balance: Option<i64> = None;
    let mut claimer_balance: Option<i64> = None;

    for row in balance_rows {
        let uid: Uuid = row.get("user_id");
        let bal: i64 = row.get("amount_paisa");
        if uid == creator_id {
            creator_balance = Some(bal);
        } else if uid == claimer_id {
            claimer_balance = Some(bal);
        }
    }

    let creator_bal =
        creator_balance.ok_or_else(|| AppError::NotFound("Creator balance missing".to_string()))?;
    let claimer_bal =
        claimer_balance.ok_or_else(|| AppError::NotFound("Claimer balance missing".to_string()))?;

    // Insufficient funds check (C24 -> 422, link stays active)
    if creator_bal < amount_paisa {
        return Err(AppError::Unprocessable(
            "Creator has insufficient balance to fulfill link".to_string(),
        ));
    }

    let new_creator_bal = creator_bal - amount_paisa;
    let new_claimer_bal = claimer_bal + amount_paisa;

    // Update balances
    sqlx::query("UPDATE balances SET amount_paisa = $1, version = version + 1, updated_at = now() WHERE user_id = $2")
        .bind(new_creator_bal)
        .bind(creator_id)
        .execute(&mut *tx)
        .await?;

    sqlx::query("UPDATE balances SET amount_paisa = $1, version = version + 1, updated_at = now() WHERE user_id = $2")
        .bind(new_claimer_bal)
        .bind(claimer_id)
        .execute(&mut *tx)
        .await?;

    let reference = generate_trx_reference();

    // Insert completed transfer (W5)
    let transfer_id = Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO transfers (id, reference, sender_id, recipient_id, amount_paisa, note, status, idempotency_key, created_at)
        VALUES ($1, $2, $3, $4, $5, $6, 'completed', $7, now())
        "#,
    )
    .bind(transfer_id)
    .bind(&reference)
    .bind(creator_id)
    .bind(claimer_id)
    .bind(amount_paisa)
    .bind(&note)
    .bind(payload.idempotency_key)
    .execute(&mut *tx)
    .await?;

    // Append 2 double-entry ledger rows (R3, R19)
    sqlx::query(
        r#"
        INSERT INTO ledger (txn_id, reference, user_id, counterparty_id, direction, amount_paisa, running_balance, kind, idempotency_key, created_at)
        VALUES ($1, $2, $3, $4, -1, $5, $6, 'link_paid', $7, now())
        "#,
    )
    .bind(transfer_id)
    .bind(&reference)
    .bind(creator_id)
    .bind(claimer_id)
    .bind(amount_paisa)
    .bind(new_creator_bal)
    .bind(payload.idempotency_key)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        r#"
        INSERT INTO ledger (txn_id, reference, user_id, counterparty_id, direction, amount_paisa, running_balance, kind, idempotency_key, created_at)
        VALUES ($1, $2, $3, $4, 1, $5, $6, 'link_paid', $7, now())
        "#,
    )
    .bind(transfer_id)
    .bind(&reference)
    .bind(claimer_id)
    .bind(creator_id)
    .bind(amount_paisa)
    .bind(new_claimer_bal)
    .bind(payload.idempotency_key)
    .execute(&mut *tx)
    .await?;

    // Insert 2 tx_history rows (§2, W5)
    sqlx::query(
        r#"
        INSERT INTO tx_history (user_id, status, kind, direction, amount_paisa, balance_after, counterparty_id, reference, entity_id, note, idempotency_key, created_at)
        VALUES ($1, 'completed', 'link', 'sent', $2, $3, $4, $5, $6, $7, $8, now())
        "#,
    )
    .bind(creator_id)
    .bind(amount_paisa)
    .bind(new_creator_bal)
    .bind(claimer_id)
    .bind(&reference)
    .bind(transfer_id)
    .bind(&note)
    .bind(payload.idempotency_key)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        r#"
        INSERT INTO tx_history (user_id, status, kind, direction, amount_paisa, balance_after, counterparty_id, reference, entity_id, note, idempotency_key, created_at)
        VALUES ($1, 'completed', 'link', 'received', $2, $3, $4, $5, $6, $7, $8, now())
        "#,
    )
    .bind(claimer_id)
    .bind(amount_paisa)
    .bind(new_claimer_bal)
    .bind(creator_id)
    .bind(&reference)
    .bind(transfer_id)
    .bind(&note)
    .bind(payload.idempotency_key)
    .execute(&mut *tx)
    .await?;

    // Update payment link to claimed
    let now = Utc::now();
    sqlx::query(
        r#"
        UPDATE payment_links
        SET status = 'claimed', claimer_id = $1, transfer_id = $2, claimed_at = $3
        WHERE id = $4
        "#,
    )
    .bind(claimer_id)
    .bind(transfer_id)
    .bind(now)
    .bind(link_id)
    .execute(&mut *tx)
    .await?;

    log_event_txn(
        &mut tx,
        "link",
        link_id,
        "claimed",
        Some(claimer_id),
        "Payment link claimed successfully",
        serde_json::json!({
            "link_id": link_id,
            "claimer_id": claimer_id,
            "transfer_id": transfer_id,
            "reference": reference,
            "amount_paisa": amount_paisa
        }),
    )
    .await?;

    tx.commit().await?;

    let transfer_dto = TransferResponse {
        id: transfer_id,
        reference,
        sender_id: creator_id,
        recipient_id: claimer_id,
        amount_paisa: amount_paisa.to_string(),
        note: note.clone(),
        status: "completed".to_string(),
        created_at: now,
    };

    let link_dto = PaymentLinkDto {
        id: link_id,
        token: token.to_string(),
        url: format!("/l/{}", token),
        amount_paisa: amount_paisa.to_string(),
        note,
        creator_name: None,
        status: "claimed".to_string(),
        expires_at,
        created_at: link_row.get("created_at"),
        claimed_at: Some(now),
        cancelled_at: None,
        claimer: None,
    };

    Ok(ClaimPaymentLinkResponse {
        transfer: transfer_dto,
        link: link_dto,
    })
}

pub async fn cancel_link(
    state: &AppState,
    actor_id: Uuid,
    token: &str,
) -> Result<PaymentLinkDto, AppError> {
    let mut tx = state.db.begin().await?;

    let link_row = sqlx::query(
        r#"
        SELECT id, creator_id, amount_paisa, note, token, status, expires_at, created_at
        FROM payment_links
        WHERE token = $1
        FOR UPDATE
        "#,
    )
    .bind(token)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| AppError::NotFound("Payment link not found".to_string()))?;

    let link_id: Uuid = link_row.get("id");
    let creator_id: Uuid = link_row.get("creator_id");
    let status: String = link_row.get("status");

    if actor_id != creator_id {
        return Err(AppError::Forbidden(
            "Only the link creator can cancel it".to_string(),
        ));
    }

    if status == "claimed" {
        return Err(AppError::Conflict(
            "Cannot cancel a link that is already claimed".to_string(),
        ));
    }

    if status == "expired" {
        return Err(AppError::Gone("Cannot cancel an expired link".to_string()));
    }

    let now = Utc::now();
    sqlx::query("UPDATE payment_links SET status = 'cancelled', cancelled_at = $1 WHERE id = $2")
        .bind(now)
        .bind(link_id)
        .execute(&mut *tx)
        .await?;

    log_event_txn(
        &mut tx,
        "link",
        link_id,
        "cancelled",
        Some(actor_id),
        "Payment link cancelled by creator",
        serde_json::json!({ "token": token }),
    )
    .await?;

    tx.commit().await?;

    Ok(PaymentLinkDto {
        id: link_id,
        token: token.to_string(),
        url: format!("/l/{}", token),
        amount_paisa: link_row.get::<i64, _>("amount_paisa").to_string(),
        note: link_row.get("note"),
        creator_name: None,
        status: "cancelled".to_string(),
        expires_at: link_row.get("expires_at"),
        created_at: link_row.get("created_at"),
        claimed_at: None,
        cancelled_at: Some(now),
        claimer: None,
    })
}

pub async fn get_my_links(
    state: &AppState,
    creator_id: Uuid,
    query: GetMyLinksQuery,
) -> Result<PaginatedLinksResponse, AppError> {
    let limit = query.limit.unwrap_or(20).clamp(1, 100);
    let fetch_limit = limit + 1;

    let rows = sqlx::query(
        r#"
        SELECT pl.id, pl.creator_id, pl.amount_paisa, pl.note, pl.token, pl.status, pl.expires_at, pl.created_at,
               pl.claimed_at, pl.cancelled_at,
               u.name as claimer_name, u.account_number as claimer_account
        FROM payment_links pl
        LEFT JOIN users u ON u.id = pl.claimer_id
        WHERE pl.creator_id = $1
          AND ($2::text IS NULL OR pl.status = $2)
          AND ($3::timestamptz IS NULL OR pl.created_at < $3)
        ORDER BY pl.created_at DESC
        LIMIT $4
        "#,
    )
    .bind(creator_id)
    .bind(query.status)
    .bind(query.cursor)
    .bind(fetch_limit)
    .fetch_all(&state.db)
    .await?;

    let mut items = Vec::new();
    let has_more = rows.len() as i64 == fetch_limit;
    let take_count = if has_more { limit as usize } else { rows.len() };

    for row in rows.into_iter().take(take_count) {
        let claimer = row
            .get::<Option<String>, _>("claimer_name")
            .map(|name| CounterpartyDto {
                name,
                account_number: row.get("claimer_account"),
            });

        items.push(PaymentLinkDto {
            id: row.get("id"),
            token: row.get("token"),
            url: format!("/l/{}", row.get::<String, _>("token")),
            amount_paisa: row.get::<i64, _>("amount_paisa").to_string(),
            note: row.get("note"),
            creator_name: None,
            status: row.get("status"),
            expires_at: row.get("expires_at"),
            created_at: row.get("created_at"),
            claimed_at: row.get("claimed_at"),
            cancelled_at: row.get("cancelled_at"),
            claimer,
        });
    }

    let next_cursor = if has_more {
        items.last().map(|it| it.created_at)
    } else {
        None
    };

    Ok(PaginatedLinksResponse { items, next_cursor })
}

pub async fn get_link_events(
    state: &AppState,
    actor_id: Uuid,
    link_id: Uuid,
) -> Result<Vec<ProcessEventDto>, AppError> {
    let link = sqlx::query("SELECT creator_id, claimer_id FROM payment_links WHERE id = $1")
        .bind(link_id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("Payment link not found".to_string()))?;

    let creator_id: Uuid = link.get("creator_id");
    let claimer_id: Option<Uuid> = link.get("claimer_id");

    if actor_id != creator_id && claimer_id != Some(actor_id) {
        return Err(AppError::NotFound("Payment link not found".to_string()));
    }

    let rows = sqlx::query(
        r#"
        SELECT pe.id, pe.entity_type, pe.entity_id, pe.event, pe.actor_id, pe.reason, pe.meta, pe.created_at,
               u.name as actor_name, u.account_number as actor_account
        FROM process_events pe
        LEFT JOIN users u ON u.id = pe.actor_id
        WHERE pe.entity_type = 'link' AND pe.entity_id = $1
        ORDER BY pe.id ASC
        "#,
    )
    .bind(link_id)
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
