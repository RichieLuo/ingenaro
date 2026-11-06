use super::*;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};

#[test]
fn internal_wait_ignores_deadline_but_cancel_is_immediate() {
    let (host, client) = ToolHost::new();
    let stopped = Arc::new(AtomicBool::new(false));
    let worker_flag = stopped.clone();
    let client = client.until_cancelled();
    let worker = std::thread::spawn(move || {
        client.call_until(
            ToolCall {
                name: "rpshop_read_pages".into(),
                arguments: serde_json::json!({}),
            },
            &worker_flag,
            Duration::from_millis(1),
        )
    });
    let deadline = Instant::now() + Duration::from_secs(2);
    let request = loop {
        if let Some(request) = host.try_recv() {
            break request;
        }
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    };
    std::thread::park_timeout(Duration::from_millis(25));
    assert!(!worker.is_finished());
    assert!(!request.is_cancelled());
    stopped.store(true, Ordering::SeqCst);
    assert!(worker.join().unwrap().is_err());
    assert!(request.is_cancelled());
}
