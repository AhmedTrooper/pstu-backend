use crate::core::error::AppError;
use crate::core::state::AppState;
use crate::features::auth::dto::{
    LoginRequest, LoginResponse, RegisterRequest, RegisterResponse, UserDto,
};
use crate::features::events::service::log_event_txn;
use argon2::{
    Argon2,
    password_hash::{PasswordHasher, PasswordVerifier},
};
use password_hash::phc::PasswordHash;
use rand::RngExt;
use redis::AsyncCommands;
use sqlx::Row;
use uuid::Uuid;

// Initial balance seed: 10,000,000 paisa = ৳100,000 (§2, W1)
pub const INITIAL_SEED_PAISA: i64 = 10_000_000;
const MAX_ACCOUNT_NUMBER_RETRIES: usize = 3;

pub fn hash_password(password: &str) -> Result<String, AppError> {
    let argon2 = Argon2::default();
    argon2
        .hash_password(password.as_bytes())
        .map(|h| h.to_string())
        .map_err(|e| AppError::Internal(format!("Failed to hash password: {}", e)))
}

pub fn verify_password(password: &str, password_hash_str: &str) -> bool {
    if let Ok(parsed_hash) = PasswordHash::new(password_hash_str) {
        Argon2::default()
            .verify_password(password.as_bytes(), &parsed_hash)
            .is_ok()
    } else {
        false
    }
}

pub fn generate_account_number() -> String {
    let mut bytes = [0u8; 8];
    rand::rng().fill(&mut bytes);
    let val = u64::from_be_bytes(bytes) % 9_000_000_000 + 1_000_000_000;
    val.to_string()
}

pub fn generate_session_token() -> String {
    let mut bytes = [0u8; 32];
    rand::rng().fill(&mut bytes);
    hex::encode(bytes)
}

pub async fn register(
    state: &AppState,
    req: RegisterRequest,
) -> Result<RegisterResponse, AppError> {
    let password_hash = hash_password(&req.password)?;

    let mut attempt = 0;
    loop {
        attempt += 1;
        let account_number = generate_account_number();

        let mut tx = state.db.begin().await?;

        // 1. Check duplicate phone before insert
        let existing_phone: Option<Uuid> =
            sqlx::query_scalar("SELECT id FROM users WHERE phone = $1")
                .bind(&req.phone)
                .fetch_optional(&mut *tx)
                .await?;

        if existing_phone.is_some() {
            return Err(AppError::Conflict(
                "Phone number is already registered".to_string(),
            ));
        }

        // 2. Insert user
        let user_res = sqlx::query(
            r#"
            INSERT INTO users (name, phone, password_hash, account_number, status, created_at, updated_at)
            VALUES ($1, $2, $3, $4, 'active', now(), now())
            RETURNING id, account_number, name, phone, created_at
            "#,
        )
        .bind(&req.name)
        .bind(&req.phone)
        .bind(&password_hash)
        .bind(&account_number)
        .fetch_one(&mut *tx)
        .await;

        let row = match user_res {
            Ok(u) => u,
            Err(sqlx::Error::Database(db_err)) if db_err.is_unique_violation() => {
                if attempt <= MAX_ACCOUNT_NUMBER_RETRIES {
                    continue; // Retry with a new account number
                }
                return Err(AppError::Conflict(
                    "Registration conflict, please retry".to_string(),
                ));
            }
            Err(e) => return Err(AppError::Database(e)),
        };

        let user_id: Uuid = row.get("id");
        let user_account_num: String = row.get("account_number");
        let user_name: String = row.get("name");
        let user_phone: String = row.get("phone");
        let user_created_at: chrono::DateTime<chrono::Utc> = row.get("created_at");

        // 3. Insert initial balance (10,000,000 paisa) in same txn (§2, W1)
        sqlx::query(
            r#"
            INSERT INTO balances (user_id, amount_paisa, version, updated_at)
            VALUES ($1, $2, 0, now())
            "#,
        )
        .bind(user_id)
        .bind(INITIAL_SEED_PAISA)
        .execute(&mut *tx)
        .await?;

        // 4. Record initial ledger row (R3)
        let txn_id = Uuid::new_v4();
        sqlx::query(
            r#"
            INSERT INTO ledger (txn_id, user_id, counterparty_id, direction, amount_paisa, running_balance, kind, created_at)
            VALUES ($1, $2, null, 1, $3, $3, 'funding', now())
            "#,
        )
        .bind(txn_id)
        .bind(user_id)
        .bind(INITIAL_SEED_PAISA)
        .execute(&mut *tx)
        .await?;

        // 5. Record process event (W1, W9)
        log_event_txn(
            &mut tx,
            "auth",
            user_id,
            "registered",
            Some(user_id),
            "User registered with initial seed balance",
            serde_json::json!({ "phone": user_phone, "account_number": user_account_num }),
        )
        .await?;

        tx.commit().await?;

        return Ok(RegisterResponse {
            id: user_id,
            account_number: user_account_num,
            name: user_name,
            phone: user_phone,
            balance_paisa: INITIAL_SEED_PAISA.to_string(),
            created_at: user_created_at,
        });
    }
}

pub async fn login(
    state: &AppState,
    req: LoginRequest,
) -> Result<(LoginResponse, String), AppError> {
    let mut redis_conn = state.redis.clone();
    let lockout_key = format!("lockout:{}", req.phone);

    // Check brute-force lockout (5 fails / 15 min -> 429, R15, C40)
    let fail_count: Option<i32> = redis_conn.get(&lockout_key).await.unwrap_or(None);
    if fail_count.unwrap_or(0) >= 5 {
        return Err(AppError::RateLimited(
            "Too many failed login attempts. Please try again after 15 minutes.".to_string(),
        ));
    }

    let user_row = sqlx::query(
        r#"
        SELECT id, account_number, name, phone, password_hash, status, created_at
        FROM users
        WHERE phone = $1
        "#,
    )
    .bind(&req.phone)
    .fetch_optional(&state.db)
    .await?;

    let is_valid = match &user_row {
        Some(row) => {
            let pass_hash: String = row.get("password_hash");
            verify_password(&req.password, &pass_hash)
        }
        None => false,
    };

    if !is_valid {
        // Record failed attempt in Redis
        let _: Result<(), _> = redis_conn.incr(&lockout_key, 1).await;
        let _: Result<(), _> = redis_conn.expire(&lockout_key, 900).await;

        if let Some(row) = &user_row {
            let user_id: Uuid = row.get("id");
            let mut conn = state.db.acquire().await?;
            let _ = crate::features::events::service::log_event_conn(
                &mut conn,
                "auth",
                user_id,
                "login_failed",
                Some(user_id),
                "Invalid password attempt",
                serde_json::json!({ "phone": req.phone }),
            )
            .await;
        }

        // Generic 401 (no account existence leak, R15)
        return Err(AppError::Unauthorized(
            "Invalid phone or password".to_string(),
        ));
    }

    let row = user_row.unwrap();
    let user_id: Uuid = row.get("id");
    let user_account_num: String = row.get("account_number");
    let user_name: String = row.get("name");
    let user_phone: String = row.get("phone");
    let user_created_at: chrono::DateTime<chrono::Utc> = row.get("created_at");

    // Clear lockout counter on success
    let _: Result<(), _> = redis_conn.del(&lockout_key).await;

    // Fetch user balance
    let balance_paisa: i64 =
        sqlx::query_scalar("SELECT amount_paisa FROM balances WHERE user_id = $1")
            .bind(user_id)
            .fetch_one(&state.db)
            .await?;

    // Create session in Redis (64-hex token, 24h TTL)
    let session_token = generate_session_token();
    let session_key = format!("session:{}", session_token);
    let ttl_secs = state.config.session_ttl_secs as i64;
    let _: () = redis_conn
        .set_ex(&session_key, user_id.to_string(), ttl_secs as u64)
        .await
        .map_err(|_| {
            AppError::ServiceUnavailable("Session storage temporarily unavailable".to_string())
        })?;

    // Log login_success process event
    let mut conn = state.db.acquire().await?;
    let _ = crate::features::events::service::log_event_conn(
        &mut conn,
        "auth",
        user_id,
        "login_success",
        Some(user_id),
        "Successful session login",
        serde_json::json!({ "phone": user_phone }),
    )
    .await;

    let response = LoginResponse {
        user: UserDto {
            id: user_id,
            account_number: user_account_num,
            name: user_name,
            phone: user_phone,
            created_at: user_created_at,
        },
        balance_paisa: balance_paisa.to_string(),
    };

    Ok((response, session_token))
}

pub async fn logout(state: &AppState, user_id: Uuid, session_token: &str) -> Result<(), AppError> {
    let mut redis_conn = state.redis.clone();
    let session_key = format!("session:{}", session_token);
    let _: Result<(), _> = redis_conn.del(&session_key).await;

    let mut conn = state.db.acquire().await?;
    let _ = crate::features::events::service::log_event_conn(
        &mut conn,
        "auth",
        user_id,
        "logout",
        Some(user_id),
        "User logged out",
        serde_json::json!({}),
    )
    .await;

    Ok(())
}
