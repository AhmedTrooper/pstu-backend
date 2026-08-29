use api::core::error::AppError;

#[test]
fn test_rate_limit_error_mapping() {
    let err = AppError::RateLimited("Too many requests".to_string());
    assert_eq!(err.status_code(), axum::http::StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(err.error_code(), "RATE_LIMITED");
}
