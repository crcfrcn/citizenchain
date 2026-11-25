//! 验证两类GRANDPA更换调用符合Runtime编码合同。

use crate::key::proof::build_runtime_call;
use codec::DecodeAll;

#[test]
fn both_call_indices_decode_with_runtime_contract() {
    let common_signature = [7u8; 64];
    let emergency = build_runtime_call(
        "LN001-NRC0G-944805165-2026",
        [3u8; 32],
        0,
        60,
        common_signature,
        None,
    )
    .expect("emergency call");
    assert_eq!(&emergency[..2], &[15, 0]);
    assert!(matches!(
        runtime::RuntimeCall::decode_all(&mut emergency.as_slice()).unwrap(),
        runtime::RuntimeCall::GrandpaKey(_)
    ));

    let routine = build_runtime_call(
        "LN001-NRC0G-944805165-2026",
        [4u8; 32],
        0,
        60,
        common_signature,
        Some([8u8; 64]),
    )
    .expect("routine call");
    assert_eq!(&routine[..2], &[15, 1]);
    assert!(matches!(
        runtime::RuntimeCall::decode_all(&mut routine.as_slice()).unwrap(),
        runtime::RuntimeCall::GrandpaKey(_)
    ));
}

#[test]
fn settings_preserve_stopped_and_running_node_call_order() {
    use crate::key::settings::apply_change;
    use std::cell::RefCell;
    for (running, expected) in [
        (false, vec!["save"]),
        (true, vec!["save", "stop", "start", "verify"]),
    ] {
        let calls = RefCell::new(Vec::new());
        let call = |name| {
            calls.borrow_mut().push(name);
            Ok(())
        };
        apply_change(
            running,
            || call("save"),
            || call("stop"),
            || call("start"),
            || call("verify"),
            || call("restore"),
        )
        .unwrap();
        assert_eq!(*calls.borrow(), expected);
    }
}

#[test]
fn settings_failures_restore_exact_original_restart_and_audit_semantics() {
    use crate::key::settings::apply_change;
    use std::cell::RefCell;
    for (failures, expected, rollback_failed) in [
        (vec!["save"], vec!["save", "restore"], false),
        (vec!["stop1"], vec!["save", "stop1", "restore"], false),
        (
            vec!["start1"],
            vec!["save", "stop1", "start1", "stop2", "restore", "start2"],
            false,
        ),
        (
            vec!["verify"],
            vec![
                "save", "stop1", "start1", "verify", "stop2", "restore", "start2",
            ],
            false,
        ),
        (
            vec!["verify", "stop2"],
            vec![
                "save", "stop1", "start1", "verify", "stop2", "restore", "start2",
            ],
            false,
        ),
        (
            vec!["verify", "restore"],
            vec!["save", "stop1", "start1", "verify", "stop2", "restore"],
            true,
        ),
        (
            vec!["verify", "start2"],
            vec![
                "save", "stop1", "start1", "verify", "stop2", "restore", "start2",
            ],
            true,
        ),
    ] {
        let calls = RefCell::new(Vec::new());
        let mut starts = 0;
        let mut stops = 0;
        let call = |name: &str| {
            calls.borrow_mut().push(name.to_owned());
            if failures.contains(&name) {
                Err(name.to_owned())
            } else {
                Ok(())
            }
        };
        let error = apply_change(
            true,
            || call("save"),
            || {
                stops += 1;
                call(&format!("stop{stops}"))
            },
            || {
                starts += 1;
                call(&format!("start{starts}"))
            },
            || call("verify"),
            || call("restore"),
        )
        .unwrap_err();
        assert_eq!(*calls.borrow(), expected);
        assert_eq!(error.rollback_failed, rollback_failed);
        assert!(error.detail.contains(failures[0]));
        if failures.contains(&"restore") {
            assert!(error.detail.contains("回滚旧配置失败：restore"));
        }
        if failures.contains(&"start2") {
            assert!(error
                .detail
                .contains("恢复旧配置后重新启动节点失败：start2"));
        }
    }
}

#[test]
fn rotation_keeps_finalized_authority_checks_and_submitted_expiry_boundaries() {
    use crate::key::rotation::{next_action, Action};
    assert_eq!(
        next_action(true, true, false, false, 61, 60),
        Action::Finalize
    );
    for (matches, new_active, old_active) in [
        (false, true, false),
        (true, false, false),
        (true, true, true),
    ] {
        assert_eq!(
            next_action(matches, new_active, old_active, false, 60, 60),
            Action::Wait
        );
        assert_eq!(
            next_action(matches, new_active, old_active, false, 61, 60),
            Action::Discard
        );
        assert_eq!(
            next_action(matches, new_active, old_active, true, u64::MAX, 60),
            Action::Wait
        );
    }
    assert_eq!(
        next_action(false, false, true, false, u64::from(u32::MAX), u32::MAX),
        Action::Wait
    );
    assert_eq!(
        next_action(false, false, true, false, u64::from(u32::MAX) + 1, u32::MAX),
        Action::Discard
    );
}
