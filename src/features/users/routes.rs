use crate::core::error::AppError;
use crate::core::state::AppState;
use crate::features::auth::middleware::AuthenticatedUser;
use crate::features::users::dto::{LookupQuery, PublicUserRes, UserLookupDto, UserProfileResponse};
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    routing::get,
};
use sqlx::Row;
use uuid::Uuid;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/me", get(get_me_handler))
        .route("/users/lookup", get(lookup_users_handler))
        .route(
            "/public/users/{account_number}",
            get(get_public_user_handler),
        )
}

async fn get_me_handler(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
) -> Result<Json<UserProfileResponse>, AppError> {
    let row = sqlx::query(
        r#"
        SELECT u.id, u.name, u.phone, u.account_number, u.created_at, b.amount_paisa
        FROM users u
        JOIN balances b ON b.user_id = u.id
        WHERE u.id = $1
        "#,
    )
    .bind(auth_user.user_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("User not found".to_string()))?;

    let id: Uuid = row.get("id");
    let name: String = row.get("name");
    let phone: String = row.get("phone");
    let account_number: String = row.get("account_number");
    let created_at: chrono::DateTime<chrono::Utc> = row.get("created_at");
    let amount_paisa: i64 = row.get("amount_paisa");

    Ok(Json(UserProfileResponse {
        id,
        name,
        phone,
        account_number,
        balance: amount_paisa.to_string(),
        created_at,
    }))
}

async fn lookup_users_handler(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Query(params): Query<LookupQuery>,
) -> Result<Json<Vec<UserLookupDto>>, AppError> {
    let q = params.q.unwrap_or_default().trim().to_string();
    if q.is_empty() {
        return Ok(Json(Vec::new()));
    }

    // Lookup using exact phone/account_number or trigram gin index on name (§14)
    let rows = sqlx::query(
        r#"
        SELECT id, name, account_number, phone
        FROM users
        WHERE phone = $1 
           OR account_number = $1 
           OR name ILIKE '%' || $1 || '%'
        ORDER BY (phone = $1 OR account_number = $1) DESC, similarity(name, $1) DESC
        LIMIT 10
        "#,
    )
    .bind(&q)
    .fetch_all(&state.db)
    .await?;

    let users = rows
        .into_iter()
        .map(|r| UserLookupDto {
            id: r.get("id"),
            name: r.get("name"),
            account_number: r.get("account_number"),
            phone: r.get("phone"),
        })
        .collect();

    Ok(Json(users))
}

async fn get_public_user_handler(
    State(state): State<AppState>,
    Path(account_number): Path<String>,
) -> Result<Json<PublicUserRes>, AppError> {
    let clean_acc = account_number.trim();
    let row = sqlx::query("SELECT name, account_number FROM users WHERE account_number = $1")
        .bind(clean_acc)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("User not found".to_string()))?;

    Ok(Json(PublicUserRes {
        name: row.get("name"),
        account_number: row.get("account_number"),
    }))
}
