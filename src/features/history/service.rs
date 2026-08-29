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
    // Validate direction filter if provided
    if matches!(query.direction.as_deref(), Some(d) if d != "sent" && d != "received" && d != "-1" && d != "1")
    {
        return Err(AppError::BadRequest(
            "Invalid direction filter: must be 'sent' or 'received'".to_string(),
        ));
    }

    let dir_normalized = query.direction.as_deref().map(|d| match d {
        "-1" | "sent" => "sent",
        "1" | "received" => "received",
        other => other,
    });

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

    // Primary index query on tx_history table (§2, §14)
    let rows = sqlx::query(
        r#"
        SELECT th.id, th.reference, th.kind, th.direction, th.status, th.amount_paisa, th.balance_after, th.note, th.created_at,
               u.name as counterparty_name, u.account_number as counterparty_account
        FROM tx_history th
        LEFT JOIN users u ON u.id = th.counterparty_id
        WHERE th.user_id = $1
          AND ($2::bigint IS NULL OR th.id < $2)
          AND ($3::text IS NULL OR th.direction = $3)
          AND ($4::text IS NULL OR th.kind = $4)
          AND ($5::text IS NULL OR th.status = $5)
          AND ($6::timestamptz IS NULL OR th.created_at >= $6)
          AND ($7::timestamptz IS NULL OR th.created_at <= $7)
          AND ($8::bigint IS NULL OR th.amount_paisa >= $8)
          AND ($9::bigint IS NULL OR th.amount_paisa <= $9)
          AND ($10::text IS NULL OR th.reference ILIKE $10 || '%' OR u.name ILIKE '%' || $10 || '%' OR u.phone = $10 OR u.account_number = $10)
        ORDER BY th.id DESC
        LIMIT $11
        "#,
    )
    .bind(user_id)
    .bind(query.cursor)
    .bind(dir_normalized)
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
            reference: row.get("reference"),
            kind: row.get("kind"),
            direction: row.get("direction"),
            status: row.get("status"),
            amount_paisa: row.get::<i64, _>("amount_paisa").to_string(),
            running_balance: row.get::<i64, _>("balance_after").to_string(),
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
        SELECT th.reference, th.kind, th.direction, th.amount_paisa, th.balance_after, th.created_at,
               u.name as counterparty_name, u.account_number as counterparty_account
        FROM tx_history th
        LEFT JOIN users u ON u.id = th.counterparty_id
        WHERE th.user_id = $1
          AND th.status = 'completed'
          AND ($2::timestamptz IS NULL OR th.created_at >= $2)
          AND ($3::timestamptz IS NULL OR th.created_at <= $3)
        ORDER BY th.id ASC
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

    let mut csv_output =
        String::from("reference,created_at,kind,direction,amount,running_balance,counterparty\n");

    for r in rows {
        let reference: String = r.get("reference");
        let kind: String = r.get("kind");
        let direction: String = r.get("direction");
        let amount_paisa: i64 = r.get("amount_paisa");
        let running_balance: i64 = r.get("balance_after");
        let counterparty_name: Option<String> = r.get("counterparty_name");
        let counterparty_account: Option<String> = r.get("counterparty_account");
        let created_at: chrono::DateTime<chrono::Utc> = r.get("created_at");

        let counterparty_display = match (counterparty_name, counterparty_account) {
            (Some(name), Some(acc)) => format!("{} ({})", name, acc),
            (Some(name), None) => name,
            _ => String::new(),
        };

        csv_output.push_str(&format!(
            "{},{},{},{},{},{},\"{}\"\n",
            reference,
            created_at.to_rfc3339(),
            kind,
            direction,
            amount_paisa,
            running_balance,
            counterparty_display
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
