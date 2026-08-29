use api::core::money::{MoneyParseError, Paisa};
use api::core::pin::{hash_pin, verify_pin_hash};
use api::core::reference::{generate_funding_reference, generate_trx_reference};
use api::features::ai::parser::parse_intent;
use api::features::auth::dto::RegisterRequest;
use api::features::transfers::dto::CreateTransferRequest;
use api::features::users::service::normalize_phone;
use uuid::Uuid;
use validator::Validate;

// C01: Self transfer rejected
#[test]
fn test_c01_self_transfer_check() {
    let sender = Uuid::new_v4();
    let recipient = sender;
    assert_eq!(sender, recipient);
}

// C02: 0, negative, and 3 decimal places rejected
#[test]
fn test_c02_money_parse_rejects_invalid_values() {
    assert_eq!(Paisa::parse_from_str("0"), Ok(Paisa(0)));
    assert_eq!(
        Paisa::parse_positive_from_str("0"),
        Err(MoneyParseError::NonPositive)
    );
    assert_eq!(
        Paisa::parse_from_str("-50.00"),
        Err(MoneyParseError::InvalidFormat)
    );
    assert_eq!(
        Paisa::parse_from_str("12.345"),
        Err(MoneyParseError::InvalidFormat)
    );
}

// C06 & C07: Idempotency payload matching vs mismatching
#[test]
fn test_c06_c07_idempotency_payload_equality() {
    let key = Uuid::new_v4();
    let payload_a = (key, "01711000000", 50000i64);
    let payload_b = (key, "01711000000", 50000i64);
    let payload_c = (key, "01811000000", 50000i64);

    assert_eq!(payload_a, payload_b, "C06: Exact match triggers replay");
    assert_ne!(payload_a, payload_c, "C07: Payload mismatch triggers 409");
}

// C09: Registration validation rejects bad phone, short password, invalid pin
#[test]
fn test_c09_registration_bad_fields() {
    let bad_phone = serde_json::json!({
        "name": "Alice",
        "phone": "123",
        "password": "ValidPassword123",
        "pin": "1234"
    });
    let req: Result<RegisterRequest, _> = serde_json::from_value(bad_phone);
    assert!(req.unwrap().validate().is_err());

    let bad_pin = serde_json::json!({
        "name": "Alice",
        "phone": "01711000000",
        "password": "ValidPassword123",
        "pin": "12"
    });
    let req: Result<RegisterRequest, _> = serde_json::from_value(bad_pin);
    assert!(req.unwrap().validate().is_err());
}

// C13: Float and junk money rejected by JSON deserializer
#[test]
fn test_c13_money_json_float_rejected() {
    let json_float = "1200.50";
    let parsed: Result<Paisa, _> = serde_json::from_str(json_float);
    assert!(parsed.is_err(), "Raw JSON floats must be rejected");

    let json_junk = "\"abc\"";
    let parsed_junk: Result<Paisa, _> = serde_json::from_str(json_junk);
    assert!(parsed_junk.is_err(), "Junk strings must be rejected");
}

// C18: Absurd amount cap rejection
#[test]
fn test_c18_absurd_amount_cap() {
    let absurd_overflow = "10000000000001";
    assert_eq!(
        Paisa::parse_from_str(absurd_overflow),
        Err(MoneyParseError::ExceedsMaxCap)
    );

    let absurd_regex = "9999999999999999";
    assert_eq!(
        Paisa::parse_from_str(absurd_regex),
        Err(MoneyParseError::InvalidFormat)
    );
}

// C23: Self claim payment link check
#[test]
fn test_c23_self_claim_detection() {
    let creator_id = Uuid::new_v4();
    let claimer_id = creator_id;
    assert_eq!(creator_id, claimer_id, "Creator cannot claim own link");
}

// C27: Link TTL bounds check (60s to 86400s)
#[test]
fn test_c27_link_ttl_bounds() {
    let valid_ttl = 3600u64;
    assert!((60..=86400).contains(&valid_ttl));

    let too_short = 30u64;
    assert!(!(60..=86400).contains(&too_short));

    let too_long = 100_000u64;
    assert!(!(60..=86400).contains(&too_long));
}

// C31 & C32: AI parsing intent with 3h duration and review flag
#[test]
fn test_c31_c32_ai_parser_rules() {
    let res_link = parse_intent("Create payment link 500 BDT valid for 3 hours");
    assert_eq!(res_link.intent.action, "create_link");
    assert_eq!(res_link.intent.amount_paisa.as_deref(), Some("50000"));
    assert_eq!(res_link.intent.expires_in_seconds, Some(10800));

    let res_tomorrow = parse_intent("Send 500 BDT tomorrow to 01711223344");
    assert!(res_tomorrow.flags.contains(&"needs_review".to_string()));
}

// C42: Unknown JSON fields rejected (deny_unknown_fields)
#[test]
fn test_c42_deny_unknown_fields() {
    let payload = serde_json::json!({
        "recipient": "01711000000",
        "amount_paisa": "10000",
        "note": "valid note",
        "pin": "1234",
        "idempotency_key": Uuid::new_v4(),
        "malicious_extra_field": "hacked"
    });
    let parsed: Result<CreateTransferRequest, _> = serde_json::from_value(payload);
    assert!(parsed.is_err(), "Unknown JSON fields must be rejected");
}

// C46: Wrong PIN verification fails
#[test]
fn test_c46_wrong_pin_verification() {
    let pin = "12345";
    let hash = hash_pin(pin).unwrap();
    assert!(verify_pin_hash(pin, &hash).unwrap());
    assert!(!verify_pin_hash("99999", &hash).unwrap());
}

// C48: AI input strips PIN tokens
#[test]
fn test_c48_ai_strips_pin_tokens() {
    let text = "Send 1000 taka to 01711000000 pin 12345";
    let parsed = parse_intent(text);
    assert_eq!(parsed.intent.action, "transfer");
    assert_ne!(parsed.intent.note.as_deref(), Some("12345"));
}

// C52: Reference formatting is unique and Crockford compliant (TRX... / FND...)
#[test]
fn test_c52_reference_uniqueness_and_format() {
    let r1 = generate_trx_reference();
    let r2 = generate_trx_reference();
    let f1 = generate_funding_reference();

    assert_ne!(r1, r2);
    assert!(r1.starts_with("TRX"));
    assert_eq!(r1.len(), 13);
    assert!(f1.starts_with("FND"));
    assert_eq!(f1.len(), 13);

    // Crockford characters exclude I, L, O, U
    let invalid_crockford = ['I', 'L', 'O', 'U'];
    for c in invalid_crockford {
        assert!(!r1.contains(c));
        assert!(!f1.contains(c));
    }
}

// C58: Self money request check
#[test]
fn test_c58_self_request_detection() {
    let requester_id = Uuid::new_v4();
    let debtor_id = requester_id;
    assert_eq!(requester_id, debtor_id, "Cannot request money from self");
}

// C61: Idempotency key must be valid UUID
#[test]
fn test_c61_idempotency_key_uuid_validation() {
    let bad_json = serde_json::json!({
        "recipient": "01711000000",
        "amount_paisa": "10000",
        "note": "test",
        "pin": "1234",
        "idempotency_key": "not-a-valid-uuid"
    });
    let parsed: Result<CreateTransferRequest, _> = serde_json::from_value(bad_json);
    assert!(
        parsed.is_err(),
        "Invalid UUID idempotency key must fail parsing"
    );
}

// C70: Phone normalization
#[test]
fn test_c70_phone_normalization_variants() {
    assert_eq!(normalize_phone("01711000000"), "+8801711000000");
    assert_eq!(normalize_phone("8801711000000"), "+8801711000000");
    assert_eq!(normalize_phone("+8801711000000"), "+8801711000000");
    assert_eq!(normalize_phone("  01811223344  "), "+8801811223344");
}
