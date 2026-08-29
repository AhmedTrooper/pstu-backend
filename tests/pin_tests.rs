use api::core::pin::{hash_pin, verify_pin_hash};

#[test]
fn test_c46_pin_hash_and_verification() {
    let pin = "12345";
    let hash = hash_pin(pin).expect("PIN hashing should succeed");
    assert!(verify_pin_hash(pin, &hash).unwrap());
    assert!(!verify_pin_hash("99999", &hash).unwrap());
}
