//! 原所属签名回归，输入和断言保持。
#![allow(clippy::expect_used, clippy::unwrap_used)]
use chain_signing::*;
use codec::Encode;
use sp_core::H256;
use sp_runtime::generic::Era;

/// 与接入前十二项扩展逐字节比较，兼容既有钱包/QR原生交易。
#[test]
fn ethereum_origin_marker_keeps_native_extension_bytes_unchanged() {
    let legacy = (
        frame_system::AuthorizeCall::<runtime::Runtime>::new(),
        frame_system::CheckNonZeroSender::<runtime::Runtime>::new(),
        runtime::CheckNonStakeSender,
        frame_system::CheckSpecVersion::<runtime::Runtime>::new(),
        frame_system::CheckTxVersion::<runtime::Runtime>::new(),
        frame_system::CheckGenesis::<runtime::Runtime>::new(),
        frame_system::CheckEra::<runtime::Runtime>::from(Era::Immortal),
        frame_system::CheckNonce::<runtime::Runtime>::from(5),
        frame_system::CheckWeight::<runtime::Runtime>::new(),
        pallet_transaction_payment::ChargeTransactionPayment::<runtime::Runtime>::from(0),
        frame_metadata_hash_extension::CheckMetadataHash::<runtime::Runtime>::new(false),
        frame_system::WeightReclaim::<runtime::Runtime>::new(),
    );
    assert_eq!(build_tx_extension(5).encode(), legacy.encode());
    let call = runtime::RuntimeCall::System(frame_system::Call::remark { remark: vec![7; 8] });
    let additional = (
        (),
        (),
        (),
        1u32,
        1u32,
        H256::repeat_byte(9),
        H256::repeat_byte(9),
        (),
        (),
        (),
        None::<[u8; 32]>,
        (),
    );
    let legacy_payload = (call.clone(), legacy, additional).encode();
    let material = build_signing_material_from_call(call, H256::repeat_byte(9), 5, 1, 1);
    assert_eq!(material.payload, legacy_payload);
    assert_eq!(material.signing_bytes, legacy_payload);
}

#[test]
fn signed_extrinsic_roundtrips_using_the_runtime_decoder() {
    use codec::Decode;
    use sp_core::{sr25519, Pair};
    let pair = sr25519::Pair::from_seed(&[4; 32]);
    for length in [1, 400] {
        let call = runtime::RuntimeCall::System(frame_system::Call::remark {
            remark: vec![7; length],
        });
        let material =
            build_signing_material_from_call(call, H256::repeat_byte(3), u32::MAX, 19, 5);
        let signature = sign_material_with_pair(&material, &pair);
        assert!(verify_signature(&material, &signature, &pair.public()));
        let extrinsic = assemble_signed_extrinsic(material, pair.public(), signature);
        let text = signed_extrinsic_hex(&extrinsic);
        let bytes = hex::decode(text.strip_prefix("0x").expect("规范前缀")).expect("合法hex");
        let mut input = bytes.as_slice();
        let decoded = runtime::UncheckedExtrinsic::decode(&mut input).expect("真实Runtime可解码");
        assert!(input.is_empty());
        assert_eq!(decoded.encode(), bytes);
    }
}
