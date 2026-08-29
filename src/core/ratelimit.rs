use crate::core::error::AppError;
use crate::core::state::AppState;
use chrono::Utc;
use tracing::warn;
use uuid::Uuid;

pub async fn check_sliding_window_rate_limit(
    state: &AppState,
    scope: &str,
    identifier: &str,
    max_requests: i64,
    window_seconds: u64,
) -> Result<bool, AppError> {
    let key = format!("rl:{}:{}", scope, identifier);
    let now_ms = Utc::now().timestamp_millis();
    let window_start_ms = now_ms - (window_seconds as i64 * 1000);
    let member = format!("{}:{}", now_ms, Uuid::new_v4());

    let mut redis_conn = state.redis.clone();

    // Use sorted sets (ZSET) for precise sliding-window rate limiting
    // 1. Remove timestamps older than window start
    // 2. Count remaining members in window
    let res: Result<(i64, i64), redis::RedisError> = redis::pipe()
        .atomic()
        .zrembyscore(&key, 0, window_start_ms)
        .zcard(&key)
        .query_async(&mut redis_conn)
        .await;

    match res {
        Ok((_, current_count)) => {
            if current_count >= max_requests {
                // Rate limit exceeded
                return Ok(false);
            }

            // Add current request to the window and refresh TTL
            let _: Result<(), _> = redis::pipe()
                .atomic()
                .zadd(&key, member, now_ms)
                .expire(&key, (window_seconds + 5) as i64)
                .query_async(&mut redis_conn)
                .await;

            Ok(true)
        }
        Err(e) => {
            // Fail open on Redis connectivity issues with a warning (§1 Resilience)
            warn!(error = ?e, "Redis rate limiting unavailable, failing open gracefully");
            Ok(true)
        }
    }
}

pub async fn enforce_rate_limit(
    state: &AppState,
    scope: &str,
    identifier: &str,
    max_requests: i64,
    window_seconds: u64,
) -> Result<(), AppError> {
    let allowed =
        check_sliding_window_rate_limit(state, scope, identifier, max_requests, window_seconds)
            .await?;
    if !allowed {
        return Err(AppError::RateLimited(format!(
            "Rate limit exceeded for '{}'. Maximum {} requests per {} seconds.",
            scope, max_requests, window_seconds
        )));
    }
    Ok(())
}
