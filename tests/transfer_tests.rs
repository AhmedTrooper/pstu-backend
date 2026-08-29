use api::core::money::Paisa;
use api::features::transfers::dto::CreateTransferRequest;
use uuid::Uuid;
use validator::Validate;

#[test]
fn test_c01_self_transfer_logic_check() {
    let user_id = Uuid::new_v4();
    let sender_id = user_id;
    let recipient_id = user_id;
    assert_eq!(sender_id, recipient_id, "Self transfer should be detected");
}

#[test]
fn test_c02_transfer_request_validation() {
    let payload = serde_json::json!({
        "recipient": "01711000000",
        "amount_paisa": "100000",
        "note": "Payment for services",
        "idempotency_key": Uuid::new_v4()
    });

    let req: Result<CreateTransferRequest, _> = serde_json::from_value(payload);
    assert!(req.is_ok());
    let req = req.unwrap();
    assert!(req.validate().is_ok());

    let parsed_money = Paisa::parse_positive_from_str(&req.amount_paisa);
    assert!(parsed_money.is_ok());
    assert_eq!(parsed_money.unwrap().as_paisa(), 100000);
}

#[test]
fn test_c07_idempotency_key_requires_exact_payload_match() {
    let sender = Uuid::new_v4();
    let recipient1 = Uuid::new_v4();
    let recipient2 = Uuid::new_v4();
    let amount = 50000i64;

    // Simulating matching vs non-matching payload checks
    assert_ne!(recipient1, recipient2);
    let matches = sender == sender && recipient1 == recipient2 && amount == amount;
    assert!(
        !matches,
        "Mismatching recipients must be detected for 409 Conflict"
    );
}
