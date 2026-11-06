use super::*;
use std::sync::mpsc;
use std::time::{Duration, Instant};

fn finish<T: Send + 'static>(load: &mut Load<T>) -> Result<T> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(value) = load.take_result() {
            return value;
        }
        assert!(Instant::now() < deadline, "background read did not finish");
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn newer_requests_cancel_old_results_and_coalesce_without_extra_workers() {
    let mut load = Load::default();
    let (release, wait) = mpsc::channel();
    load.request();
    load.start("igit-test-read", move |version| {
        wait.recv().unwrap();
        assert!(version.cancelled());
        "old"
    })
    .unwrap();
    for _ in 0..10 {
        load.request();
        load.start("igit-test-read", |_| {
            panic!("a second reader started before the first finished")
        })
        .unwrap();
    }
    assert!(load.loading());
    assert!(load.take_result().is_none());
    release.send(()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while load.running.is_some() {
        assert!(load.take_result().is_none(), "stale result escaped");
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
    load.start("igit-test-read", |_| "latest").unwrap();
    assert_eq!(finish(&mut load).unwrap(), "latest");
    assert!(!load.pending());
}

#[test]
fn cancelled_disconnection_is_ignored_and_current_failure_can_retry() {
    let mut load: Load<()> = Load::default();
    let (sender, receiver) = mpsc::channel();
    load.running = Some((0, receiver));
    load.cancel();
    drop(sender);
    assert!(load.take_result().is_none());
    assert!(!load.pending());

    let (sender, receiver) = mpsc::channel();
    load.running = Some((load.generation.load(Ordering::Relaxed), receiver));
    drop(sender);
    assert!(load.take_result().unwrap().is_err());
    load.request();
    load.start("igit-test-read", |_| ()).unwrap();
    finish(&mut load).unwrap();
    assert!(!load.pending());
}
