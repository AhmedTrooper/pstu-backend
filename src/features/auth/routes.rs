use crate::core::error::AppError;
use crate::core::state::AppState;
use crate::features::auth::dto::{LoginRequest, LogoutResponse, RegisterRequest, RegisterResponse};
use crate::features::auth::middleware::{AuthenticatedUser, SESSION_COOKIE_NAME};
use crate::features::auth::service;
use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::post,
};
use std::collections::HashMap;
use tower_cookies::{Cookie, Cookies};
use validator::Validate;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/auth/register", post(register_handler))
        .route("/auth/login", post(login_handler))
        .route("/auth/logout", post(logout_handler))
}

async fn register_handler(
    State(state): State<AppState>,
    Json(payload): Json<RegisterRequest>,
) -> Result<(StatusCode, Json<RegisterResponse>), AppError> {
    if let Err(val_err) = payload.validate() {
        let mut fields = HashMap::new();
        for (field, errors) in val_err.field_errors() {
            if let Some(err) = errors.first() {
                let msg = err
                    .message
                    .as_ref()
                    .map(|m| m.to_string())
                    .unwrap_or_else(|| "invalid value".to_string());
                fields.insert(field.to_string(), msg);
            }
        }
        return Err(AppError::Validation {
            message: "Validation failed on registration fields".to_string(),
            fields,
        });
    }

    let res = service::register(&state, payload).await?;
    Ok((StatusCode::CREATED, Json(res)))
}

async fn login_handler(
    State(state): State<AppState>,
    cookies: Cookies,
    Json(payload): Json<LoginRequest>,
) -> Result<Response, AppError> {
    if let Err(val_err) = payload.validate() {
        let mut fields = HashMap::new();
        for (field, errors) in val_err.field_errors() {
            if let Some(err) = errors.first() {
                let msg = err
                    .message
                    .as_ref()
                    .map(|m| m.to_string())
                    .unwrap_or_else(|| "invalid value".to_string());
                fields.insert(field.to_string(), msg);
            }
        }
        return Err(AppError::Validation {
            message: "Validation failed on login fields".to_string(),
            fields,
        });
    }

    let (res, session_token) = service::login(&state, payload).await?;

    let mut cookie = Cookie::new(SESSION_COOKIE_NAME, session_token);
    cookie.set_path("/");
    cookie.set_http_only(true);
    cookie.set_same_site(tower_cookies::cookie::SameSite::Lax);
    cookie.set_max_age(tower_cookies::cookie::time::Duration::seconds(
        state.config.session_ttl_secs as i64,
    ));
    cookies.add(cookie);

    Ok((StatusCode::OK, Json(res)).into_response())
}

async fn logout_handler(
    State(state): State<AppState>,
    cookies: Cookies,
    auth_user: AuthenticatedUser,
) -> Result<Json<LogoutResponse>, AppError> {
    service::logout(&state, auth_user.user_id, &auth_user.session_token).await?;

    let mut cookie = Cookie::new(SESSION_COOKIE_NAME, "");
    cookie.set_path("/");
    cookie.set_http_only(true);
    cookie.set_max_age(tower_cookies::cookie::time::Duration::seconds(0));
    cookies.add(cookie);

    Ok(Json(LogoutResponse { ok: true }))
}
