use crate::core::state::AppState;
use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
};
use serde::Serialize;

#[derive(Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
    pub postgres: &'static str,
    pub redis: &'static str,
    pub server_started_at: chrono::DateTime<chrono::Utc>,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/health", get(health_check))
        .route("/health/live", get(liveness_check))
        .route("/health/ready", get(readiness_check))
        .route("/metrics", get(metrics_handler))
}

async fn health_check(State(state): State<AppState>) -> Response {
    let mut pg_ok = true;
    let mut redis_ok = true;

    // Check Postgres
    if sqlx::query("SELECT 1").execute(&state.db).await.is_err() {
        pg_ok = false;
    }

    // Check Redis
    let mut redis_conn = state.redis.clone();
    if redis::cmd("PING")
        .query_async::<String>(&mut redis_conn)
        .await
        .is_err()
    {
        redis_ok = false;
    }

    let status = if pg_ok && redis_ok { "ok" } else { "degraded" };
    let status_code = if pg_ok && redis_ok {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    let body = HealthResponse {
        status,
        postgres: if pg_ok { "ok" } else { "down" },
        redis: if redis_ok { "ok" } else { "down" },
        server_started_at: state.server_started_at,
    };

    (status_code, Json(body)).into_response()
}

async fn liveness_check() -> StatusCode {
    StatusCode::OK
}

async fn readiness_check(State(state): State<AppState>) -> StatusCode {
    let pg_res = sqlx::query("SELECT 1").execute(&state.db).await;
    let mut redis_conn = state.redis.clone();
    let redis_res = redis::cmd("PING")
        .query_async::<String>(&mut redis_conn)
        .await;

    if pg_res.is_ok() && redis_res.is_ok() {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    }
}

async fn metrics_handler(State(state): State<AppState>) -> String {
    state.metrics_handle.render()
}
