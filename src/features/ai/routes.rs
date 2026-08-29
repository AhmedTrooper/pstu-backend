use crate::core::error::AppError;
use crate::core::ratelimit::enforce_rate_limit;
use crate::core::state::AppState;
use crate::features::ai::dto::{AIParseRequest, AIParseResponse};
use crate::features::ai::parser;
use crate::features::auth::middleware::AuthenticatedUser;
use axum::{Json, Router, extract::State, routing::post};
use validator::Validate;

pub fn router() -> Router<AppState> {
    Router::new().route("/ai/parse", post(parse_ai_intent_handler))
}

async fn parse_ai_intent_handler(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Json(payload): Json<AIParseRequest>,
) -> Result<Json<AIParseResponse>, AppError> {
    // 30 AI parse requests / minute per user (R13, T31)
    enforce_rate_limit(&state, "ai_parse", &auth_user.user_id.to_string(), 30, 60).await?;

    payload
        .validate()
        .map_err(|e| AppError::BadRequest(e.to_string()))?;
    let resp = parser::parse_intent(&payload.text);
    Ok(Json(resp))
}
