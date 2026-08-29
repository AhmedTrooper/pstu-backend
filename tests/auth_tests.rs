use api::features::auth::dto::RegisterRequest;
use axum::http::{Method, Request, header};
use validator::Validate;

#[test]
fn test_c09_registration_validation_bad_fields() {
    let payload = serde_json::json!({
        "name": "",
        "phone": "01711",
        "password": "123"
    });

    let reg_req: Result<RegisterRequest, _> = serde_json::from_value(payload);
    assert!(reg_req.is_ok());
    let reg_req = reg_req.unwrap();
    let val_res = reg_req.validate();
    assert!(
        val_res.is_err(),
        "Validation should fail for empty name/short password"
    );
}

#[test]
fn test_c10_unauthenticated_request_has_no_credentials() {
    let req = Request::builder()
        .uri("/api/v1/me")
        .method(Method::GET)
        .body(())
        .unwrap();

    assert!(req.headers().get(header::AUTHORIZATION).is_none());
}
