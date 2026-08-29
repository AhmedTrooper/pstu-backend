use crate::core::error::AppError;
use crate::core::pin::{enforce_user_pin, hash_pin};
use crate::core::reference::generate_funding_reference;
use crate::core::state::AppState;
use crate::features::auth::dto::{
    LoginRequest, LoginResponse, LogoutResponse, PinChangeReq, PinResetReq, PinUpdatedRes,
    RegisterRequest, RegisterResponse, UserDto,
};
use crate::features::events::service::{log_event, log_event_txn};
use argon2::{
    Argon2,
    password_hash::{PasswordHasher, PasswordVerifier},
};
use chrono::Utc;
use password_hash::phc::PasswordHash;
use rand::RngExt;
use redis::AsyncCommands;
use sqlx::Row;
use tower_cookies::{Cookie, Cookies};
use tracing::info;
use uuid::Uuid;

pub const SEED_BALANCE_PAISA: i64 = 10_000_000; // ৳100,000 (§2)
pub const MAX_LOGIN_FAILURES: i64 = 5;
pub const LOCKOUT_DURATION_SECS: u64 = 900; // 15 minutes (R15, C40)

pub fn generate_account_number() -> String {
    let mut rng = rand::rng();
    let num: u64 = rng.random_range(1_000_000_000..=9_999_999_999);
    num.to_string()
}

pub fn generate_session_token() -> String {
    let mut bytes = [0u8; 32];
    rand::rng().fill(&mut bytes);
    hex::encode(bytes)
}

pub fn hash_password(password: &str) -> Result<String, AppError> {
    let argon2 = Argon2::default();
    let hash = argon2
        .hash_password(password.as_bytes())
        .map_err(|e| AppError::Internal(format!("Failed to hash password: {}", e)))?
        .to_string();
    Ok(hash)
}

pub fn verify_password(password: &str, password_hash_str: &str) -> Result<bool, AppError> {
    let parsed_hash = PasswordHash::new(password_hash_str)
        .map_err(|e| AppError::Internal(format!("Invalid password hash format: {}", e)))?;
    let argon2 = Argon2::default();
    Ok(argon2
        .verify_password(password.as_bytes(), &parsed_hash)
        .is_ok())
}

pub async fn register_user(
    state: &AppState,
    req: RegisterRequest,
) -> Result<RegisterResponse, AppError> {
    let phone_clean = req.phone.trim().to_string();
    let name_clean = req.name.trim().to_string();

    // Check duplicate phone early
    let existing: Option<Uuid> = sqlx::query_scalar("SELECT id FROM users WHERE phone = $1")
        .bind(&phone_clean)
        .fetch_optional(&state.db)
        .await?;

    if existing.is_some() {
        return Err(AppError::Conflict(
            "Phone number is already registered".to_string(),
        ));
    }

    let password_hash = hash_password(&req.password)?;
    let pin_hash = hash_pin(&req.pin)?;
    let user_id = Uuid::new_v4();
    let account_number = generate_account_number();
    let funding_ref = generate_funding_reference();
    let now = Utc::now();

    let mut tx = state.db.begin().await?;

    // 1. Insert user
    sqlx::query(
        r#"
        INSERT INTO users (id, account_number, name, phone, password_hash, pin_hash, pin_updated_at, status, created_at, updated_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7, 'active', $8, $8)
        "#,
    )
    .bind(user_id)
    .bind(&account_number)
    .bind(&name_clean)
    .bind(&phone_clean)
    .bind(&password_hash)
    .bind(&pin_hash)
    .bind(now)
    .bind(now)
    .execute(&mut *tx)
    .await?;

    // 2. Insert initial seed balance (10M paisa = ৳100,000)
    sqlx::query(
        r#"
        INSERT INTO balances (user_id, amount_paisa, version, updated_at)
        VALUES ($1, $2, 0, $3)
        "#,
    )
    .bind(user_id)
    .bind(SEED_BALANCE_PAISA)
    .bind(now)
    .execute(&mut *tx)
    .await?;

    // 3. Append initial funding ledger row (+1 credit)
    let funding_txn_id = Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO ledger (txn_id, reference, user_id, counterparty_id, direction, amount_paisa, running_balance, kind, created_at)
        VALUES ($1, $2, $3, NULL, 1, $4, $4, 'funding', $5)
        "#,
    )
    .bind(funding_txn_id)
    .bind(&funding_ref)
    .bind(user_id)
    .bind(SEED_BALANCE_PAISA)
    .bind(now)
    .execute(&mut *tx)
    .await?;

    // 4. Insert tx_history read model row (§2, W1)
    sqlx::query(
        r#"
        INSERT INTO tx_history (user_id, status, kind, direction, amount_paisa, balance_after, counterparty_id, reference, entity_id, note, created_at)
        VALUES ($1, 'completed', 'funding', 'received', $2, $2, NULL, $3, $4, 'Initial account registration seed', $5)
        "#,
    )
    .bind(user_id)
    .bind(SEED_BALANCE_PAISA)
    .bind(&funding_ref)
    .bind(funding_txn_id)
    .bind(now)
    .execute(&mut *tx)
    .await?;

    // 5. Append process_event
    log_event_txn(
        &mut tx,
        "auth",
        user_id,
        "registered",
        Some(user_id),
        "Initial user registration and 10M seed funding",
        serde_json::json!({
            "account_number": account_number,
            "seed_paisa": SEED_BALANCE_PAISA
        }),
    )
    .await?;

    tx.commit().await?;

    // Asynchronous welcome email (W1, R22)
    state.mailer.dispatch_email(
        format!("{}@pstupay.local", phone_clean),
        "Welcome to PSTU Pay".to_string(),
        format!(
            "Hello {},\n\nYour account {} has been created with initial seed funding of ৳100,000.\nFunding TrxID: {}\n\nThank you,\nPSTU Pay Team",
            name_clean, account_number, funding_ref
        ),
    );

    info!(
        user_id = %user_id,
        account_number = %account_number,
        phone = %phone_clean,
        "User registered successfully with initial seed funding"
    );

    Ok(RegisterResponse {
        id: user_id,
        account_number,
        name: name_clean,
        phone: phone_clean,
        balance: SEED_BALANCE_PAISA.to_string(),
        created_at: now,
    })
}

pub async fn login_user(
    state: &AppState,
    req: LoginRequest,
    cookies: &Cookies,
) -> Result<LoginResponse, AppError> {
    let phone_clean = req.phone.trim().to_string();
    let lockout_key = format!("lf:{}", phone_clean);

    let mut redis_conn = state.redis.clone();

    // 1. Check brute force lockout defense (R15, C40)
    let failures: i64 = redis_conn.get(&lockout_key).await.unwrap_or(0);
    if failures >= MAX_LOGIN_FAILURES {
        return Err(AppError::RateLimited(
            "Account temporarily locked due to too many failed attempts. Please try again in 15 minutes."
                .to_string(),
        ));
    }

    // 2. Fetch user from DB
    let user_row = sqlx::query(
        r#"
        SELECT u.id, u.account_number, u.name, u.phone, u.password_hash, u.status, u.created_at,
               b.amount_paisa as balance
        FROM users u
        JOIN balances b ON b.user_id = u.id
        WHERE u.phone = $1
        "#,
    )
    .bind(&phone_clean)
    .fetch_optional(&state.db)
    .await?;

    let row = match user_row {
        Some(r) => r,
        None => {
            let new_failures: i64 = redis_conn.incr(&lockout_key, 1).await.unwrap_or(1);
            let _: Result<(), _> = redis_conn
                .expire(&lockout_key, LOCKOUT_DURATION_SECS as i64)
                .await;

            if new_failures >= MAX_LOGIN_FAILURES {
                return Err(AppError::RateLimited(
                    "Account temporarily locked due to too many failed attempts. Please try again in 15 minutes."
                        .to_string(),
                ));
            }
            return Err(AppError::Unauthorized(
                "Invalid phone number or password".to_string(),
            ));
        }
    };

    let user_id: Uuid = row.get("id");
    let password_hash: String = row.get("password_hash");
    let status: String = row.get("status");

    if status != "active" {
        return Err(AppError::Forbidden(
            "Account is suspended or locked".to_string(),
        ));
    }

    // 3. Verify password
    let password_ok = verify_password(&req.password, &password_hash)?;
    if !password_ok {
        let new_failures: i64 = redis_conn.incr(&lockout_key, 1).await.unwrap_or(1);
        let _: Result<(), _> = redis_conn
            .expire(&lockout_key, LOCKOUT_DURATION_SECS as i64)
            .await;

        let _ = log_event(
            state,
            "auth",
            user_id,
            "login_failed",
            Some(user_id),
            "Failed login attempt (incorrect password)",
            serde_json::json!({ "failure_count": new_failures }),
        )
        .await;

        if new_failures >= MAX_LOGIN_FAILURES {
            return Err(AppError::RateLimited(
                "Account temporarily locked due to too many failed attempts. Please try again in 15 minutes."
                    .to_string(),
            ));
        }

        return Err(AppError::Unauthorized(
            "Invalid phone number or password".to_string(),
        ));
    }

    // 4. Success: Clear lockout and create 24h Redis session
    let _: Result<(), _> = redis_conn.del(&lockout_key).await;
    let session_token = generate_session_token();
    let session_key = format!("session:{}", session_token);
    let user_sessions_key = format!("sids:{}", user_id);

    let session_ttl = state.config.session_ttl_secs;
    let _: Result<(), _> = redis_conn
        .set_ex(&session_key, user_id.to_string(), session_ttl)
        .await;
    let _: Result<(), _> = redis_conn.sadd(&user_sessions_key, &session_token).await;

    let mut cookie = Cookie::new("sid", session_token);
    cookie.set_http_only(true);
    cookie.set_path("/");
    cookie.set_same_site(tower_cookies::cookie::SameSite::Lax);
    cookie.set_max_age(Some(tower_cookies::cookie::time::Duration::seconds(
        session_ttl as i64,
    )));
    cookies.add(cookie);

    let _ = log_event(
        state,
        "auth",
        user_id,
        "login_success",
        Some(user_id),
        "Successful user login",
        serde_json::json!({}),
    )
    .await;

    Ok(LoginResponse {
        user: UserDto {
            id: user_id,
            account_number: row.get("account_number"),
            name: row.get("name"),
            phone: row.get("phone"),
            created_at: row.get("created_at"),
        },
        balance: row.get::<i64, _>("balance").to_string(),
    })
}

pub async fn logout_user(
    state: &AppState,
    user_id: Uuid,
    cookies: &Cookies,
) -> Result<LogoutResponse, AppError> {
    if let Some(cookie) = cookies.get("sid") {
        let session_token = cookie.value();
        let session_key = format!("session:{}", session_token);
        let user_sessions_key = format!("sids:{}", user_id);

        let mut redis_conn = state.redis.clone();
        let _: Result<(), _> = redis_conn.del(&session_key).await;
        let _: Result<(), _> = redis_conn.srem(&user_sessions_key, session_token).await;

        let mut remove_cookie = Cookie::new("sid", "");
        remove_cookie.set_path("/");
        remove_cookie.set_same_site(tower_cookies::cookie::SameSite::Lax);
        remove_cookie.set_max_age(Some(tower_cookies::cookie::time::Duration::seconds(0)));
        cookies.add(remove_cookie);
    }

    let _ = log_event(
        state,
        "auth",
        user_id,
        "logout",
        Some(user_id),
        "User logged out",
        serde_json::json!({}),
    )
    .await;

    Ok(LogoutResponse { ok: true })
}

pub async fn change_pin(
    state: &AppState,
    user_id: Uuid,
    req: PinChangeReq,
) -> Result<PinUpdatedRes, AppError> {
    // 1. Verify current PIN (R17, C46, C47)
    enforce_user_pin(state, user_id, &req.current_pin).await?;

    // 2. Hash new PIN and update
    let new_pin_hash = hash_pin(&req.new_pin)?;
    let now = Utc::now();

    sqlx::query("UPDATE users SET pin_hash = $1, pin_updated_at = $2 WHERE id = $3")
        .bind(new_pin_hash)
        .bind(now)
        .bind(user_id)
        .execute(&state.db)
        .await?;

    let _ = log_event(
        state,
        "auth",
        user_id,
        "pin_changed",
        Some(user_id),
        "Transaction PIN changed successfully",
        serde_json::json!({}),
    )
    .await;

    Ok(PinUpdatedRes {
        ok: true,
        pin_updated_at: now,
    })
}

pub async fn reset_pin(
    state: &AppState,
    user_id: Uuid,
    req: PinResetReq,
    cookies: &Cookies,
) -> Result<PinUpdatedRes, AppError> {
    // 1. Verify login password (R17, W13)
    let password_hash: Option<String> =
        sqlx::query_scalar("SELECT password_hash FROM users WHERE id = $1")
            .bind(user_id)
            .fetch_optional(&state.db)
            .await?;

    let pass_hash =
        password_hash.ok_or_else(|| AppError::NotFound("User not found".to_string()))?;
    let pass_ok = verify_password(&req.password, &pass_hash)?;

    if !pass_ok {
        return Err(AppError::Unauthorized(
            "Incorrect login password for PIN reset".to_string(),
        ));
    }

    // 2. Update PIN
    let new_pin_hash = hash_pin(&req.new_pin)?;
    let now = Utc::now();

    sqlx::query("UPDATE users SET pin_hash = $1, pin_updated_at = $2 WHERE id = $3")
        .bind(new_pin_hash)
        .bind(now)
        .bind(user_id)
        .execute(&state.db)
        .await?;

    // 3. Invalidate all other sessions except current (W13)
    let current_sid = cookies.get("sid").map(|c| c.value().to_string());
    let user_sessions_key = format!("sids:{}", user_id);
    let mut redis_conn = state.redis.clone();

    let all_sids: Vec<String> = redis_conn
        .smembers(&user_sessions_key)
        .await
        .unwrap_or_default();
    for sid in all_sids {
        if Some(&sid) != current_sid.as_ref() {
            let session_key = format!("session:{}", sid);
            let _: Result<(), _> = redis_conn.del(&session_key).await;
            let _: Result<(), _> = redis_conn.srem(&user_sessions_key, &sid).await;
        }
    }

    let _ = log_event(
        state,
        "auth",
        user_id,
        "pin_reset",
        Some(user_id),
        "Transaction PIN reset via password; secondary sessions invalidated",
        serde_json::json!({}),
    )
    .await;

    // Asynchronous PIN reset notification (W13, R22)
    state.mailer.dispatch_email(
        format!("{}@pstupay.local", user_id),
        "Security Alert: Transaction PIN Reset".to_string(),
        "Your transaction PIN was recently reset using your account password. All secondary active sessions have been invalidated.\nIf you did not perform this action, please contact support immediately.".to_string(),
    );

    Ok(PinUpdatedRes {
        ok: true,
        pin_updated_at: now,
    })
}
