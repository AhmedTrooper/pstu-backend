use crate::core::error::AppError;
use crate::core::state::AppState;
use sqlx::Row;
use std::str::FromStr;
use uuid::Uuid;

pub fn normalize_phone(raw: &str) -> String {
    let trimmed = raw.trim();
    let digits_only: String = trimmed.chars().filter(|c| c.is_ascii_digit()).collect();

    if digits_only.starts_with("01") && digits_only.len() == 11 {
        format!("+88{}", digits_only)
    } else if digits_only.starts_with("8801") && digits_only.len() == 13 {
        format!("+{}", digits_only)
    } else {
        trimmed.to_string()
    }
}

pub async fn resolve_recipient_user(state: &AppState, identifier: &str) -> Result<Uuid, AppError> {
    let trimmed = identifier.trim();

    // 1. Try UUID parse
    if let Ok(id) = Uuid::from_str(trimmed) {
        let row = sqlx::query("SELECT id, status FROM users WHERE id = $1")
            .bind(id)
            .fetch_optional(&state.db)
            .await?;

        if let Some(r) = row {
            let status: String = r.get("status");
            if status != "active" {
                return Err(AppError::Unprocessable(
                    "Counterparty account is inactive or suspended".to_string(),
                ));
            }
            return Ok(r.get("id"));
        }
    }

    let normalized_phone = normalize_phone(trimmed);

    // 2. Exact account_number or normalized phone lookup (R20)
    let row = sqlx::query(
        r#"
        SELECT id, status FROM users 
        WHERE account_number = $1
           OR phone = $1
           OR phone = $2
        ORDER BY (account_number = $1) DESC, (phone = $2) DESC
        LIMIT 1
        "#,
    )
    .bind(trimmed)
    .bind(&normalized_phone)
    .fetch_optional(&state.db)
    .await?;

    if let Some(r) = row {
        let status: String = r.get("status");
        if status != "active" {
            return Err(AppError::Unprocessable(
                "Counterparty account is inactive or suspended".to_string(),
            ));
        }
        return Ok(r.get("id"));
    }

    Err(AppError::NotFound("Recipient not found".to_string()))
}
