use crate::platform::state_after_wait;

#[test]
fn timeout_is_starting_only_while_process_survives() {
    let waiting = state_after_wait(true, "not ready".into());
    assert_eq!(waiting.status, "starting");
    assert_eq!(waiting.status_label, "启动中");
    assert!(waiting.running && waiting.detail.is_none());
    let failed = state_after_wait(false, "exited".into());
    assert_eq!(failed.status, "error");
    assert!(!failed.running);
    assert!(failed.detail.expect("failure detail").contains("exited"));
}
