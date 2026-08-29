use std::env;

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub host: String,
    pub port: u16,
    pub database_url: String,
    pub redis_url: String,
    pub nats_url: String,
    pub session_ttl_secs: u64,
    pub default_link_ttl_secs: u64,
    pub max_body_size_bytes: usize,
    pub cors_allowed_origins: String,
    pub ai_provider: Option<String>,
    pub ai_api_key: Option<String>,
    pub ai_model: Option<String>,
    pub smtp_host: Option<String>,
    pub smtp_port: u16,
    pub smtp_user: Option<String>,
    pub smtp_password: Option<String>,
    pub mail_from_email: Option<String>,
}

impl AppConfig {
    pub fn load_from_env() -> Result<Self, String> {
        let _ = dotenvy::dotenv();

        let host = env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
        let port = env::var("PORT")
            .unwrap_or_else(|_| "8080".to_string())
            .parse::<u16>()
            .map_err(|e| format!("Invalid PORT config: {}", e))?;

        let database_url = env::var("DATABASE_URL").unwrap_or_else(|_| {
            "postgres://postgres:postgres@localhost:5432/money_movement".to_string()
        });

        let redis_url =
            env::var("REDIS_URL").unwrap_or_else(|_| "redis://localhost:6379".to_string());

        let nats_url = env::var("NATS_URL").unwrap_or_else(|_| "nats://localhost:4222".to_string());

        let session_ttl_secs = env::var("SESSION_TTL_SECS")
            .unwrap_or_else(|_| "86400".to_string())
            .parse::<u64>()
            .unwrap_or(86400);

        let default_link_ttl_secs = env::var("DEFAULT_LINK_TTL_SECS")
            .unwrap_or_else(|_| "10800".to_string())
            .parse::<u64>()
            .unwrap_or(10800);

        let max_body_size_bytes = env::var("MAX_BODY_SIZE_BYTES")
            .unwrap_or_else(|_| "1048576".to_string())
            .parse::<usize>()
            .unwrap_or(1048576);

        let cors_allowed_origins = env::var("CORS_ALLOWED_ORIGINS")
            .or_else(|_| env::var("CORS_ALLOW_ORIGIN"))
            .unwrap_or_else(|_| "*".to_string());

        let ai_provider = env::var("AI_PROVIDER").ok();
        let ai_api_key = env::var("AI_API_KEY")
            .or_else(|_| env::var("OPENAI_API_KEY"))
            .ok();
        let ai_model = env::var("AI_MODEL").ok();

        let smtp_host = env::var("SMTP_HOST").ok();
        let smtp_port = env::var("SMTP_PORT")
            .unwrap_or_else(|_| "587".to_string())
            .parse::<u16>()
            .unwrap_or(587);
        let smtp_user = env::var("SMTP_USER").ok();
        let smtp_password = env::var("SMTP_PASSWORD").ok();
        let mail_from_email = env::var("MAIL_FROM_EMAIL").ok();

        Ok(Self {
            host,
            port,
            database_url,
            redis_url,
            nats_url,
            session_ttl_secs,
            default_link_ttl_secs,
            max_body_size_bytes,
            cors_allowed_origins,
            ai_provider,
            ai_api_key,
            ai_model,
            smtp_host,
            smtp_port,
            smtp_user,
            smtp_password,
            mail_from_email,
        })
    }
}
