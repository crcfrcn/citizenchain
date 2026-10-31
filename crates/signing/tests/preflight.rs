//! 原所属签名回归，输入和断言保持。
#![allow(clippy::expect_used, clippy::unwrap_used)]
use chain_signing::*;

#[test]
fn classify_invalid_tx_known_variants() {
    assert!(classify_invalid_tx(&[0x01, 0x00, 0x02]).contains("Future"));
    assert!(classify_invalid_tx(&[0x01, 0x00, 0x03]).contains("Stale"));
    assert!(classify_invalid_tx(&[0x01, 0x00, 0x04]).contains("BadProof"));
    assert!(classify_invalid_tx(&[0x01, 0x00, 0x01]).contains("Payment"));
}

#[test]
fn dry_run_reject_future_gives_user_hint() {
    assert_eq!(
        dry_run_reject_message(&[0x01, 0x00, 0x02], "0x010002"),
        "上一笔交易尚未出块，请稍候再试"
    );
}

#[test]
fn preflight_runtime_api_error_stops_before_submit() {
    let message = preflight_reject_message(
        r#"RuntimeApi("Execution failed: wasm trap: unreachable in TaggedTransactionQueue_validate_transaction")"#,
    );
    assert!(message.starts_with("链运行时校验失败，交易未提交"));
    assert!(message.contains("TaggedTransactionQueue_validate_transaction"));
}

#[test]
fn all_invalid_transaction_tags_and_short_inputs_have_stable_classification() {
    for (tag, expected) in [
        (0, "Call"),
        (1, "Payment"),
        (2, "Future"),
        (3, "Stale"),
        (4, "BadProof"),
        (5, "AncientBirthBlock"),
        (6, "ExhaustsResources"),
        (7, "Custom"),
        (8, "BadMandatory"),
        (9, "MandatoryValidation"),
        (10, "BadSigner"),
        (11, "IndeterminateImplicit"),
        (12, "UnknownOrigin"),
    ] {
        assert!(classify_invalid_tx(&[1, 0, tag])
            .starts_with(&format!("InvalidTransaction::{expected}")));
    }
    for input in [vec![], vec![1], vec![1, 1]] {
        assert_eq!(classify_invalid_tx(&input), "UnknownTransaction");
    }
    assert_eq!(
        classify_invalid_tx(&[1, 0, 255]),
        "InvalidTransaction::Unknown"
    );
    assert!(dry_run_reject_message(&[1, 0, 4], "0x010004").contains("BadProof"));
    for error in [
        "RuntimeApi unavailable",
        "wasm trap",
        "unreachable",
        "TaggedTransactionQueue_validate_transaction failed",
    ] {
        assert!(preflight_reject_message(error).starts_with("链运行时校验失败，交易未提交"));
    }
    assert!(
        preflight_reject_message("connection refused").starts_with("链交易预检失败，交易未提交")
    );
}
