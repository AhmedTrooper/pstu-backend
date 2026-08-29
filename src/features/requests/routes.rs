use crate::core::envelope::ApiResponse;
use crate::core::error::AppError;
use crate::core::state::AppState;
use crate::features::auth::middleware::AuthenticatedUser;
use crate::features::events::model::ProcessEventDto;
use crate::features::requests::dto::{
    AcceptMoneyRequest, AcceptRequestResponse, CreateMoneyRequest, GetRequestsQuery,
    MoneyRequestDto, PaginatedRequestsResponse,
};
use crate::features::requests::service;
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, post},
};
use std::collections::HashMap;
use uuid::Uuid;
use validator::Validate;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/requests",
            post(create_request_handler).get(get_requests_handler),
        )
        .route("/requests/{id}/accept", post(accept_request_handler))
        .route("/requests/{id}/reject", post(reject_request_handler))
        .route("/requests/{id}/cancel", post(cancel_request_handler))
        .route("/requests/{id}/events", get(get_request_events_handler))
}

async fn create_request_handler(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Json(payload): Json<CreateMoneyRequest>,
) -> Result<(StatusCode, Json<ApiResponse<MoneyRequestDto>>), AppError> {
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
            message: "Validation failed on money request".to_string(),
            fields,
        });
    }

    let req = service::create_request(&state, auth_user.user_id, payload).await?;
    Ok((StatusCode::CREATED, Json(ApiResponse::new(req))))
}

async fn get_requests_handler(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Query(params): Query<GetRequestsQuery>,
) -> Result<Json<ApiResponse<PaginatedRequestsResponse>>, AppError> {
    let res = service::get_requests(&state, auth_user.user_id, params).await?;
    Ok(Json(ApiResponse::new(res)))
}

async fn accept_request_handler(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(id): Path<Uuid>,
    Json(payload): Json<AcceptMoneyRequest>,
) -> Result<Json<ApiResponse<AcceptRequestResponse>>, AppError> {
    payload
        .validate()
        .map_err(|e| AppError::BadRequest(e.to_string()))?;
    let res = service::accept_request(&state, auth_user.user_id, id, payload).await?;
    Ok(Json(ApiResponse::new(res)))
}

async fn reject_request_handler(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(id): Path<Uuid>,
) -> Result<Json<ApiResponse<MoneyRequestDto>>, AppError> {
    let res = service::reject_request(&state, auth_user.user_id, id).await?;
    Ok(Json(ApiResponse::new(res)))
}

async fn cancel_request_handler(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(id): Path<Uuid>,
) -> Result<Json<ApiResponse<MoneyRequestDto>>, AppError> {
    let res = service::cancel_request(&state, auth_user.user_id, id).await?;
    Ok(Json(ApiResponse::new(res)))
}

async fn get_request_events_handler(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(id): Path<Uuid>,
) -> Result<Json<ApiResponse<Vec<ProcessEventDto>>>, AppError> {
    let events = service::get_request_events(&state, auth_user.user_id, id).await?;
    Ok(Json(ApiResponse::new(events)))
}
