use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};

#[test]
fn test_c05_atomic_balance_exhaustion_simulation() {
    let initial_balance = 100_000i64; // 100,000 paisa
    let transfer_amount = 10_000i64; // 10,000 paisa each
    let total_attempts = 20;

    let balance = Arc::new(AtomicI64::new(initial_balance));
    let mut handles = Vec::new();
    let success_count = Arc::new(AtomicI64::new(0));
    let fail_count = Arc::new(AtomicI64::new(0));

    for _ in 0..total_attempts {
        let bal = Arc::clone(&balance);
        let succ = Arc::clone(&success_count);
        let fail = Arc::clone(&fail_count);

        let handle = std::thread::spawn(move || {
            // Emulate atomic CAS balance update with lock
            loop {
                let current = bal.load(Ordering::SeqCst);
                if current < transfer_amount {
                    fail.fetch_add(1, Ordering::SeqCst);
                    break;
                }
                if bal
                    .compare_exchange(
                        current,
                        current - transfer_amount,
                        Ordering::SeqCst,
                        Ordering::SeqCst,
                    )
                    .is_ok()
                {
                    succ.fetch_add(1, Ordering::SeqCst);
                    break;
                }
            }
        });
        handles.push(handle);
    }

    for h in handles {
        h.join().unwrap();
    }

    assert_eq!(success_count.load(Ordering::SeqCst), 10);
    assert_eq!(fail_count.load(Ordering::SeqCst), 10);
    assert_eq!(balance.load(Ordering::SeqCst), 0);
}
