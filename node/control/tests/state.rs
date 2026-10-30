use super::*;

#[test]
fn lifecycle_contract_has_nine_unique_states_and_clean_default() {
    let states = [
        NodeLifecycleState::Stopped,
        NodeLifecycleState::Starting,
        NodeLifecycleState::Initializing,
        NodeLifecycleState::Running,
        NodeLifecycleState::Stopping,
        NodeLifecycleState::Restarting,
        NodeLifecycleState::Failed,
        NodeLifecycleState::LockHeld,
        NodeLifecycleState::Exited,
    ];
    assert_eq!(
        states.map(NodeLifecycleState::as_str),
        [
            "stopped",
            "starting",
            "initializing",
            "running",
            "stopping",
            "restarting",
            "failed",
            "lock_held",
            "exited"
        ]
    );
    let default = RuntimeState::default();
    assert!(default.node_handle.is_none());
    assert!(default.last_error.is_none());
    assert_eq!(default.node_state, NodeLifecycleState::Stopped);
}

#[test]
fn lifecycle_gate_remains_locked_until_owner_releases_it() {
    let guard = lock_node_lifecycle();
    assert!(NODE_LIFECYCLE_LOCK.get().unwrap().try_lock().is_err());
    drop(guard);
    assert!(NODE_LIFECYCLE_LOCK.get().unwrap().try_lock().is_ok());
}
