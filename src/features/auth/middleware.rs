use crate::core::error::AppError;
use crate::core::state::AppState;
use axum::{
    extract::{FromRef, FromRequestParts},
    http::request::Parts,
};
use redis::AsyncCommands;
use std::str::FromStr;
use tower_cookies::Cookies;
use uuid::Uuid;

pub const SESSION_COOKIE_NAME: &str = "pstu_session";

#[derive(Debug, Clone)]
pub struct AuthenticatedUser {
    pub user_id: Uuid,
    pub session_token: String,
}

impl<S> FromRequestParts<S> for AuthenticatedUser
where
    AppState: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let app_state = AppState::from_ref(state);

        // 1. Try extracting from Authorization header
        let bearer_token = parts
            .headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|h| h.to_str().ok())
            .and_then(|h| {
                h.strip_prefix("Bearer ")
                    .map(|token| token.trim().to_string())
            });

        // 2. Try extracting from Cookies extension
        let cookie_token = parts
            .extensions
            .get::<Cookies>()
            .and_then(|cookies| cookies.get(SESSION_COOKIE_NAME))
            .map(|c| c.value().to_string());

        let token = bearer_token
            .or(cookie_token)
            .ok_or_else(|| AppError::Unauthorized("Authentication required".to_string()))?;

        if token.trim().is_empty() {
            return Err(AppError::Unauthorized(
                "Authentication required".to_string(),
            ));
        }

        // 3. Validate against Redis session store
        let mut redis_conn = app_state.redis.clone();
        let session_key = format!("session:{}", token);

        let user_id_str: Option<String> = redis_conn.get(&session_key).await.map_err(|_| {
            AppError::ServiceUnavailable("Session cache temporarily unavailable".to_string())
        })?;

        let user_id_str = user_id_str
            .ok_or_else(|| AppError::Unauthorized("Session expired or invalid".to_string()))?;

        let user_id = Uuid::from_str(&user_id_str)
            .map_err(|_| AppError::Unauthorized("Malformed session state".to_string()))?;

        Ok(AuthenticatedUser {
            user_id,
            session_token: token,
        })
    }
}
