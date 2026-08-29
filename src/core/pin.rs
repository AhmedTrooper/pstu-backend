use crate::core::error::AppError;
use crate::core::state::AppState;
use crate::features::events::service::log_event;
use argon2::{
    Argon2,
    password_hash::{PasswordHasher, PasswordVerifier},
};
use password_hash::phc::PasswordHash;
use redis::AsyncCommands;
use uuid::Uuid;

pub const MAX_PIN_FAILURES: i64 = 5;
pub const PIN_LOCKOUT_SECONDS: u64 = 900; // 15 minutes

pub fn hash_pin(pin: &str) -> Result<String, AppError> {
    let argon2 = Argon2::default();
    let hash = argon2
        .hash_password(pin.as_bytes())
        .map_err(|e| AppError::Internal(format!("Failed to hash PIN: {}", e)))?
        .to_string();
    Ok(hash)
}

pub fn verify_pin_hash(pin: &str, pin_hash_str: &str) -> Result<bool, AppError> {
    let parsed_hash = PasswordHash::new(pin_hash_str)
        .map_err(|e| AppError::Internal(format!("Invalid PIN hash format: {}", e)))?;
    let argon2 = Argon2::default();
    Ok(argon2.verify_password(pin.as_bytes(), &parsed_hash).is_ok())
}

pub async fn enforce_user_pin(
    state: &AppState,
    user_id: Uuid,
    provided_pin: &str,
) -> Result<(), AppError> {
    let lockout_key = format!("pf:{}", user_id);
    let mut redis_conn = state.redis.clone();

    // Check PIN lockout counter (R17, C47)
    let failures: i64 = redis_conn.get(&lockout_key).await.unwrap_or(0);
    if failures >= MAX_PIN_FAILURES {
        return Err(AppError::RateLimited(
            "Transaction PIN is locked due to multiple failed attempts. Please try again in 15 minutes."
                .to_string(),
        ));
    }

    // Fetch user pin hash
    let pin_hash: Option<String> = sqlx::query_scalar("SELECT pin_hash FROM users WHERE id = $1")
        .bind(user_id)
        .fetch_optional(&state.db)
        .await?;

    let pin_hash = pin_hash.ok_or_else(|| AppError::NotFound("User not found".to_string()))?;

    let is_valid = verify_pin_hash(provided_pin, &pin_hash)?;

    if !is_valid {
        let new_failures: i64 = redis_conn.incr(&lockout_key, 1).await.unwrap_or(1);
        let _: Result<(), _> = redis_conn
            .expire(&lockout_key, PIN_LOCKOUT_SECONDS as i64)
            .await;

        let _ = log_event(
            state,
            "auth",
            user_id,
            "pin_failed",
            Some(user_id),
            "Incorrect transaction PIN entered",
            serde_json::json!({ "failure_count": new_failures }),
        )
        .await;

        if new_failures >= MAX_PIN_FAILURES {
            return Err(AppError::RateLimited(
                "Transaction PIN has been locked after 5 failed attempts. Please try again in 15 minutes."
                    .to_string(),
            ));
        }

        return Err(AppError::Unprocessable(
            "Transaction PIN is incorrect".to_string(),
        ));
    }

    // Reset failure counter on success
    let _: Result<(), _> = redis_conn.del(&lockout_key).await;
    Ok(())
}
