pub mod core;
pub mod features;

use crate::core::state::AppState;
use axum::{
    Router,
    extract::DefaultBodyLimit,
    http::{HeaderName, HeaderValue, Method, header},
};
use tower_cookies::CookieManagerLayer;
use tower_http::{
    catch_panic::CatchPanicLayer,
    cors::{AllowOrigin, Any, CorsLayer},
    request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer},
    trace::TraceLayer,
};

pub fn build_app(state: AppState) -> Router {
    let x_request_id = HeaderName::from_static("x-request-id");
    let x_idempotent_replay = HeaderName::from_static("x-idempotent-replay");
    let x_csrf = HeaderName::from_static("x-csrf");

    // Dynamic CORS configuration (R14)
    let cors_cfg = state.config.cors_allowed_origins.trim();
    let cors = if cors_cfg == "*" || cors_cfg.is_empty() {
        CorsLayer::new()
            .allow_origin(Any)
            .allow_methods([
                Method::GET,
                Method::POST,
                Method::PUT,
                Method::DELETE,
                Method::PATCH,
                Method::OPTIONS,
                Method::HEAD,
            ])
            .allow_headers([
                header::AUTHORIZATION,
                header::CONTENT_TYPE,
                header::ACCEPT,
                header::ORIGIN,
                header::COOKIE,
                x_request_id.clone(),
                x_csrf.clone(),
                x_idempotent_replay.clone(),
            ])
            .expose_headers([
                x_request_id.clone(),
                x_idempotent_replay.clone(),
                header::CONTENT_DISPOSITION,
            ])
    } else {
        let origins: Vec<HeaderValue> = cors_cfg
            .split(',')
            .filter_map(|s| {
                let trimmed = s.trim();
                if !trimmed.is_empty() {
                    HeaderValue::from_str(trimmed).ok()
                } else {
                    None
                }
            })
            .collect();

        CorsLayer::new()
            .allow_origin(AllowOrigin::list(origins))
            .allow_credentials(true)
            .allow_methods([
                Method::GET,
                Method::POST,
                Method::PUT,
                Method::DELETE,
                Method::PATCH,
                Method::OPTIONS,
                Method::HEAD,
            ])
            .allow_headers([
                header::AUTHORIZATION,
                header::CONTENT_TYPE,
                header::ACCEPT,
                header::ORIGIN,
                header::COOKIE,
                x_request_id.clone(),
                x_csrf.clone(),
                x_idempotent_replay.clone(),
            ])
            .expose_headers([
                x_request_id.clone(),
                x_idempotent_replay.clone(),
                header::CONTENT_DISPOSITION,
            ])
    };

    let api_v1 = Router::new()
        .merge(features::health::routes::router())
        .merge(features::auth::routes::router())
        .merge(features::users::routes::router())
        .merge(features::transfers::routes::router())
        .merge(features::history::routes::router())
        .merge(features::requests::routes::router())
        .merge(features::links::routes::router())
        .merge(features::notifications::routes::router())
        .merge(features::ai::routes::router());

    Router::new()
        .merge(features::health::routes::router())
        .nest("/api/v1", api_v1)
        .layer(DefaultBodyLimit::max(state.config.max_body_size_bytes))
        .layer(CookieManagerLayer::new())
        .layer(cors)
        .layer(PropagateRequestIdLayer::new(x_request_id.clone()))
        .layer(SetRequestIdLayer::new(x_request_id, MakeRequestUuid))
        .layer(TraceLayer::new_for_http())
        .layer(CatchPanicLayer::new())
        .with_state(state)
}
