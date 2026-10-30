use super::*;

#[test]
fn system_username_rejects_blank_and_control_characters_without_running_authentication() {
    for name in ["", " ", "user\nname", "user\0name"] {
        assert!(validate_system_username(name).is_err());
    }
    assert_eq!(validate_system_username(" 测试账户 ").unwrap(), "测试账户");
}

#[test]
fn failure_window_keeps_boundary_future_and_other_accounts() {
    let mut store = AuthRateLimitStore::default();
    store.accounts.insert(
        "test".into(),
        AuthRateLimitState {
            recent_failures: VecDeque::from([699, 700, 1000, 1001]),
        },
    );
    store.accounts.insert(
        "other".into(),
        AuthRateLimitState {
            recent_failures: VecDeque::from([999]),
        },
    );
    assert_eq!(
        compact_auth_rate_limit_state(&mut store, "test", 1000),
        Some(3)
    );
    assert_eq!(
        store.accounts["test"].recent_failures,
        VecDeque::from([700, 1000, 1001])
    );
    assert_eq!(store.accounts["other"].recent_failures.len(), 1);
    assert_eq!(
        compact_auth_rate_limit_state(&mut store, "test", 1400),
        Some(0)
    );
    assert!(!store.accounts.contains_key("test"));
    assert!(store.accounts.contains_key("other"));
    assert_eq!(
        compact_auth_rate_limit_state(&mut store, "missing", 1400),
        None
    );
}
