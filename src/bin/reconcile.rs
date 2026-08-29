use api::core::config::AppConfig;
use api::features::reconcile::service::run_reconciliation;
use sqlx::postgres::PgPoolOptions;
use std::process::exit;

#[tokio::main]
async fn main() {
    let _ = dotenvy::dotenv();
    let config = AppConfig::load_from_env().expect("failed to load configuration");

    println!("============================================================");
    println!("  PSTU MONEY MOVEMENT ENGINE — RECONCILIATION AUDIT (T30)");
    println!("============================================================");
    println!("Connecting to database: {}", config.database_url);

    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&config.database_url)
        .await
        .expect("failed to connect to PostgreSQL database");

    println!("Running 3-contract verification audit on live ledger...");
    match run_reconciliation(&pool).await {
        Ok(report) => {
            println!("\n--- Audit Summary ---");
            println!("  Total Users Registered:      {}", report.users_count);
            println!(
                "  Users Reconciled (Contract 1): {}",
                report.users_reconciled
            );
            println!(
                "  Ledger Entries Verified:     {}",
                report.ledger_entries_verified
            );
            println!(
                "  Total System Balance:        {} paisa",
                report.total_system_balance_paisa
            );
            println!(
                "  Expected System Balance:     {} paisa",
                report.expected_system_balance_paisa
            );

            if report.errors.is_empty() {
                println!("\n>>> RECONCILE OK: All 3 Double-Entry Invariants Satisfied! <<<\n");
                exit(0);
            } else {
                eprintln!(
                    "\n>>> RECONCILE FAILED: Found {} Invariant Violations! <<<",
                    report.errors.len()
                );
                for (idx, err) in report.errors.iter().enumerate() {
                    eprintln!("  [{}] {}", idx + 1, err);
                }
                exit(1);
            }
        }
        Err(e) => {
            eprintln!("Reconciliation error: {}", e);
            exit(1);
        }
    }
}
