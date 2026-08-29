use api::build_app;
use api::core::config::AppConfig;
use api::core::state::AppState;
use api::core::telemetry::init_telemetry;
use sqlx::postgres::PgPoolOptions;
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tokio::signal;
use tracing::{error, info};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let metrics_handle = init_telemetry();

    info!("Starting PSTU Payment Gateway API...");

    let config = AppConfig::load_from_env().map_err(|e| {
        Box::<dyn std::error::Error>::from(format!("Configuration initialization failed: {}", e))
    })?;

    info!(
        host = %config.host,
        port = %config.port,
        "Configuration loaded successfully"
    );

    // Initialize PostgreSQL connection pool with retry resilience
    let mut db_pool_opt = None;
    for attempt in 1..=15 {
        match PgPoolOptions::new()
            .max_connections(50)
            .acquire_timeout(std::time::Duration::from_secs(3))
            .connect(&config.database_url)
            .await
        {
            Ok(pool) => {
                db_pool_opt = Some(pool);
                break;
            }
            Err(e) => {
                if attempt == 15 {
                    error!(error = ?e, "Failed to connect to PostgreSQL after multiple attempts");
                    return Err(Box::new(e) as Box<dyn std::error::Error>);
                }
                info!(
                    "Waiting for PostgreSQL to become ready (attempt {}/15)...",
                    attempt
                );
                tokio::time::sleep(std::time::Duration::from_millis(800)).await;
            }
        }
    }
    let db_pool = db_pool_opt.unwrap();

    info!("Connected to PostgreSQL database");

    // Run database migrations
    info!("Running database migrations...");
    sqlx::migrate!("./migrations")
        .run(&db_pool)
        .await
        .map_err(|e| {
            error!(error = ?e, "Failed to execute database migrations");
            Box::new(e) as Box<dyn std::error::Error>
        })?;
    info!("Database migrations executed successfully");

    // Initialize Redis connection manager with retry resilience
    let redis_client = redis::Client::open(config.redis_url.clone()).map_err(|e| {
        error!(error = ?e, "Failed to open Redis client");
        Box::new(e) as Box<dyn std::error::Error>
    })?;

    let mut redis_conn_opt = None;
    for attempt in 1..=15 {
        match redis::aio::ConnectionManager::new(redis_client.clone()).await {
            Ok(conn) => {
                redis_conn_opt = Some(conn);
                break;
            }
            Err(e) => {
                if attempt == 15 {
                    error!(error = ?e, "Failed to establish Redis connection manager after multiple attempts");
                    return Err(Box::<dyn std::error::Error>::from(e.to_string()));
                }
                info!(
                    "Waiting for Redis to become ready (attempt {}/15)...",
                    attempt
                );
                tokio::time::sleep(std::time::Duration::from_millis(800)).await;
            }
        }
    }
    let redis_conn = redis_conn_opt.unwrap();
    info!("Connected to Redis instance");

    // Initialize optional NATS connection
    let nats_client = match async_nats::connect(&config.nats_url).await {
        Ok(client) => {
            info!("Connected to NATS event bus at {}", config.nats_url);
            Some(client)
        }
        Err(err) => {
            tracing::warn!(error = ?err, "NATS not reachable, continuing in standalone mode");
            None
        }
    };

    let state = AppState::new(
        db_pool,
        redis_conn,
        nats_client,
        metrics_handle,
        config.clone(),
    );

    let app = build_app(state);

    let addr: SocketAddr = format!("{}:{}", config.host, config.port).parse()?;
    info!("Listening on http://{}", addr);

    let listener = TcpListener::bind(addr).await?;
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    info!("Server shutdown completed cleanly");
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}
