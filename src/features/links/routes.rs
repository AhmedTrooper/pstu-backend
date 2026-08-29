use crate::core::error::AppError;
use crate::core::ratelimit::enforce_rate_limit;
use crate::core::state::AppState;
use crate::features::auth::middleware::AuthenticatedUser;
use crate::features::events::model::ProcessEventDto;
use crate::features::links::dto::{
    ClaimPaymentLinkRequest, ClaimPaymentLinkResponse, CreatePaymentLinkRequest, GetMyLinksQuery,
    PaginatedLinksResponse, PaymentLinkDto,
};
use crate::features::links::service;
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
        .route("/links", post(create_link_handler))
        .route("/me/links", get(get_my_links_handler))
        .route("/links/{token}", get(get_link_handler))
        .route("/links/{token}/claim", post(claim_link_handler))
        .route("/links/{token}/cancel", post(cancel_link_handler))
        .route("/links/{id}/events", get(get_link_events_handler))
}

async fn create_link_handler(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Json(payload): Json<CreatePaymentLinkRequest>,
) -> Result<(StatusCode, Json<PaymentLinkDto>), AppError> {
    // 20 link creations / minute per user (R13, T31)
    enforce_rate_limit(&state, "links", &auth_user.user_id.to_string(), 20, 60).await?;

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
            message: "Validation failed on payment link creation".to_string(),
            fields,
        });
    }

    let link = service::create_link(&state, auth_user.user_id, payload).await?;
    Ok((StatusCode::CREATED, Json(link)))
}

async fn get_my_links_handler(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Query(params): Query<GetMyLinksQuery>,
) -> Result<Json<PaginatedLinksResponse>, AppError> {
    let res = service::get_my_links(&state, auth_user.user_id, params).await?;
    Ok(Json(res))
}

async fn get_link_handler(
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> Result<Json<PaymentLinkDto>, AppError> {
    let link = service::get_link_by_token(&state, &token).await?;
    Ok(Json(link))
}

async fn claim_link_handler(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(token): Path<String>,
    Json(payload): Json<ClaimPaymentLinkRequest>,
) -> Result<(StatusCode, Json<ClaimPaymentLinkResponse>), AppError> {
    // 10 claim attempts / minute per user (R13, T31)
    enforce_rate_limit(&state, "claim_link", &auth_user.user_id.to_string(), 10, 60).await?;

    payload
        .validate()
        .map_err(|e| AppError::BadRequest(e.to_string()))?;
    let res = service::claim_link(&state, auth_user.user_id, &token, payload).await?;
    Ok((StatusCode::CREATED, Json(res)))
}

async fn cancel_link_handler(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(token): Path<String>,
) -> Result<Json<PaymentLinkDto>, AppError> {
    let res = service::cancel_link(&state, auth_user.user_id, &token).await?;
    Ok(Json(res))
}

async fn get_link_events_handler(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<ProcessEventDto>>, AppError> {
    let events = service::get_link_events(&state, auth_user.user_id, id).await?;
    Ok(Json(events))
}
