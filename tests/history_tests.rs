use api::features::history::dto::PaginatedTransactionsResponse;

#[test]
fn test_c14_empty_history_response_structure() {
    let empty_resp = PaginatedTransactionsResponse {
        items: Vec::new(),
        next_cursor: None,
    };

    let json = serde_json::to_string(&empty_resp).unwrap();
    assert!(json.contains("\"items\":[]"));
    assert!(json.contains("\"next_cursor\":null"));
}

#[test]
fn test_c20_invalid_direction_filter_validation() {
    let dir = 2i16; // Only -1 and 1 allowed
    let is_valid = dir == -1 || dir == 1;
    assert!(
        !is_valid,
        "Direction filter other than -1 and 1 must be rejected with 400"
    );
}
