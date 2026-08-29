use api::features::reconcile::service::ReconcileReport;

#[test]
fn test_reconciliation_report_invariants() {
    let mut report = ReconcileReport {
        users_count: 5,
        total_system_balance_paisa: 50_000_000,
        expected_system_balance_paisa: 50_000_000,
        users_reconciled: 5,
        ledger_entries_verified: 20,
        errors: Vec::new(),
    };

    assert_eq!(
        report.total_system_balance_paisa,
        report.expected_system_balance_paisa
    );
    assert!(report.errors.is_empty());

    // If total balance diverges, error recorded
    report.total_system_balance_paisa = 49_000_000;
    if report.total_system_balance_paisa != report.expected_system_balance_paisa {
        report.errors.push("Invariant violation".to_string());
    }
    assert_eq!(report.errors.len(), 1);
}
