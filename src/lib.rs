pub mod core;
pub mod features;

use crate::core::state::AppState;
use axum::{
    Router,
    extract::DefaultBodyLimit,
    http::{HeaderName, Method, header},
};
use tower_cookies::CookieManagerLayer;
use tower_http::{
    catch_panic::CatchPanicLayer,
    cors::{Any, CorsLayer},
    request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer},
    trace::TraceLayer,
};

pub fn build_app(state: AppState) -> Router {
    let x_request_id = HeaderName::from_static("x-request-id");

    let cors = CorsLayer::new()
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers([
            header::AUTHORIZATION,
            header::CONTENT_TYPE,
            header::ACCEPT,
            x_request_id.clone(),
        ])
        .allow_origin(Any);

    let api_v1 = Router::new()
        .merge(features::auth::routes::router())
        .merge(features::users::routes::router())
        .merge(features::transfers::routes::router())
        .merge(features::history::routes::router());

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
