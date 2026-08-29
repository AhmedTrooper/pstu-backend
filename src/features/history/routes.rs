use crate::core::error::AppError;
use crate::core::state::AppState;
use crate::features::auth::middleware::AuthenticatedUser;
use crate::features::events::model::ProcessEventDto;
use crate::features::history::dto::{
    ActivityQuery, PaginatedTransactionsResponse, StatementQuery, TransactionHistoryQuery,
};
use crate::features::history::service;
use axum::{
    Json, Router,
    extract::{Query, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/me/transactions", get(get_transactions_handler))
        .route("/me/statement.csv", get(get_statement_csv_handler))
        .route("/me/activity", get(get_activity_handler))
}

async fn get_transactions_handler(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Query(params): Query<TransactionHistoryQuery>,
) -> Result<Json<PaginatedTransactionsResponse>, AppError> {
    let res = service::get_transactions(&state, auth_user.user_id, params).await?;
    Ok(Json(res))
}

async fn get_statement_csv_handler(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Query(params): Query<StatementQuery>,
) -> Result<Response, AppError> {
    let csv_content = service::generate_statement_csv(&state, auth_user.user_id, params).await?;

    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/csv; charset=utf-8"),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_static("attachment; filename=\"statement.csv\""),
    );

    Ok((StatusCode::OK, headers, csv_content).into_response())
}

async fn get_activity_handler(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Query(params): Query<ActivityQuery>,
) -> Result<Json<Vec<ProcessEventDto>>, AppError> {
    let events =
        service::get_user_activity(&state, auth_user.user_id, params.cursor, params.limit).await?;
    Ok(Json(events))
}
