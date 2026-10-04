//! Rust host 端链交易签名材料唯一真源。
//!
//! 本 crate 负责 Substrate extrinsic 的 `TxExtension`、`SignedPayload` 和
//! `UncheckedExtrinsic` 组装；`runtime/primitives::sign` 仍负责 QR/op_tag 业务签名域。

use citizenchain as runtime;
use codec::{Decode, Encode};
use sha2::{Digest, Sha256};
use sp_core::{sr25519, Pair, H256};
use sp_runtime::{generic::Era, traits::IdentifyAccount, AccountId32, MultiAddress, MultiSigner};

/// 链交易签名材料，供 QR 冷签和本机热签共用。
pub struct SigningMaterial {
    pub call: runtime::RuntimeCall,
    pub tx_ext: runtime::TxExtension,
    /// QR `b.d` 必须携带的完整审阅载荷。
    ///
    /// 这里不能调用 `SignedPayload::encode()` 取值：Substrate 在 payload
    /// 超过 256 字节时会把 Encode/using_encoded 结果替换成 32 字节
    /// blake2_256。QR 展示需要完整 payload，签名才使用下方 `signing_bytes`。
    pub payload: Vec<u8>,
    /// sr25519 实际签名输入，严格复用 Substrate `SignedPayload::using_encoded` 规则。
    pub signing_bytes: Vec<u8>,
}

/// 解码 runtime call，并拒绝尾随字节。
pub fn decode_runtime_call(call_data: &[u8]) -> Result<runtime::RuntimeCall, String> {
    let mut input = call_data;
    let call = runtime::RuntimeCall::decode(&mut input)
        .map_err(|e| format!("call_data 不是当前 runtime 可解码调用: {e}"))?;
    if !input.is_empty() {
        return Err(format!("call_data 存在 {} 字节尾随数据", input.len()));
    }
    Ok(call)
}

/// 构建 runtime 定义的交易扩展，顺序必须与 `Runtime::TxExtension` 一致。
pub fn build_tx_extension(nonce: u32) -> runtime::TxExtension {
    (
        frame_system::AuthorizeCall::<runtime::Runtime>::new(),
        frame_system::CheckNonZeroSender::<runtime::Runtime>::new(),
        runtime::CheckNonStakeSender,
        frame_system::CheckSpecVersion::<runtime::Runtime>::new(),
        frame_system::CheckTxVersion::<runtime::Runtime>::new(),
        frame_system::CheckGenesis::<runtime::Runtime>::new(),
        frame_system::CheckEra::<runtime::Runtime>::from(Era::Immortal),
        frame_system::CheckNonce::<runtime::Runtime>::from(nonce),
        frame_system::CheckWeight::<runtime::Runtime>::new(),
        // tip 不属于 CitizenChain 五类费用，唯一协议值固定为零。
        pallet_transaction_payment::ChargeTransactionPayment::<runtime::Runtime>::from(
            primitives::fee_policy::TRANSACTION_TIP,
        ),
        frame_metadata_hash_extension::CheckMetadataHash::<runtime::Runtime>::new(false),
        frame_system::WeightReclaim::<runtime::Runtime>::new(),
    )
}

/// 从 call_data 构建完整签名材料。
pub fn build_signing_material(
    call_data: &[u8],
    genesis_hash: &[u8; 32],
    nonce: u32,
    spec_version: u32,
    tx_version: u32,
) -> Result<SigningMaterial, String> {
    let call = decode_runtime_call(call_data)?;
    Ok(build_signing_material_from_call(
        call,
        H256::from_slice(genesis_hash),
        nonce,
        spec_version,
        tx_version,
    ))
}

/// 从已解码 call 构建完整签名材料。
pub fn build_signing_material_from_call(
    call: runtime::RuntimeCall,
    genesis_hash: H256,
    nonce: u32,
    spec_version: u32,
    tx_version: u32,
) -> SigningMaterial {
    let tx_ext = build_tx_extension(nonce);
    let additional_signed = (
        (),
        (),
        (),
        spec_version,
        tx_version,
        genesis_hash,
        genesis_hash,
        (),
        (),
        (),
        None,
        (),
    );
    // 审阅载荷是 SignedPayload 的原始三元组 SCALE 字节，用于钱包完整解码和中文展示。
    // 签名字节另走 `using_encoded`，保留 Substrate 对 >256B payload 签 hash 的规则。
    let review_payload = (call.clone(), tx_ext.clone(), additional_signed).encode();
    let raw_payload =
        runtime::SignedPayload::from_raw(call.clone(), tx_ext.clone(), additional_signed);

    SigningMaterial {
        call,
        tx_ext,
        payload: review_payload,
        signing_bytes: raw_payload.using_encoded(|payload| payload.to_vec()),
    }
}

/// 构建 QR 冷签需要的完整审阅 payload 和实际 sr25519 签名字节。
pub fn build_signing_payloads(
    call_data: &[u8],
    genesis_hash: &[u8; 32],
    nonce: u32,
    spec_version: u32,
    tx_version: u32,
) -> Result<(Vec<u8>, Vec<u8>), String> {
    let material =
        build_signing_material(call_data, genesis_hash, nonce, spec_version, tx_version)?;
    Ok((material.payload, material.signing_bytes))
}

/// sha256 hex，用于会话校验和日志定位。
pub fn sha256_hex(data: &[u8]) -> String {
    hex::encode(Sha256::digest(data))
}

/// 解析跨端唯一格式的 32 字节 sr25519 公钥：小写 `0x` + 64 位十六进制。
pub fn parse_sr25519_public_key(public_key: &str) -> Result<sr25519::Public, String> {
    if public_key.len() != 66
        || !public_key.starts_with("0x")
        || !public_key[2..]
            .chars()
            .all(|ch| ch.is_ascii_digit() || ('a'..='f').contains(&ch))
    {
        return Err("公钥格式无效，应为小写 0x + 64 位十六进制".to_string());
    }
    let raw = hex::decode(&public_key[2..]).map_err(|e| format!("公钥解码失败: {e}"))?;
    let bytes = <[u8; 32]>::try_from(raw.as_slice()).map_err(|_| "公钥必须 32 字节")?;
    Ok(sr25519::Public::from_raw(bytes))
}

/// 解析 64 字节 sr25519 签名 hex。
pub fn parse_sr25519_signature_hex(signature_hex: &str) -> Result<sr25519::Signature, String> {
    let raw = hex::decode(signature_hex.trim_start_matches("0x"))
        .map_err(|e| format!("签名解码失败: {e}"))?;
    let bytes = <[u8; 64]>::try_from(raw.as_slice()).map_err(|_| "签名必须 64 字节")?;
    Ok(sr25519::Signature::from_raw(bytes))
}

/// 使用共享签名字节本地验签。
pub fn verify_signature(
    material: &SigningMaterial,
    signature: &sr25519::Signature,
    public: &sr25519::Public,
) -> bool {
    sr25519::Pair::verify(signature, &material.signing_bytes, public)
}

/// sr25519 公钥对应的 AccountId。
pub fn account_id_from_public(public: sr25519::Public) -> AccountId32 {
    MultiSigner::from(public).into_account()
}

/// 组装 signed extrinsic。
pub fn assemble_signed_extrinsic(
    material: SigningMaterial,
    public: sr25519::Public,
    signature: sr25519::Signature,
) -> runtime::UncheckedExtrinsic {
    runtime::UncheckedExtrinsic::new_signed(
        material.call,
        MultiAddress::Id(account_id_from_public(public)),
        runtime::Signature::Sr25519(signature),
        material.tx_ext,
    )
}

/// signed extrinsic 的 0x-prefixed SCALE hex。
pub fn signed_extrinsic_hex(extrinsic: &runtime::UncheckedExtrinsic) -> String {
    format!("0x{}", hex::encode(extrinsic.encode()))
}

/// 用 sr25519 pair 对共享签名字节签名。
pub fn sign_material_with_pair(
    material: &SigningMaterial,
    sender: &sr25519::Pair,
) -> sr25519::Signature {
    sender.sign(&material.signing_bytes)
}

/// 使用指定 runtime 版本构建并签名 extrinsic。
pub fn build_signed_extrinsic_with_pair(
    call: runtime::RuntimeCall,
    genesis_hash: H256,
    nonce: u32,
    spec_version: u32,
    tx_version: u32,
    sender: &sr25519::Pair,
) -> runtime::UncheckedExtrinsic {
    let material =
        build_signing_material_from_call(call, genesis_hash, nonce, spec_version, tx_version);
    let public = sender.public();
    let signature = sign_material_with_pair(&material, sender);
    assemble_signed_extrinsic(material, public, signature)
}

/// 使用本机 runtime 版本构建并签名 extrinsic，供 benchmark/本地结算沿用既有语义。
pub fn build_signed_extrinsic_local(
    call: runtime::RuntimeCall,
    genesis_hash: H256,
    nonce: u32,
    sender: &sr25519::Pair,
) -> runtime::UncheckedExtrinsic {
    build_signed_extrinsic_with_pair(
        call,
        genesis_hash,
        nonce,
        runtime::VERSION.spec_version,
        runtime::VERSION.transaction_version,
        sender,
    )
}

/// 解析 dry-run 的 TransactionValidityError。
pub fn classify_invalid_tx(result_bytes: &[u8]) -> String {
    if result_bytes.len() > 1 && result_bytes[1] == 0x00 {
        let kind = match result_bytes.get(2).copied().unwrap_or(0xff) {
            0 => "Call(当前链状态下不可调度)",
            1 => "Payment(余额不足以支付手续费)",
            2 => "Future(nonce 超前，交易会卡在 future 队列永不出块)",
            3 => "Stale(nonce 已被消费，交易过期)",
            4 => "BadProof(签名校验失败)",
            5 => "AncientBirthBlock(签名时代过旧)",
            6 => "ExhaustsResources(资源超限，请稍后重试)",
            7 => "Custom(自定义交易校验失败)",
            8 => "BadMandatory(强制交易无效)",
            9 => "MandatoryValidation(强制校验失败)",
            10 => "BadSigner(签名账户无效)",
            11 => "IndeterminateImplicit(隐式校验数据不确定)",
            12 => "UnknownOrigin(交易来源未知)",
            _ => "Unknown",
        };
        format!("InvalidTransaction::{kind}")
    } else {
        "UnknownTransaction".to_string()
    }
}

/// 把 dry-run 拒绝结果转为前端报错。
pub fn dry_run_reject_message(result_bytes: &[u8], raw_hex: &str) -> String {
    if result_bytes.starts_with(&[0x01, 0x00, 0x02]) {
        return "上一笔交易尚未出块，请稍候再试".to_string();
    }
    let reason = classify_invalid_tx(result_bytes);
    format!("交易校验失败，已拒绝提交: {reason} (hex: {raw_hex})")
}

/// 把 dry-run RPC 层失败转为中文预检拒绝。
///
/// 这里覆盖 RuntimeApi / wasm trap 等运行时校验崩溃；调用方不得继续
/// `author_submitExtrinsic`，否则同一错误会在交易池终审阶段再次冒出。
pub fn preflight_reject_message(error: &str) -> String {
    if error.contains("RuntimeApi")
        || error.contains("wasm trap")
        || error.contains("unreachable")
        || error.contains("TaggedTransactionQueue_validate_transaction")
    {
        return format!("链运行时校验失败，交易未提交: {error}");
    }
    format!("链交易预检失败，交易未提交: {error}")
}

#[cfg(test)]
// 签名材料夹具异常必须立即中止测试，断言式解包仅限本测试模块。
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn signing_material_roundtrip_and_hash_rule() {
        let call = runtime::RuntimeCall::System(frame_system::Call::remark {
            remark: vec![7u8; 8],
        });
        let call_data = call.encode();
        let genesis = [9u8; 32];
        let m = build_signing_material(&call_data, &genesis, 5, 1, 1).expect("material");
        assert_eq!(m.call.encode(), call_data);
        // 小载荷:签名输入 == 审阅 payload 本体。
        assert!(m.payload.len() <= 256);
        assert_eq!(m.signing_bytes, m.payload);

        let big_call = runtime::RuntimeCall::System(frame_system::Call::remark {
            remark: vec![7u8; 400],
        });
        let big = build_signing_material(&big_call.encode(), &genesis, 5, 1, 1).expect("material");
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
        assert!(decode_runtime_call(&data).is_err());
    }

    #[test]
    fn public_key_requires_prefixed_lowercase_hex() {
        let valid = format!("0x{}", "ab".repeat(32));
        assert!(parse_sr25519_public_key(&valid).is_ok());
        assert!(parse_sr25519_public_key(&"ab".repeat(32)).is_err());
        assert!(parse_sr25519_public_key(&format!("0x{}", "AB".repeat(32))).is_err());
    }

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
}
