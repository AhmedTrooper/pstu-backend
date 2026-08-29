use crate::core::error::AppError;
use crate::core::money::Paisa;
use crate::core::state::AppState;
use crate::features::events::model::ProcessEventDto;
use crate::features::history::dto::{
    CounterpartyDto, PaginatedTransactionsResponse, StatementQuery, TransactionHistoryQuery,
    TransactionItemDto,
};
use sqlx::Row;
use uuid::Uuid;

const DEFAULT_LIMIT: i64 = 20;
const MAX_LIMIT: i64 = 100;
const MAX_STATEMENT_ROWS: usize = 10_000;

pub async fn get_transactions(
    state: &AppState,
    user_id: Uuid,
    query: TransactionHistoryQuery,
) -> Result<PaginatedTransactionsResponse, AppError> {
    // Validate direction enum if provided
    if matches!(query.direction, Some(dir) if dir != -1 && dir != 1) {
        return Err(AppError::BadRequest(
            "Invalid direction filter: must be -1 (sent) or 1 (received)".to_string(),
        ));
    }

    // Validate kind enum if provided
    if let Some(ref k) = query.kind {
        let valid_kinds = [
            "funding",
            "transfer_sent",
            "transfer_received",
            "request_paid",
            "link_paid",
        ];
        if !valid_kinds.contains(&k.as_str()) {
            return Err(AppError::BadRequest(format!(
                "Invalid kind filter: '{}'",
                k
            )));
        }
    }

    let limit = query.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let fetch_limit = limit + 1;

    let min_paisa = if let Some(ref p) = query.amount_min {
        Some(
            Paisa::parse_from_str(p)
                .map_err(|e| AppError::BadRequest(format!("Invalid amount_min: {}", e)))?
                .0,
        )
    } else {
        None
    };

    let max_paisa = if let Some(ref p) = query.amount_max {
        Some(
            Paisa::parse_from_str(p)
                .map_err(|e| AppError::BadRequest(format!("Invalid amount_max: {}", e)))?
                .0,
        )
    } else {
        None
    };

    // Primary index query on idx_ledger_user_id_id_desc (§14)
    let rows = sqlx::query(
        r#"
        SELECT l.id, l.txn_id, l.kind, l.direction, l.amount_paisa, l.running_balance, l.created_at,
               COALESCE(t.status, 'completed') as status,
               COALESCE(t.note, '') as note,
               u.name as counterparty_name, u.account_number as counterparty_account
        FROM ledger l
        LEFT JOIN transfers t ON t.id = l.txn_id
        LEFT JOIN users u ON u.id = l.counterparty_id
        WHERE l.user_id = $1
          AND ($2::bigint IS NULL OR l.id < $2)
          AND ($3::smallint IS NULL OR l.direction = $3)
          AND ($4::text IS NULL OR l.kind = $4)
          AND ($5::text IS NULL OR COALESCE(t.status, 'completed') = $5)
          AND ($6::timestamptz IS NULL OR l.created_at >= $6)
          AND ($7::timestamptz IS NULL OR l.created_at <= $7)
          AND ($8::bigint IS NULL OR l.amount_paisa >= $8)
          AND ($9::bigint IS NULL OR l.amount_paisa <= $9)
          AND ($10::text IS NULL OR u.name ILIKE '%' || $10 || '%' OR u.phone = $10 OR u.account_number = $10)
        ORDER BY l.id DESC
        LIMIT $11
        "#,
    )
    .bind(user_id)
    .bind(query.cursor)
    .bind(query.direction)
    .bind(query.kind)
    .bind(query.status)
    .bind(query.from)
    .bind(query.to)
    .bind(min_paisa)
    .bind(max_paisa)
    .bind(query.counterparty.or(query.q))
    .bind(fetch_limit)
    .fetch_all(&state.db)
    .await?;

    let mut items = Vec::new();
    let has_more = rows.len() as i64 == fetch_limit;
    let take_count = if has_more { limit as usize } else { rows.len() };

    for row in rows.into_iter().take(take_count) {
        let counterparty = row
            .get::<Option<String>, _>("counterparty_name")
            .map(|name| CounterpartyDto {
                name,
                account_number: row.get("counterparty_account"),
            });

        items.push(TransactionItemDto {
            id: row.get("id"),
            txn_id: row.get("txn_id"),
            kind: row.get("kind"),
            direction: row.get("direction"),
            status: row.get("status"),
            amount_paisa: row.get::<i64, _>("amount_paisa").to_string(),
            running_balance: row.get::<i64, _>("running_balance").to_string(),
            counterparty,
            note: row.get("note"),
            created_at: row.get("created_at"),
        });
    }

    let next_cursor = if has_more {
        items.last().map(|it| it.id)
    } else {
        None
    };

    Ok(PaginatedTransactionsResponse { items, next_cursor })
}

pub async fn generate_statement_csv(
    state: &AppState,
    user_id: Uuid,
    query: StatementQuery,
) -> Result<String, AppError> {
    let rows = sqlx::query(
        r#"
        SELECT l.id, l.txn_id, l.kind, l.direction, l.amount_paisa, l.running_balance, l.created_at,
               u.name as counterparty_name, u.account_number as counterparty_account
        FROM ledger l
        LEFT JOIN users u ON u.id = l.counterparty_id
        WHERE l.user_id = $1
          AND ($2::timestamptz IS NULL OR l.created_at >= $2)
          AND ($3::timestamptz IS NULL OR l.created_at <= $3)
        ORDER BY l.id ASC
        LIMIT 10001
        "#,
    )
    .bind(user_id)
    .bind(query.from)
    .bind(query.to)
    .fetch_all(&state.db)
    .await?;

    if rows.len() > MAX_STATEMENT_ROWS {
        return Err(AppError::Unprocessable(
            "Statement range contains too many records (max 10,000). Please narrow your date range."
                .to_string(),
        ));
    }

    let mut csv_output = String::from(
        "id,txn_id,kind,direction,amount_paisa,running_balance,counterparty_name,counterparty_account,created_at\n",
    );

    for r in rows {
        let id: i64 = r.get("id");
        let txn_id: Uuid = r.get("txn_id");
        let kind: String = r.get("kind");
        let direction: i16 = r.get("direction");
        let amount_paisa: i64 = r.get("amount_paisa");
        let running_balance: i64 = r.get("running_balance");
        let counterparty_name: String = r
            .get::<Option<String>, _>("counterparty_name")
            .unwrap_or_default();
        let counterparty_account: String = r
            .get::<Option<String>, _>("counterparty_account")
            .unwrap_or_default();
        let created_at: chrono::DateTime<chrono::Utc> = r.get("created_at");

        csv_output.push_str(&format!(
            "{},{},{},{},{},{},\"{}\",\"{}\",{}\n",
            id,
            txn_id,
            kind,
            direction,
            amount_paisa,
            running_balance,
            counterparty_name,
            counterparty_account,
            created_at.to_rfc3339()
        ));
    }

    Ok(csv_output)
}

pub async fn get_user_activity(
    state: &AppState,
    user_id: Uuid,
    cursor: Option<i64>,
    limit: Option<i64>,
) -> Result<Vec<ProcessEventDto>, AppError> {
    let lim = limit.unwrap_or(20).clamp(1, 100);

    let rows = sqlx::query(
        r#"
        SELECT pe.id, pe.entity_type, pe.entity_id, pe.event, pe.actor_id, pe.reason, pe.meta, pe.created_at,
               u.name as actor_name, u.account_number as actor_account
        FROM process_events pe
        LEFT JOIN users u ON u.id = pe.actor_id
        WHERE (pe.actor_id = $1 OR pe.entity_id = $1)
          AND ($2::bigint IS NULL OR pe.id < $2)
        ORDER BY pe.id DESC
        LIMIT $3
        "#,
    )
    .bind(user_id)
    .bind(cursor)
    .bind(lim)
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
