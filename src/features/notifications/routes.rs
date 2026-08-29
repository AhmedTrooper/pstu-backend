use crate::core::error::AppError;
use crate::core::state::AppState;
use crate::features::auth::middleware::AuthenticatedUser;
use crate::features::notifications::dto::{NotifQuery, NotifReadReq, NotificationsResponse, OkRes};
use crate::features::notifications::service;
use axum::{
    Json, Router,
    extract::{Query, State},
    routing::{get, post},
};
use validator::Validate;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/me/notifications", get(get_notifications_handler))
        .route("/me/notifications/read", post(mark_read_handler))
}

async fn get_notifications_handler(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Query(params): Query<NotifQuery>,
) -> Result<Json<NotificationsResponse>, AppError> {
    let res = service::get_notifications(&state, auth_user.user_id, params).await?;
    Ok(Json(res))
}

async fn mark_read_handler(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Json(payload): Json<NotifReadReq>,
) -> Result<Json<OkRes>, AppError> {
    payload
        .validate()
        .map_err(|e| AppError::BadRequest(e.to_string()))?;
    let res = service::mark_notifications_read(&state, auth_user.user_id, payload).await?;
    Ok(Json(res))
}
