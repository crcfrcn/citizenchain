//! 验证签名材料的完整审阅、账户与链上下文及载荷边界。

#![allow(clippy::expect_used, clippy::unwrap_used)]
use chain_signing::*;
use codec::Encode;
use sp_core::H256;

#[test]
fn signing_material_keeps_full_review_and_binds_v5_account() {
    let call = runtime::RuntimeCall::System(frame_system::Call::remark {
        remark: vec![7u8; 8],
    });
    let call_data = call.encode();
    let genesis = [9u8; 32];
    let m = build_signing_material(&call_data, &genesis, 5, 1, 1).expect("material");
    assert_eq!(m.call.encode(), call_data);
    // 完整审阅材料不受长度分界改变；实际签名始终绑定账户。
    assert!(m.payload.len() <= 256);
    assert_eq!(
        m.signing_message(&[1; 32]),
        mldsa::review_message(&[1; 32], &m.payload)
    );
    assert_ne!(m.signing_message(&[1; 32]), m.signing_message(&[2; 32]));

    let big_call = runtime::RuntimeCall::System(frame_system::Call::remark {
        remark: vec![7u8; 400],
    });
    let big = build_signing_material(&big_call.encode(), &genesis, 5, 1, 1).expect("material");
    // 大载荷仍完整审阅，两种算法均签署同一账户绑定摘要。
    assert!(big.payload.len() > 256);
    assert_eq!(big.signing_message(&[1; 32]).len(), 32);
    assert_ne!(big.payload, big.signing_message(&[1; 32]));
}

#[test]
fn tail_data_in_call_is_rejected() {
    let call = runtime::RuntimeCall::System(frame_system::Call::remark { remark: vec![] });
    let mut data = call.encode();
    data.push(0xff);
    assert!(decode_runtime_call(&data).is_err());
}

/// 覆盖原长度分界两侧，保证统一V5摘要不丢失完整审阅材料。
#[test]
fn exact_payload_boundary_and_large_review_bytes_are_preserved() {
    for size in [256usize, 257] {
        let material = (0..512)
            .map(|length| {
                let call = runtime::RuntimeCall::System(frame_system::Call::remark {
                    remark: vec![0x5a; length],
                });
                build_signing_material_from_call(call, H256::repeat_byte(7), 0, 91, 17)
            })
            .find(|material| material.payload.len() == size)
            .expect("真实载荷可覆盖边界");
        assert_eq!(
            material.signing_message(&[3; 32]),
            mldsa::review_message(&[3; 32], &material.payload)
        );
        assert_eq!(material.payload.len(), size);
    }
}

#[test]
fn nonce_versions_and_genesis_are_bound_to_the_real_payload() {
    let call = runtime::RuntimeCall::System(frame_system::Call::remark {
        remark: b"binding".to_vec(),
    });
    let base = build_signing_material_from_call(call.clone(), H256::repeat_byte(1), 0, 31, 7);
    for material in [
        build_signing_material_from_call(call.clone(), H256::repeat_byte(2), 0, 31, 7),
        build_signing_material_from_call(call.clone(), H256::repeat_byte(1), 1, 31, 7),
        build_signing_material_from_call(call.clone(), H256::repeat_byte(1), 0, 32, 7),
        build_signing_material_from_call(call, H256::repeat_byte(1), 0, 31, 8),
    ] {
        assert_ne!(base.payload, material.payload);
    }
    for nonce in [0, 63, 64, 16_383, 16_384, u32::MAX] {
        assert_eq!(
            build_tx_extension(nonce).7.encode(),
            codec::Compact(nonce).encode()
        );
    }
}

#[test]
fn malformed_or_truncated_runtime_call_never_becomes_signing_material() {
    for input in [vec![], vec![0], vec![0xff, 0xff], vec![0, 0, 0xff]] {
        assert!(build_signing_material(&input, &[1; 32], 0, 1, 1).is_err());
    }
}
