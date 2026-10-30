use super::*;

#[test]
fn upgrade_call_rejects_missing_reason_and_cid_outside_protocol_bounds() {
    let params = difficulty::PowDifficultyParams::genesis_default();
    assert!(propose_runtime_upgrade("", b"candidate", "reason", params).is_err());
    let maximum = primitives::core_const::CID_NUMBER_MAX_BYTES as usize;
    assert!(
        propose_runtime_upgrade(&"a".repeat(maximum + 1), b"candidate", "reason", params).is_err()
    );
    assert!(propose_runtime_upgrade("44", b"candidate", "", params).is_err());
    assert!(propose_runtime_upgrade(&"a".repeat(maximum), b"candidate", "reason", params).is_ok());
}
