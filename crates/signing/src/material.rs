//! 链交易扩展与完整审阅载荷。
use codec::{Decode, Encode};
use sp_core::H256;
use sp_runtime::generic::Era;

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
        // 末两项与Runtime嵌套组合一致；两个空编码扩展保持原生签名材料不变。
        (
            Default::default(),
            frame_system::WeightReclaim::<runtime::Runtime>::new(),
        ),
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
        ((), ()),
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
