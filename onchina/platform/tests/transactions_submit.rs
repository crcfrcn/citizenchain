//! 随所属功能迁移的原有回归，输入与断言保持不变。
use codec::Encode;

/// 材料构建离线自洽:同输入同产物,call 解码回等值,>256B 审阅载荷仍完整保留。
#[test]
fn signing_material_roundtrip_and_hash_rule() {
    let call = runtime::RuntimeCall::System(frame_system::Call::remark {
        remark: vec![7u8; 8],
    });
    let call_data = call.encode();
    let genesis = [9u8; 32];
    let m = chain_signing::build_signing_material(&call_data, &genesis, 5, 1, 1).expect("material");
    assert_eq!(m.call.encode(), call_data);
    // 小载荷:签名输入 == 审阅 payload 本体。
    assert!(m.payload.len() <= 256);
    assert_eq!(m.signing_bytes, m.payload);

    let big_call = runtime::RuntimeCall::System(frame_system::Call::remark {
        remark: vec![7u8; 400],
    });
    let big = chain_signing::build_signing_material(&big_call.encode(), &genesis, 5, 1, 1)
        .expect("material");
    // 大载荷:QR 仍必须拿到完整审阅 payload；只有实际签名输入是 32 字节 blake2_256。
    assert!(big.payload.len() > 256);
    assert_eq!(big.signing_bytes.len(), 32);
    assert_ne!(big.payload, big.signing_bytes);
}

#[test]
fn tail_data_in_call_is_rejected() {
    let call = runtime::RuntimeCall::System(frame_system::Call::remark { remark: vec![] });
    let mut data = call.encode();
    data.push(0xff);
    assert!(chain_signing::decode_runtime_call(&data).is_err());
}

#[test]
fn only_post_submit_uncertainty_carries_recoverable_tx_hash() {
    let rejected = super::ChainSubmitError::from("dry-run rejected".to_string());
    assert_eq!(rejected.submitted_tx_hash(), None);

    let uncertain = super::ChainSubmitError::submitted_unconfirmed(
        "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
        "subscription dropped".to_string(),
    );
    assert_eq!(
        uncertain.submitted_tx_hash(),
        Some("0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
    );
}
