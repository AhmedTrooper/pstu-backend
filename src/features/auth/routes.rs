use crate::core::error::AppError;
use crate::core::state::AppState;
use crate::features::auth::dto::{
    LoginRequest, LoginResponse, LogoutResponse, PinChangeReq, PinResetReq, PinUpdatedRes,
    RegisterRequest, RegisterResponse,
};
use crate::features::auth::middleware::AuthenticatedUser;
use crate::features::auth::service;
use axum::{Json, Router, extract::State, http::StatusCode, routing::post};
use std::collections::HashMap;
use tower_cookies::Cookies;
use validator::Validate;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/auth/register", post(register_handler))
        .route("/auth/login", post(login_handler))
        .route("/auth/logout", post(logout_handler))
        .route("/me/pin/change", post(change_pin_handler))
        .route("/me/pin/reset", post(reset_pin_handler))
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
            message: "Validation failed on user registration".to_string(),
            fields,
        });
    }

    let res = service::register_user(&state, payload).await?;
    Ok((StatusCode::CREATED, Json(res)))
}

async fn login_handler(
    State(state): State<AppState>,
    cookies: Cookies,
    Json(payload): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, AppError> {
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
            message: "Validation failed on login".to_string(),
            fields,
        });
    }

    let res = service::login_user(&state, payload, &cookies).await?;
    Ok(Json(res))
}

async fn logout_handler(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    cookies: Cookies,
) -> Result<Json<LogoutResponse>, AppError> {
    let res = service::logout_user(&state, auth_user.user_id, &cookies).await?;
    Ok(Json(res))
}

async fn change_pin_handler(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Json(payload): Json<PinChangeReq>,
) -> Result<Json<PinUpdatedRes>, AppError> {
    payload
        .validate()
        .map_err(|e| AppError::BadRequest(e.to_string()))?;
    let res = service::change_pin(&state, auth_user.user_id, payload).await?;
    Ok(Json(res))
}

async fn reset_pin_handler(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    cookies: Cookies,
    Json(payload): Json<PinResetReq>,
) -> Result<Json<PinUpdatedRes>, AppError> {
    payload
        .validate()
        .map_err(|e| AppError::BadRequest(e.to_string()))?;
    let res = service::reset_pin(&state, auth_user.user_id, payload, &cookies).await?;
    Ok(Json(res))
}
