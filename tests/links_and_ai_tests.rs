use api::features::ai::parser::parse_intent;
use api::features::links::dto::CreatePaymentLinkRequest;
use validator::Validate;

#[test]
fn test_c31_ai_link_3h_parsing() {
    let text = "Create payment link 500 BDT valid for 3 hours";
    let resp = parse_intent(text);
    assert_eq!(resp.intent.action, "create_link");
    assert_eq!(resp.intent.amount_paisa.as_deref(), Some("50000"));
    assert_eq!(resp.intent.expires_in_seconds, Some(10800));
}

#[test]
fn test_c29_ai_transfer_intent_only() {
    let text = "Send 1200 taka to 01711223344";
    let resp = parse_intent(text);
    assert_eq!(resp.intent.action, "transfer");
    assert_eq!(resp.intent.amount_paisa.as_deref(), Some("120000"));
    assert_eq!(resp.intent.recipient.as_deref(), Some("01711223344"));
}

#[test]
fn test_c32_ai_tomorrow_needs_review_flag() {
    let text = "Send 500 BDT tomorrow to 01711223344";
    let resp = parse_intent(text);
    assert!(resp.flags.contains(&"needs_review".to_string()));
}

#[test]
fn test_c30_ai_gibberish_help() {
    let text = "hello world foo bar random words";
    let resp = parse_intent(text);
    assert_eq!(resp.intent.action, "help");
}

#[test]
fn test_c27_link_request_validation() {
    let payload = serde_json::json!({
        "amount_paisa": "10000",
        "note": "Donation link",
        "expires_in_seconds": 3600,
        "pin": "1234"
    });

    let req: Result<CreatePaymentLinkRequest, _> = serde_json::from_value(payload);
    assert!(req.is_ok());
    assert!(req.unwrap().validate().is_ok());
}
