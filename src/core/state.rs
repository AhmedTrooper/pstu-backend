use crate::core::config::AppConfig;
use chrono::{DateTime, Utc};
use metrics_exporter_prometheus::PrometheusHandle;
use redis::aio::ConnectionManager;
use sqlx::PgPool;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub redis: ConnectionManager,
    pub nats: Option<async_nats::Client>,
    pub metrics_handle: PrometheusHandle,
    pub config: Arc<AppConfig>,
    pub server_started_at: DateTime<Utc>,
}

impl AppState {
    pub fn new(
        db: PgPool,
        redis: ConnectionManager,
        nats: Option<async_nats::Client>,
        metrics_handle: PrometheusHandle,
        config: AppConfig,
    ) -> Self {
        Self {
            db,
            redis,
            nats,
            metrics_handle,
            config: Arc::new(config),
            server_started_at: Utc::now(),
        }
    }
}
