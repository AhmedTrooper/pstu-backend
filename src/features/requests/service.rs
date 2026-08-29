use crate::core::error::AppError;
use crate::core::money::Paisa;
use crate::core::pin::enforce_user_pin;
use crate::core::reference::generate_trx_reference;
use crate::core::state::AppState;
use crate::features::events::model::ProcessEventDto;
use crate::features::events::service::log_event_txn;
use crate::features::requests::dto::{
    AcceptMoneyRequest, AcceptRequestResponse, CounterpartyDto, CreateMoneyRequest,
    GetRequestsQuery, MoneyRequestDto, PaginatedRequestsResponse,
};
use crate::features::transfers::dto::TransferResponse;
use crate::features::transfers::service::resolve_recipient;
use chrono::Utc;
use sqlx::Row;
use uuid::Uuid;

pub async fn create_request(
    state: &AppState,
    requester_id: Uuid,
    req: CreateMoneyRequest,
) -> Result<MoneyRequestDto, AppError> {
    let amount = Paisa::parse_positive_from_str(&req.amount_paisa)
        .map_err(|e| AppError::BadRequest(e.to_string()))?;

    let debtor_id = resolve_recipient(state, &req.debtor).await?;

    if requester_id == debtor_id {
        return Err(AppError::BadRequest(
            "Cannot request money from yourself".to_string(),
        ));
    }

    let note = req.note.unwrap_or_default().trim().to_string();
    let request_id = Uuid::new_v4();

    let mut tx = state.db.begin().await?;

    sqlx::query(
        r#"
        INSERT INTO money_requests (id, requester_id, debtor_id, amount_paisa, note, status, created_at)
        VALUES ($1, $2, $3, $4, $5, 'pending', now())
        "#,
    )
    .bind(request_id)
    .bind(requester_id)
    .bind(debtor_id)
    .bind(amount.0)
    .bind(&note)
    .execute(&mut *tx)
    .await?;

    log_event_txn(
        &mut tx,
        "request",
        request_id,
        "created",
        Some(requester_id),
        "Money request created",
        serde_json::json!({
            "requester_id": requester_id,
            "debtor_id": debtor_id,
            "amount_paisa": amount.0
        }),
    )
    .await?;

    tx.commit().await?;

    Ok(MoneyRequestDto {
        id: request_id,
        requester_id,
        debtor_id,
        amount_paisa: amount.0.to_string(),
        note,
        status: "pending".to_string(),
        created_at: Utc::now(),
        resolved_at: None,
        requester: None,
        debtor: None,
    })
}

pub async fn get_requests(
    state: &AppState,
    actor_id: Uuid,
    query: GetRequestsQuery,
) -> Result<PaginatedRequestsResponse, AppError> {
    let limit = query.limit.unwrap_or(20).clamp(1, 100);
    let fetch_limit = limit + 1;

    let role = query.role.as_deref().unwrap_or("all");

    let rows = sqlx::query(
        r#"
        SELECT mr.id, mr.requester_id, mr.debtor_id, mr.amount_paisa, mr.note, mr.status, mr.created_at, mr.resolved_at,
               req_u.name as req_name, req_u.account_number as req_account,
               deb_u.name as deb_name, deb_u.account_number as deb_account
        FROM money_requests mr
        JOIN users req_u ON req_u.id = mr.requester_id
        JOIN users deb_u ON deb_u.id = mr.debtor_id
        WHERE (
            (($1 = 'incoming' OR $1 = 'debtor') AND mr.debtor_id = $2) OR
            (($1 = 'outgoing' OR $1 = 'requester') AND mr.requester_id = $2) OR
            ($1 = 'all' AND (mr.debtor_id = $2 OR mr.requester_id = $2))
        )
        AND ($3::text IS NULL OR mr.status = $3)
        AND ($4::timestamptz IS NULL OR mr.created_at < $4)
        ORDER BY mr.created_at DESC
        LIMIT $5
        "#,
    )
    .bind(role)
    .bind(actor_id)
    .bind(query.status)
    .bind(query.cursor)
    .bind(fetch_limit)
    .fetch_all(&state.db)
    .await?;

    let mut items = Vec::new();
    let has_more = rows.len() as i64 == fetch_limit;
    let take_count = if has_more { limit as usize } else { rows.len() };

    for row in rows.into_iter().take(take_count) {
        let requester = Some(CounterpartyDto {
            id: row.get("requester_id"),
            name: row.get("req_name"),
            account_number: row.get("req_account"),
        });

        let debtor = Some(CounterpartyDto {
            id: row.get("debtor_id"),
            name: row.get("deb_name"),
            account_number: row.get("deb_account"),
        });

        items.push(MoneyRequestDto {
            id: row.get("id"),
            requester_id: row.get("requester_id"),
            debtor_id: row.get("debtor_id"),
            amount_paisa: row.get::<i64, _>("amount_paisa").to_string(),
            note: row.get("note"),
            status: row.get("status"),
            created_at: row.get("created_at"),
            resolved_at: row.get("resolved_at"),
            requester,
            debtor,
        });
    }

    let next_cursor = if has_more {
        items.last().map(|it| it.created_at)
    } else {
        None
    };

    Ok(PaginatedRequestsResponse { items, next_cursor })
}

pub async fn accept_request(
    state: &AppState,
    actor_id: Uuid,
    request_id: Uuid,
    payload: AcceptMoneyRequest,
) -> Result<AcceptRequestResponse, AppError> {
    // 1. Verify debtor PIN (R17, C46, C47)
    enforce_user_pin(state, actor_id, &payload.pin).await?;

    let mut tx = state.db.begin().await?;

    // Lock request row FOR UPDATE (§5 W3, C11)
    let request_row = sqlx::query(
        r#"
        SELECT id, requester_id, debtor_id, amount_paisa, note, status, created_at
        FROM money_requests
        WHERE id = $1
        FOR UPDATE
        "#,
    )
    .bind(request_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| AppError::NotFound("Money request not found".to_string()))?;

    let requester_id: Uuid = request_row.get("requester_id");
    let debtor_id: Uuid = request_row.get("debtor_id");
    let amount_paisa: i64 = request_row.get("amount_paisa");
    let note: String = request_row.get("note");
    let status: String = request_row.get("status");

    // Only pending requests can be accepted (C11)
    if status != "pending" {
        return Err(AppError::Conflict(format!(
            "Money request cannot be accepted because it is already {}",
            status
        )));
    }

    // Only debtor can accept (C12)
    if actor_id != debtor_id {
        return Err(AppError::Forbidden(
            "Only the recipient (debtor) can accept this money request".to_string(),
        ));
    }

    // Money movement: Debtor pays Requester (R5 ascending lock order)
    let (first_id, second_id) = if debtor_id < requester_id {
        (debtor_id, requester_id)
    } else {
        (requester_id, debtor_id)
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

    let mut debtor_balance: Option<i64> = None;
    let mut requester_balance: Option<i64> = None;

    for row in balance_rows {
        let uid: Uuid = row.get("user_id");
        let bal: i64 = row.get("amount_paisa");
        if uid == debtor_id {
            debtor_balance = Some(bal);
        } else if uid == requester_id {
            requester_balance = Some(bal);
        }
    }

    let debtor_bal =
        debtor_balance.ok_or_else(|| AppError::NotFound("Debtor balance missing".to_string()))?;
    let requester_bal = requester_balance
        .ok_or_else(|| AppError::NotFound("Requester balance missing".to_string()))?;

    if debtor_bal < amount_paisa {
        return Err(AppError::Unprocessable(
            "Insufficient balance to accept request".to_string(),
        ));
    }

    let new_debtor_bal = debtor_bal - amount_paisa;
    let new_requester_bal = requester_bal + amount_paisa;

    // Update balances
    sqlx::query("UPDATE balances SET amount_paisa = $1, version = version + 1, updated_at = now() WHERE user_id = $2")
        .bind(new_debtor_bal)
        .bind(debtor_id)
        .execute(&mut *tx)
        .await?;

    sqlx::query("UPDATE balances SET amount_paisa = $1, version = version + 1, updated_at = now() WHERE user_id = $2")
        .bind(new_requester_bal)
        .bind(requester_id)
        .execute(&mut *tx)
        .await?;

    let reference = generate_trx_reference();

    // Insert completed transfer
    let transfer_id = Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO transfers (id, reference, sender_id, recipient_id, amount_paisa, note, status, idempotency_key, created_at)
        VALUES ($1, $2, $3, $4, $5, $6, 'completed', $7, now())
        "#,
    )
    .bind(transfer_id)
    .bind(&reference)
    .bind(debtor_id)
    .bind(requester_id)
    .bind(amount_paisa)
    .bind(&note)
    .bind(payload.idempotency_key)
    .execute(&mut *tx)
    .await?;

    // Append 2 double-entry ledger rows
    sqlx::query(
        r#"
        INSERT INTO ledger (txn_id, reference, user_id, counterparty_id, direction, amount_paisa, running_balance, kind, idempotency_key, created_at)
        VALUES ($1, $2, $3, $4, -1, $5, $6, 'request_paid', $7, now())
        "#,
    )
    .bind(transfer_id)
    .bind(&reference)
    .bind(debtor_id)
    .bind(requester_id)
    .bind(amount_paisa)
    .bind(new_debtor_bal)
    .bind(payload.idempotency_key)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        r#"
        INSERT INTO ledger (txn_id, reference, user_id, counterparty_id, direction, amount_paisa, running_balance, kind, idempotency_key, created_at)
        VALUES ($1, $2, $3, $4, 1, $5, $6, 'request_paid', $7, now())
        "#,
    )
    .bind(transfer_id)
    .bind(&reference)
    .bind(requester_id)
    .bind(debtor_id)
    .bind(amount_paisa)
    .bind(new_requester_bal)
    .bind(payload.idempotency_key)
    .execute(&mut *tx)
    .await?;

    // Insert 2 tx_history rows (§2, W3)
    sqlx::query(
        r#"
        INSERT INTO tx_history (user_id, status, kind, direction, amount_paisa, balance_after, counterparty_id, reference, entity_id, note, idempotency_key, created_at)
        VALUES ($1, 'completed', 'request', 'sent', $2, $3, $4, $5, $6, $7, $8, now())
        "#,
    )
    .bind(debtor_id)
    .bind(amount_paisa)
    .bind(new_debtor_bal)
    .bind(requester_id)
    .bind(&reference)
    .bind(transfer_id)
    .bind(&note)
    .bind(payload.idempotency_key)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        r#"
        INSERT INTO tx_history (user_id, status, kind, direction, amount_paisa, balance_after, counterparty_id, reference, entity_id, note, idempotency_key, created_at)
        VALUES ($1, 'completed', 'request', 'received', $2, $3, $4, $5, $6, $7, $8, now())
        "#,
    )
    .bind(requester_id)
    .bind(amount_paisa)
    .bind(new_requester_bal)
    .bind(debtor_id)
    .bind(&reference)
    .bind(transfer_id)
    .bind(&note)
    .bind(payload.idempotency_key)
    .execute(&mut *tx)
    .await?;

    // Update request to accepted
    let now = Utc::now();
    sqlx::query("UPDATE money_requests SET status = 'accepted', resolved_at = $1 WHERE id = $2")
        .bind(now)
        .bind(request_id)
        .execute(&mut *tx)
        .await?;

    log_event_txn(
        &mut tx,
        "request",
        request_id,
        "accepted",
        Some(actor_id),
        "Money request accepted and paid",
        serde_json::json!({
            "request_id": request_id,
            "transfer_id": transfer_id,
            "reference": reference,
            "amount_paisa": amount_paisa
        }),
    )
    .await?;

    tx.commit().await?;

    // Asynchronous request paid email (W3, R22)
    state.mailer.dispatch_email(
        format!("{}@pstupay.local", requester_id),
        format!("Money Request Paid - {}", reference),
        format!(
            "Your money request of ৳{:.2} has been paid.\nTrxID: {}\nNote: {}\nThank you,\nPSTU Pay Team",
            amount_paisa as f64 / 100.0, reference, note
        ),
    );

    let req_dto = MoneyRequestDto {
        id: request_id,
        requester_id,
        debtor_id,
        amount_paisa: amount_paisa.to_string(),
        note: note.clone(),
        status: "accepted".to_string(),
        created_at: request_row.get("created_at"),
        resolved_at: Some(now),
        requester: None,
        debtor: None,
    };

    let transfer_dto = TransferResponse {
        id: transfer_id,
        reference,
        sender_id: debtor_id,
        recipient_id: requester_id,
        amount_paisa: amount_paisa.to_string(),
        note,
        status: "completed".to_string(),
        created_at: now,
    };

    Ok(AcceptRequestResponse {
        request: req_dto,
        transfer: transfer_dto,
    })
}

pub async fn reject_request(
    state: &AppState,
    actor_id: Uuid,
    request_id: Uuid,
) -> Result<MoneyRequestDto, AppError> {
    let mut tx = state.db.begin().await?;

    let request_row = sqlx::query(
        r#"
        SELECT id, requester_id, debtor_id, amount_paisa, note, status, created_at
        FROM money_requests
        WHERE id = $1
        FOR UPDATE
        "#,
    )
    .bind(request_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| AppError::NotFound("Money request not found".to_string()))?;

    let requester_id: Uuid = request_row.get("requester_id");
    let debtor_id: Uuid = request_row.get("debtor_id");
    let status: String = request_row.get("status");

    if status != "pending" {
        return Err(AppError::Conflict(format!(
            "Money request is already {}",
            status
        )));
    }

    if actor_id != debtor_id {
        return Err(AppError::Forbidden(
            "Only the debtor can reject this request".to_string(),
        ));
    }

    let now = Utc::now();
    sqlx::query("UPDATE money_requests SET status = 'rejected', resolved_at = $1 WHERE id = $2")
        .bind(now)
        .bind(request_id)
        .execute(&mut *tx)
        .await?;

    log_event_txn(
        &mut tx,
        "request",
        request_id,
        "rejected",
        Some(actor_id),
        "Money request rejected by debtor",
        serde_json::json!({ "status": "rejected" }),
    )
    .await?;

    tx.commit().await?;

    Ok(MoneyRequestDto {
        id: request_id,
        requester_id,
        debtor_id,
        amount_paisa: request_row.get::<i64, _>("amount_paisa").to_string(),
        note: request_row.get("note"),
        status: "rejected".to_string(),
        created_at: request_row.get("created_at"),
        resolved_at: Some(now),
        requester: None,
        debtor: None,
    })
}

pub async fn cancel_request(
    state: &AppState,
    actor_id: Uuid,
    request_id: Uuid,
) -> Result<MoneyRequestDto, AppError> {
    let mut tx = state.db.begin().await?;

    let request_row = sqlx::query(
        r#"
        SELECT id, requester_id, debtor_id, amount_paisa, note, status, created_at
        FROM money_requests
        WHERE id = $1
        FOR UPDATE
        "#,
    )
    .bind(request_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| AppError::NotFound("Money request not found".to_string()))?;

    let requester_id: Uuid = request_row.get("requester_id");
    let debtor_id: Uuid = request_row.get("debtor_id");
    let status: String = request_row.get("status");

    if status != "pending" {
        return Err(AppError::Conflict(format!(
            "Money request cannot be cancelled because it is already {}",
            status
        )));
    }

    if actor_id != requester_id {
        return Err(AppError::Forbidden(
            "Only the requester can cancel this request".to_string(),
        ));
    }

    let now = Utc::now();
    sqlx::query("UPDATE money_requests SET status = 'cancelled', resolved_at = $1 WHERE id = $2")
        .bind(now)
        .bind(request_id)
        .execute(&mut *tx)
        .await?;

    log_event_txn(
        &mut tx,
        "request",
        request_id,
        "cancelled",
        Some(actor_id),
        "Money request cancelled by requester",
        serde_json::json!({ "status": "cancelled" }),
    )
    .await?;

    tx.commit().await?;

    Ok(MoneyRequestDto {
        id: request_id,
        requester_id,
        debtor_id,
        amount_paisa: request_row.get::<i64, _>("amount_paisa").to_string(),
        note: request_row.get("note"),
        status: "cancelled".to_string(),
        created_at: request_row.get("created_at"),
        resolved_at: Some(now),
        requester: None,
        debtor: None,
    })
}

pub async fn get_request_events(
    state: &AppState,
    actor_id: Uuid,
    request_id: Uuid,
) -> Result<Vec<ProcessEventDto>, AppError> {
    let req = sqlx::query("SELECT requester_id, debtor_id FROM money_requests WHERE id = $1")
        .bind(request_id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("Money request not found".to_string()))?;

    let requester_id: Uuid = req.get("requester_id");
    let debtor_id: Uuid = req.get("debtor_id");

    if actor_id != requester_id && actor_id != debtor_id {
        return Err(AppError::NotFound("Money request not found".to_string()));
    }

    let rows = sqlx::query(
        r#"
        SELECT pe.id, pe.entity_type, pe.entity_id, pe.event, pe.actor_id, pe.reason, pe.meta, pe.created_at,
               u.name as actor_name, u.account_number as actor_account
        FROM process_events pe
        LEFT JOIN users u ON u.id = pe.actor_id
        WHERE pe.entity_type = 'request' AND pe.entity_id = $1
        ORDER BY pe.id ASC
        "#,
    )
    .bind(request_id)
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
