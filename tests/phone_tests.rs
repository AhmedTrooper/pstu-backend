use api::features::users::service::normalize_phone;

#[test]
fn test_r20_phone_normalization() {
    assert_eq!(normalize_phone("01711223344"), "+8801711223344");
    assert_eq!(normalize_phone("8801711223344"), "+8801711223344");
    assert_eq!(normalize_phone("+8801711223344"), "+8801711223344");
    assert_eq!(normalize_phone("  01812345678  "), "+8801812345678");
}
