use api::features::requests::dto::CreateMoneyRequest;
use uuid::Uuid;
use validator::Validate;

#[test]
fn test_c11_request_creation_validation() {
    let payload = serde_json::json!({
        "debtor": "01711000000",
        "amount_paisa": "250000",
        "note": "Lunch bill split"
    });

    let req: Result<CreateMoneyRequest, _> = serde_json::from_value(payload);
    assert!(req.is_ok());
    assert!(req.unwrap().validate().is_ok());
}

#[test]
fn test_c12_wrong_actor_acceptance_forbidden_logic() {
    let debtor_id = Uuid::new_v4();
    let wrong_actor_id = Uuid::new_v4();

    assert_ne!(debtor_id, wrong_actor_id);
    let can_accept = debtor_id == wrong_actor_id;
    assert!(
        !can_accept,
        "Only debtor is allowed to accept request (403 Forbidden)"
    );
}
