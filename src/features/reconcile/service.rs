use crate::core::error::AppError;
use sqlx::{PgPool, Row};
use tracing::info;
use uuid::Uuid;

#[derive(Debug, Default)]
pub struct ReconcileReport {
    pub users_count: i64,
    pub total_system_balance_paisa: i64,
    pub expected_system_balance_paisa: i64,
    pub users_reconciled: i64,
    pub ledger_entries_verified: i64,
    pub errors: Vec<String>,
}

pub async fn run_reconciliation(pool: &PgPool) -> Result<ReconcileReport, AppError> {
    let mut report = ReconcileReport::default();

    // 1. Contract 2: Check total system balance invariant (§2)
    let user_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(pool)
        .await?;

    let total_balance: i64 =
        sqlx::query_scalar("SELECT COALESCE(SUM(amount_paisa), 0)::BIGINT FROM balances")
            .fetch_one(pool)
            .await?;

    let expected_balance = user_count * 10_000_000;
    report.users_count = user_count;
    report.total_system_balance_paisa = total_balance;
    report.expected_system_balance_paisa = expected_balance;

    if total_balance != expected_balance {
        report.errors.push(format!(
            "CONTRACT 2 VIOLATION: Total balance mismatch! Expected {} paisa ({} users * 10M), found {} paisa (difference: {} paisa)",
            expected_balance, user_count, total_balance, total_balance - expected_balance
        ));
    }

    // 2. Contract 1 & 3: Check each user's sum and running_balance chain
    let users = sqlx::query("SELECT id, name, account_number FROM users")
        .fetch_all(pool)
        .await?;

    for user in users {
        let user_id: Uuid = user.get("id");
        let account_number: String = user.get("account_number");

        let current_balance: i64 =
            sqlx::query_scalar("SELECT amount_paisa FROM balances WHERE user_id = $1")
                .bind(user_id)
                .fetch_one(pool)
                .await?;

        // Fetch all ledger rows for user ordered chronologically
        let ledger_rows = sqlx::query(
            "SELECT id, direction, amount_paisa, running_balance FROM ledger WHERE user_id = $1 ORDER BY id ASC",
        )
        .bind(user_id)
        .fetch_all(pool)
        .await?;

        let mut computed_sum: i64 = 0;
        let mut previous_running: Option<i64> = None;

        for row in ledger_rows {
            report.ledger_entries_verified += 1;
            let id: i64 = row.get("id");
            let direction: i16 = row.get("direction");
            let amount: i64 = row.get("amount_paisa");
            let running: i64 = row.get("running_balance");

            let delta = amount * (direction as i64);
            computed_sum += delta;

            // Contract 3: Check running balance chain
            if let Some(prev) = previous_running {
                let expected_running = prev + delta;
                if running != expected_running {
                    report.errors.push(format!(
                        "CONTRACT 3 VIOLATION (Chain Error): User {} (id: {}) ledger row {} expected running_balance {}, found {}",
                        account_number, user_id, id, expected_running, running
                    ));
                }
            } else {
                // First ledger row must equal delta
                if running != delta {
                    report.errors.push(format!(
                        "CONTRACT 3 VIOLATION (Initial Row): User {} (id: {}) first ledger row {} running_balance {} != delta {}",
                        account_number, user_id, id, running, delta
                    ));
                }
            }

            previous_running = Some(running);
        }

        // Contract 1: Check Σ ledger == balance
        if computed_sum != current_balance {
            report.errors.push(format!(
                "CONTRACT 1 VIOLATION: User {} (id: {}) computed ledger sum {} != balance {}",
                account_number, user_id, computed_sum, current_balance
            ));
        } else {
            report.users_reconciled += 1;
        }
    }

    info!(
        users_count = report.users_count,
        users_reconciled = report.users_reconciled,
        ledger_entries_verified = report.ledger_entries_verified,
        errors_count = report.errors.len(),
        "Reconciliation execution completed"
    );

    Ok(report)
}
