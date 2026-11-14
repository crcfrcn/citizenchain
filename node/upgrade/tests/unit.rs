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

#[test]
fn reason_uses_trimmed_utf8_byte_limit() {
    assert_eq!(normalize_reason(" \n 升级说明 \t").unwrap(), "升级说明");
    assert!(normalize_reason(" \n\t ").is_err());
    assert!(normalize_reason(&"a".repeat(1024)).is_ok());
    assert!(normalize_reason(&"a".repeat(1025)).is_err());
    assert!(normalize_reason(&("中".repeat(341) + "a")).is_ok());
    assert!(normalize_reason(&"中".repeat(342))
        .unwrap_err()
        .contains("1026 字节"));
}

#[test]
fn pow_preflight_preserves_existing_version_and_algorithm_rules() {
    let active = difficulty::PowDifficultyParams::genesis_default();
    assert!(validate_pow_params(active, active, false).is_ok());
    let mut next = active;
    next.params_version += 1;
    assert!(validate_pow_params(next, active, false).is_err());
    next = active;
    next.target_block_time_ms /= 2;
    assert!(validate_pow_params(next, active, false).is_err());
    next.params_version += 1;
    assert!(validate_pow_params(next, active, false).is_ok());
    next.params_version += 1;
    assert!(validate_pow_params(next, active, false).is_err());
    next = active;
    next.algorithm_version += 1;
    next.params_version += 1;
    assert!(validate_pow_params(next, active, false)
        .unwrap_err()
        .contains("算法版本"));
    assert!(validate_pow_params(active, active, true)
        .unwrap_err()
        .contains("待生效"));
}

#[test]
fn pow_preflight_rejects_zero_and_window_overflow_without_new_limits() {
    let active = difficulty::PowDifficultyParams::genesis_default();
    for field in 0..6 {
        let mut next = active;
        match field {
            0 => next.params_version = 0,
            1 => next.algorithm_version = 0,
            2 => next.target_block_time_ms = 0,
            3 => next.adjustment_interval = 0,
            4 => next.max_adjust_up_factor = 0,
            _ => next.max_adjust_down_divisor = 0,
        }
        assert!(validate_pow_params(next, active, false).is_err());
    }
    let mut next = active;
    next.params_version += 1;
    next.target_block_time_ms = u64::MAX;
    assert!(validate_pow_params(next, active, false).is_err());
    next = active;
    next.params_version += 1;
    next.max_adjust_up_factor = u64::MAX;
    assert!(validate_pow_params(next, active, false).is_ok());
}

fn pow_key(name: &[u8]) -> String {
    let mut bytes = sp_crypto_hashing::twox_128(b"PowDifficulty").to_vec();
    bytes.extend_from_slice(&sp_crypto_hashing::twox_128(name));
    format!("0x{}", hex::encode(bytes))
}

#[test]
fn pow_snapshot_reads_active_and_pending_at_one_finalized_hash() {
    let hash = format!("0x{}", "ab".repeat(32));
    let active = difficulty::PowDifficultyParams::genesis_default();
    let mut next = active;
    next.params_version += 1;
    next.target_block_time_ms /= 2;
    let pending = difficulty::PendingPowDifficultyParams {
        params: next,
        activate_at: 12,
    };
    let mut reads = Vec::new();
    let snapshot = crate::commands::read_pow_snapshot_at(&hash, |key, at| {
        reads.push((key.to_string(), at.to_string()));
        let value = if key == pow_key(b"ActiveParams") {
            active.encode()
        } else {
            pending.encode()
        };
        Ok(Some(format!("0x{}", hex::encode(value))))
    })
    .unwrap();
    assert_eq!(snapshot, (active, Some(pending)));
    assert_eq!(
        reads,
        vec![
            (pow_key(b"ActiveParams"), hash.clone()),
            (pow_key(b"PendingParams"), hash)
        ]
    );
}

#[test]
fn pow_snapshot_does_not_treat_missing_or_unknown_data_as_safe_defaults() {
    use crate::commands::read_pow_snapshot_at;
    let active = difficulty::PowDifficultyParams::genesis_default();
    let encoded = format!("0x{}", hex::encode(active.encode()));
    assert!(read_pow_snapshot_at("hash", |_, _| Ok(None)).is_err());
    assert!(read_pow_snapshot_at("hash", |_, _| Err("RPC失败".to_string())).is_err());
    assert!(read_pow_snapshot_at("hash", |_, _| Ok(Some(format!("{encoded}00")))).is_err());
    assert!(read_pow_snapshot_at("hash", |key, _| {
        Ok(Some(if key == pow_key(b"ActiveParams") {
            encoded.clone()
        } else {
            "0x01".to_string()
        }))
    })
    .is_err());
    assert!(read_pow_snapshot_at("hash", |key, _| {
        if key == pow_key(b"ActiveParams") {
            Ok(Some(encoded.clone()))
        } else {
            Err("读取待生效参数失败".to_string())
        }
    })
    .is_err());
    assert_eq!(
        read_pow_snapshot_at("hash", |key, _| {
            Ok(if key == pow_key(b"ActiveParams") {
                Some(encoded.clone())
            } else {
                None
            })
        })
        .unwrap(),
        (active, None)
    );
}
