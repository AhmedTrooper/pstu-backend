use crate::core::error::AppError;
use crate::core::ratelimit::enforce_rate_limit;
use crate::core::state::AppState;
use crate::features::auth::middleware::AuthenticatedUser;
use crate::features::events::model::ProcessEventDto;
use crate::features::transfers::dto::{CreateTransferRequest, TransferDetailResponse};
use crate::features::transfers::service;
use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use std::collections::HashMap;
use uuid::Uuid;
use validator::Validate;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/transfers", post(create_transfer_handler))
        .route("/transfers/{id}", get(get_transfer_handler))
        .route("/transfers/{id}/events", get(get_transfer_events_handler))
}

async fn create_transfer_handler(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Json(payload): Json<CreateTransferRequest>,
) -> Result<Response, AppError> {
    // Enforce 30 transfers/minute rate limit per user (R13, T31)
    enforce_rate_limit(&state, "transfers", &auth_user.user_id.to_string(), 30, 60).await?;

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
            message: "Validation failed on transfer request".to_string(),
            fields,
        });
    }

    let (transfer, is_replay) =
        service::process_transfer(&state, auth_user.user_id, payload).await?;

    let status_code = if is_replay {
        StatusCode::OK
    } else {
        StatusCode::CREATED
    };

    Ok((status_code, Json(transfer)).into_response())
}

async fn get_transfer_handler(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(id): Path<Uuid>,
) -> Result<Json<TransferDetailResponse>, AppError> {
    let detail = service::get_transfer_detail(&state, auth_user.user_id, id).await?;
    Ok(Json(detail))
}

async fn get_transfer_events_handler(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<ProcessEventDto>>, AppError> {
    let events = service::get_transfer_events(&state, auth_user.user_id, id).await?;
    Ok(Json(events))
}
