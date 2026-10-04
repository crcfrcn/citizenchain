// 治理投票 QR 签名：构建 QR_V1 签名请求、验证响应、提交 extrinsic。
//
// 协议流程：
// 1. 后端构建未签名 review_payload + QR 请求 JSON
// 2. 前端显示 QR 码 → 用户用 citizenwallet 离线设备扫码签名
// 3. 前端摄像头扫描响应 QR → 传回后端
// 4. 后端按本地 session 校验 request id 与签名公钥 → 构建 signed extrinsic → 提交到链

use crate::shared::rpc;
use codec::Encode;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

pub(crate) const QR_V1: &str = "QR_V1";
pub(crate) const DEFAULT_TTL_SECS: u64 = 90;
const RPC_REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
/// 提交后给本地交易池更新 next-index 的短暂观察延迟，与 PoW 出块时间无关。
const POST_SUBMIT_AUDIT_DELAY: Duration = Duration::from_secs(10);
use crate::shared::constants::RPC_RESPONSE_LIMIT_SMALL;
/// SS58 前缀 2027。
const SS58_PREFIX: u16 = 2027;
/// 冷签交易统一使用 immortal era，前端保留的 sign_block_number 固定回传 0。
pub(crate) const IMMORTAL_SIGN_BLOCK_NUMBER: u64 = 0;

pub(crate) const QR_KIND_SIGN_REQUEST: u8 = primitives::sign::QR_KIND_SIGN_REQUEST;
pub(crate) const QR_KIND_SIGN_RESPONSE: u8 = primitives::sign::QR_KIND_SIGN_RESPONSE;

#[derive(Debug, Clone)]
struct ChainSignSession {
    expected_signer_public_key: String,
    call_data_hex: String,
    /// QR `b.d` 携带的完整审阅载荷 SHA-256。
    payload_hash_hex: String,
    /// sr25519 实际签名输入 SHA-256，用于提交前重建校验。
    signing_payload_hash_hex: String,
    nonce: u32,
    expires_at: u64,
}

static CHAIN_SIGN_SESSIONS: OnceLock<Mutex<HashMap<String, ChainSignSession>>> = OnceLock::new();

fn chain_sign_sessions() -> &'static Mutex<HashMap<String, ChainSignSession>> {
    CHAIN_SIGN_SESSIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

pub(crate) fn chain_action_code(call_data: &[u8]) -> Result<u16, String> {
    if call_data.len() < 2 {
        return Err("call_data 至少需要 pallet/call 两字节".to_string());
    }
    Ok(((call_data[0] as u16) << 8) | call_data[1] as u16)
}

// QR body 定长字段的 base64url 编解码统一走 qr-protocol 单源,
// 本文件不再自带第二份实现(与 onchina 分裂会导致同一二维码一端过一端拒)。
pub(crate) fn public_key_b64(signer_public_key_bytes: &[u8]) -> Result<String, String> {
    qr_protocol::public_key_b64(signer_public_key_bytes, "b.u").map_err(|e| e.to_string())
}

pub(crate) fn payload_b64(payload: &[u8]) -> String {
    qr_protocol::bytes_to_b64(payload)
}

fn deserialize_b64_signer_public_key<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = String::deserialize(deserializer)?;
    qr_protocol::b64_to_prefixed_hex(&value, qr_protocol::PUBLIC_KEY_BYTES, "b.u")
        .map_err(serde::de::Error::custom)
}

fn deserialize_b64_signature<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = String::deserialize(deserializer)?;
    qr_protocol::b64_to_prefixed_hex(&value, qr_protocol::SIGNATURE_BYTES, "b.s")
        .map_err(serde::de::Error::custom)
}

fn remember_chain_sign_session(
    request_id: String,
    session: ChainSignSession,
) -> Result<(), String> {
    let mut sessions = chain_sign_sessions()
        .lock()
        .map_err(|_| "链交易签名 session 状态异常".to_string())?;
    let now = now_secs()?;
    sessions.retain(|_, item| item.expires_at >= now);
    sessions.insert(request_id, session);
    Ok(())
}

pub(crate) fn remember_chain_sign_request_session(
    request_id: &str,
    expected_signer_public_key: &str,
    call_data: &[u8],
    payload_hash_hex: &str,
    signing_payload_hash_hex: &str,
    nonce: u32,
    expires_at: u64,
) -> Result<(), String> {
    remember_chain_sign_session(
        request_id.to_string(),
        ChainSignSession {
            expected_signer_public_key: crate::shared::validation::normalize_public_key(
                expected_signer_public_key,
            )?,
            call_data_hex: hex::encode(call_data),
            payload_hash_hex: normalize_hash_hex(payload_hash_hex, "payload_hash")?,
            signing_payload_hash_hex: normalize_hash_hex(
                signing_payload_hash_hex,
                "signing_payload_hash",
            )?,
            nonce,
            expires_at,
        },
    )
}

fn take_chain_sign_session(request_id: &str) -> Result<ChainSignSession, String> {
    let mut sessions = chain_sign_sessions()
        .lock()
        .map_err(|_| "链交易签名 session 状态异常".to_string())?;
    sessions
        .remove(request_id)
        .ok_or_else(|| "未找到本地签名 session，请重新生成二维码".to_string())
}

fn normalize_hash_hex(value: &str, field: &str) -> Result<String, String> {
    let clean = value
        .trim()
        .strip_prefix("0x")
        .unwrap_or(value.trim())
        .to_ascii_lowercase();
    if clean.len() != 64 || !clean.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(format!("{field} 必须是 32 字节 hex"));
    }
    Ok(clean)
}

/// 金额格式化：带千分位逗号，保留 2 位小数。
pub(crate) fn format_amount(yuan: f64) -> String {
    let fixed = format!("{:.2}", yuan);
    let parts: Vec<&str> = fixed.split('.').collect();
    let int_part = parts[0];
    let dec_part = parts.get(1).unwrap_or(&"00");
    let negative = int_part.starts_with('-');
    let digits: &str = if negative { &int_part[1..] } else { int_part };
    let mut result = String::new();
    for (i, ch) in digits.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(ch);
    }
    let formatted: String = result.chars().rev().collect();
    if negative {
        format!("-{}.{}", formatted, dec_part)
    } else {
        format!("{}.{}", formatted, dec_part)
    }
}

// ──── QR 协议数据结构 ────

/// QR_V1/k=1 签名请求 body(节点桌面端 → 离线设备)。
#[derive(Debug, Serialize)]
pub struct SignRequestBody {
    /// a:链交易动作码,固定为 `(pallet_index << 8) | call_index`。
    #[serde(rename = "a")]
    pub action: u16,
    /// g:签名算法,1 固定为 sr25519。
    #[serde(rename = "g")]
    pub sig_alg: u8,
    /// u:签名账户 32B 公钥,base64url(no padding)。
    #[serde(rename = "u")]
    pub signer_public_key: String,
    /// d:完整 review_payload bytes,base64url(no padding)。
    ///
    /// 普通链交易必须可被钱包完整解码和中文展示；32B signing bytes 仅 Runtime 升级
    /// hash-only 请求允许进入 QR。
    #[serde(rename = "d")]
    pub payload: String,
}

/// QR_V1/k=1 sign_request envelope。
#[derive(Debug, Serialize)]
pub struct QrSignRequest {
    #[serde(rename = "p")]
    pub proto: String,
    #[serde(rename = "k")]
    pub kind: u8,
    #[serde(rename = "i")]
    pub id: String,
    #[serde(rename = "e")]
    pub expires_at: u64,
    #[serde(rename = "b")]
    pub body: SignRequestBody,
}

/// QR_V1/k=2 签名响应 body(离线设备 → 节点桌面端)。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignResponseBody {
    #[serde(rename = "u", deserialize_with = "deserialize_b64_signer_public_key")]
    pub signer_public_key: String,
    #[serde(rename = "s", deserialize_with = "deserialize_b64_signature")]
    pub signature: String,
}

/// QR_V1/k=2 sign_response envelope。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QrSignResponse {
    #[serde(rename = "p")]
    pub proto: String,
    #[serde(rename = "k")]
    pub kind: u8,
    #[serde(rename = "i")]
    pub id: String,
    #[serde(rename = "e")]
    pub expires_at: u64,
    #[serde(rename = "b")]
    pub body: SignResponseBody,
}

/// 构建投票签名请求的结果。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoteSignRequestResult {
    /// 完整的 QR 签名请求 JSON 字符串。
    pub request_json: String,
    /// 后端构造的完整 call data hex（不含 0x）。
    pub call_data_hex: String,
    /// 请求 ID（用于后续验证匹配）。
    pub request_id: String,
    /// QR 审阅 payload 的 SHA-256 哈希（用于验证响应）。
    pub expected_payload_hash: String,
    /// 签名时使用的 nonce（提交时必须复用）。
    pub sign_nonce: u32,
    /// 冷签交易统一使用 immortal era；该字段保留给前端会话，固定为 0。
    pub sign_block_number: u64,
}

/// 投票提交结果。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoteSubmitResult {
    pub tx_hash: String,
}

// ──── 公开函数 ────

/// 构建内部投票（`internal_vote`）签名请求。
///
/// 内部投票统一走 `InternalVote::cast`(pallet=20, call=0)；个人多签使用
/// `Personal` 票据声明，机构使用 `InstitutionRole(role_code)` 票据声明。
/// 由投票引擎按 ProposalData 前缀自动分派到对应 `InternalVoteExecutor`。
///
/// Call 编码: `[0x14][0x00][proposal_id:u64_le][ticket_claim][approve:bool]`。
///
/// 返回 QR 签名请求 JSON + 请求 ID + 预期审阅 payload hash。
pub fn build_vote_sign_request(
    proposal_id: u64,
    signer_public_key: &str,
    voter_role_code: Option<&str>,
    approve: bool,
) -> Result<VoteSignRequestResult, String> {
    // 验证公钥格式
    let signer_public_key = crate::shared::validation::normalize_public_key(signer_public_key)?;
    let signer_public_key_bytes = hex::decode(signer_public_key.trim_start_matches("0x"))
        .map_err(|e| format!("公钥解码失败: {e}"))?;
    let signer_account_id = signer_account_id_from_public_key(&signer_public_key)?;

    // 获取链上参数
    let (spec_version, tx_version) = fetch_runtime_version()?;
    let genesis_hash = fetch_genesis_hash()?;
    let nonce = fetch_nonce(&signer_account_id)?;

    // ticket_claim SCALE：Personal=0；InstitutionRole=1 + RoleCode(BoundedVec)。
    let mut call_data = Vec::with_capacity(12 + voter_role_code.map(str::len).unwrap_or_default());
    call_data.push(20u8); // InternalVote sub-pallet index
    call_data.push(0u8); // cast call index
    call_data.extend_from_slice(&proposal_id.to_le_bytes());
    match voter_role_code {
        Some(role_code) => {
            if role_code.is_empty()
                || role_code.len() > entity_primitives::INSTITUTION_ROLE_CODE_MAX_BYTES as usize
            {
                return Err("voter_role_code 长度超出链上岗位码范围".to_string());
            }
            call_data.push(1u8);
            call_data.extend_from_slice(&encode_compact_u32(role_code.len() as u32));
            call_data.extend_from_slice(role_code.as_bytes());
        }
        None => call_data.push(0u8),
    }
    call_data.push(if approve { 1u8 } else { 0u8 });

    // 链交易签名材料只能由 runtime 类型构造，避免 node 冷签与热钱包
    // 交易路径在 TxExtension 或 additional_signed 字节上分叉。
    let (payload, signing_bytes) =
        build_signing_payloads(&call_data, &genesis_hash, nonce, spec_version, tx_version)?;

    // 计算审阅 payload hash 与实际签名字节 hash，分别用于 QR 会话和提交校验。
    let payload_hash = sha256_hash(&payload);
    let payload_hash_hex = hex::encode(payload_hash);
    let signing_payload_hash_hex = hex::encode(sha256_hash(&signing_bytes));

    // 生成请求 ID
    let request_id = generate_request_id("vote");

    let now = now_secs()?;
    let expires_at = now + DEFAULT_TTL_SECS;
    let request = QrSignRequest {
        proto: QR_V1.to_string(),
        kind: QR_KIND_SIGN_REQUEST,
        id: request_id.clone(),
        expires_at,
        body: SignRequestBody {
            action: chain_action_code(&call_data)?,
            sig_alg: 1,
            signer_public_key: public_key_b64(&signer_public_key_bytes)?,
            payload: payload_b64(&payload),
        },
    };

    let request_json =
        serde_json::to_string(&request).map_err(|e| format!("序列化签名请求失败: {e}"))?;
    remember_chain_sign_session(
        request_id.clone(),
        ChainSignSession {
            expected_signer_public_key: signer_public_key,
            call_data_hex: hex::encode(&call_data),
            payload_hash_hex: payload_hash_hex.clone(),
            signing_payload_hash_hex,
            nonce,
            expires_at,
        },
    )?;

    Ok(VoteSignRequestResult {
        request_json,
        call_data_hex: hex::encode(&call_data),
        request_id,
        expected_payload_hash: format!("0x{}", payload_hash_hex),
        sign_nonce: nonce,
        sign_block_number: IMMORTAL_SIGN_BLOCK_NUMBER,
    })
}

/// 构建 joint_vote 签名请求（联合投票内部投票阶段：pallet=21, call=0）。
///
/// JointVote pallet:`cast_admin` 在 21.0,
/// `cast_referendum` 在 21.1(联合公投阶段需双层凭证,本函数不覆盖)。
///
/// `actor_cid_number` 是联合投票机构身份的唯一主键，不派生或附带主账户。
pub fn build_joint_vote_sign_request(
    proposal_id: u64,
    signer_public_key: &str,
    actor_cid_number: &str,
    voter_role_code: &str,
    approve: bool,
) -> Result<VoteSignRequestResult, String> {
    let signer_public_key = crate::shared::validation::normalize_public_key(signer_public_key)?;
    let signer_public_key_bytes = hex::decode(signer_public_key.trim_start_matches("0x"))
        .map_err(|e| format!("公钥解码失败: {e}"))?;
    let signer_account_id = signer_account_id_from_public_key(&signer_public_key)?;

    if actor_cid_number.is_empty()
        || actor_cid_number.len() > primitives::core_const::CID_NUMBER_MAX_BYTES as usize
    {
        return Err("actor_cid_number 长度必须在链上 CID_NUMBER_MAX_BYTES 范围内".to_string());
    }
    if voter_role_code.is_empty()
        || voter_role_code.len() > entity_primitives::INSTITUTION_ROLE_CODE_MAX_BYTES as usize
    {
        return Err("voter_role_code 长度超出链上岗位码范围".to_string());
    }

    let (spec_version, tx_version) = fetch_runtime_version()?;
    let genesis_hash = fetch_genesis_hash()?;
    let nonce = fetch_nonce(&signer_account_id)?;

    // call data: [21][0][proposal_id][cid_number][voter_role_code][approve]
    let mut call_data = Vec::with_capacity(14 + actor_cid_number.len() + voter_role_code.len());
    call_data.push(21u8); // JointVote sub-pallet index
    call_data.push(0u8); // cast_admin call index
    call_data.extend_from_slice(&proposal_id.to_le_bytes());
    call_data.extend_from_slice(&encode_compact_u32(actor_cid_number.len() as u32));
    call_data.extend_from_slice(actor_cid_number.as_bytes());
    call_data.extend_from_slice(&encode_compact_u32(voter_role_code.len() as u32));
    call_data.extend_from_slice(voter_role_code.as_bytes());
    call_data.push(if approve { 1u8 } else { 0u8 });

    let (payload, signing_bytes) =
        build_signing_payloads(&call_data, &genesis_hash, nonce, spec_version, tx_version)?;
    let payload_hash = sha256_hash(&payload);
    let payload_hash_hex = hex::encode(payload_hash);
    let signing_payload_hash_hex = hex::encode(sha256_hash(&signing_bytes));
    let request_id = generate_request_id("jvote");

    let now = now_secs()?;
    let expires_at = now + DEFAULT_TTL_SECS;
    let request = QrSignRequest {
        proto: QR_V1.to_string(),
        kind: QR_KIND_SIGN_REQUEST,
        id: request_id.clone(),
        expires_at,
        body: SignRequestBody {
            action: chain_action_code(&call_data)?,
            sig_alg: 1,
            signer_public_key: public_key_b64(&signer_public_key_bytes)?,
            payload: payload_b64(&payload),
        },
    };

    let request_json =
        serde_json::to_string(&request).map_err(|e| format!("序列化签名请求失败: {e}"))?;
    remember_chain_sign_session(
        request_id.clone(),
        ChainSignSession {
            expected_signer_public_key: signer_public_key,
            call_data_hex: hex::encode(&call_data),
            payload_hash_hex: payload_hash_hex.clone(),
            signing_payload_hash_hex,
            nonce,
            expires_at,
        },
    )?;

    Ok(VoteSignRequestResult {
        request_json,
        call_data_hex: hex::encode(&call_data),
        request_id,
        expected_payload_hash: format!("0x{}", payload_hash_hex),
        sign_nonce: nonce,
        sign_block_number: IMMORTAL_SIGN_BLOCK_NUMBER,
    })
}

/// Compact<u32> 编码（公开版本，供 mod.rs 调用）。
pub fn encode_compact_u32_pub(value: u32) -> Vec<u8> {
    encode_compact_u32(value)
}

/// 从 SS58 展示地址解码 32 字节账户 ID。
pub fn account_id_from_ss58_address(ss58_address: &str) -> Result<[u8; 32], String> {
    let data = bs58::decode(ss58_address)
        .into_vec()
        .map_err(|_| "SS58 地址解码失败".to_string())?;
    let (prefix, prefix_len) = crate::settings::address_utils::decode_ss58_prefix(&data)?;
    if prefix != SS58_PREFIX {
        return Err(format!("SS58 地址前缀无效，期望 2027，实际 {prefix}"));
    }
    if data.len() < prefix_len + 32 + 2 {
        return Err("SS58 地址长度无效".to_string());
    }
    let (without_checksum, checksum) = data.split_at(data.len() - 2);
    let hash = blake2b_simd::Params::new()
        .hash_length(64)
        .to_state()
        .update(b"SS58PRE")
        .update(without_checksum)
        .finalize();
    if checksum != &hash.as_bytes()[..2] {
        return Err("SS58 地址校验和无效".to_string());
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(&data[prefix_len..prefix_len + 32]);
    Ok(out)
}

/// 验证签名响应并提交 extrinsic（通用，支持 vote_transfer 和 joint_vote）。
///
/// call_data 由调用方根据投票类型构建。
pub fn verify_and_submit(
    request_id: &str,
    expected_signer_public_key: &str,
    expected_payload_hash: &str,
    call_data: &[u8],
    sign_nonce: u32,
    sign_block_number: u64,
    response_json: &str,
) -> Result<VoteSubmitResult, String> {
    // 解析响应
    let response: QrSignResponse =
        serde_json::from_str(response_json).map_err(|e| format!("解析签名响应失败: {e}"))?;

    // 验证协议版本
    if response.proto != QR_V1 {
        return Err(format!(
            "协议版本不匹配：期望 {QR_V1}，实际 {}",
            response.proto
        ));
    }
    if response.kind != QR_KIND_SIGN_RESPONSE {
        return Err(format!(
            "二维码类型不匹配：期望 k={QR_KIND_SIGN_RESPONSE}，实际 k={}",
            response.kind
        ));
    }
    // 验证请求 ID 匹配
    if response.id != request_id {
        return Err("请求 ID 不匹配,可能扫描了其他交易的签名".to_string());
    }
    let session = take_chain_sign_session(request_id)?;
    let now = now_secs()?;
    if session.expires_at < now {
        return Err("签名 session 已过期，请重新生成二维码".to_string());
    }
    if response.expires_at != session.expires_at {
        return Err("签名响应过期时间与本地 session 不匹配".to_string());
    }

    // 验证公钥匹配
    let expected_signer_public_key =
        crate::shared::validation::normalize_public_key(expected_signer_public_key)?;
    if session.expected_signer_public_key != expected_signer_public_key {
        return Err("提交参数公钥与本地签名 session 不匹配".to_string());
    }
    if response.body.signer_public_key != expected_signer_public_key {
        return Err("公钥不匹配".to_string());
    }

    let expected_payload_hash_clean =
        normalize_hash_hex(expected_payload_hash, "expected_payload_hash")?;
    if expected_payload_hash_clean != session.payload_hash_hex
        && expected_payload_hash_clean != session.signing_payload_hash_hex
    {
        return Err("提交参数 payload hash 与本地签名 session 不匹配".to_string());
    }
    if session.nonce != sign_nonce {
        return Err("提交参数 nonce 与本地签名 session 不匹配".to_string());
    }
    let call_data_hex = hex::encode(call_data);
    if session.call_data_hex != call_data_hex {
        return Err("提交参数 call_data 与本地签名 session 不匹配".to_string());
    }

    let public = chain_signing::parse_sr25519_public_key(&expected_signer_public_key)?;
    let signature = chain_signing::parse_sr25519_signature_hex(&response.body.signature)?;

    let (spec_version, tx_version) = fetch_runtime_version()?;
    let genesis_hash = fetch_genesis_hash()?;
    let material = chain_signing::build_signing_material(
        call_data,
        &genesis_hash,
        sign_nonce,
        spec_version,
        tx_version,
    )?;
    let payload_hash = hex::encode(sha256_hash(&material.payload));
    let signing_payload_hash = hex::encode(sha256_hash(&material.signing_bytes));
    if session.payload_hash_hex != payload_hash
        || session.signing_payload_hash_hex != signing_payload_hash
    {
        return Err(format!(
            "本地签名 session payload hash 不匹配：expected={}, runtime_payload={}, signing_payload={}",
            session.payload_hash_hex, payload_hash, signing_payload_hash
        ));
    }

    if !chain_signing::verify_signature(&material, &signature, &public) {
        return Err("sr25519 本地验签失败，拒绝提交到链".to_string());
    }

    let _ = sign_block_number; // immortal era 不再使用区块号，字段只保留给前端会话结构。
    eprintln!("[签名提交] sign_nonce={sign_nonce}, era=immortal, runtime_typed=true");
    let extrinsic = chain_signing::assemble_signed_extrinsic(material, public, signature);
    let full_extrinsic = extrinsic.encode();

    // 先 dry-run 验证，避免提交错误交易导致链卡住
    let extrinsic_hex = format!("0x{}", hex::encode(&full_extrinsic));
    eprintln!(
        "[签名提交] extrinsic hex ({} bytes): {}",
        full_extrinsic.len(),
        &extrinsic_hex[..extrinsic_hex.len().min(200)]
    );
    eprintln!("[签名提交] call_data hex: 0x{}", hex::encode(call_data));

    let dry_run_result = rpc_post(
        "system_dryRun",
        Value::Array(vec![Value::String(extrinsic_hex.clone())]),
    );
    match &dry_run_result {
        Ok(v) => {
            let s = v.as_str().unwrap_or("");
            eprintln!("[签名提交] dry-run 结果: {s}");
            // dry-run 返回 SCALE 编码的 ApplyExtrinsicResult:
            //   0x0000 = Ok(Ok(())) 成功
            //   0x00 01 xx = Ok(Err(DispatchError)) 可调度错误
            //   0x01 00 xx = Err(InvalidTransaction::xxx)
            //   0x01 01 xx = Err(UnknownTransaction::xxx)
            let result_hex = s.strip_prefix("0x").unwrap_or(s);
            // dry-run 已应答但结果无法解码/为空属于异常应答，此时放行
            // 提交等于放弃校验，必须拒绝；与下方"dry-run RPC 不可用"的可用性
            // 兜底是两回事。
            let result_bytes = hex::decode(result_hex)
                .map_err(|e| format!("dry-run 结果异常，拒绝提交: {e} (raw: {s})"))?;
            if result_bytes.is_empty() {
                return Err("dry-run 返回空结果，拒绝提交".to_string());
            }
            if result_bytes[0] != 0x00 {
                // 外层 Result = Err → TransactionValidityError。
                // Future/Stale 等交易提交后只会"看似成功永不上链"
                // （Future 进 future 队列且不向 peer 广播），一律拒绝并把
                // 原因抛给前端，绝不再"继续尝试提交"。
                let reason = classify_invalid_tx(&result_bytes);
                eprintln!("[签名提交] dry-run 拒绝: {reason} (hex: {s})");
                return Err(dry_run_reject_message(&result_bytes, s));
            }
            if result_bytes.len() > 1 && result_bytes[1] != 0x00 {
                // Ok(Err(DispatchError)) — 交易格式正确但执行会失败，阻止提交
                return Err(format!("交易执行会失败: DispatchError (hex: {s})"));
            }
            // 0x0000 = Ok(Ok(())) → 可以提交
        }
        Err(e) => {
            // dry-run RPC 本身不可用（节点未启用 system_dryRun 等）
            // 时保持可用性兜底继续提交，由交易池做最终校验。
            eprintln!("[签名提交] dry-run RPC 失败: {e}");
            eprintln!("[签名提交] 跳过 dry-run 检查，继续提交");
        }
    }

    // dry-run 通过后再正式提交
    let result = rpc_post(
        "author_submitExtrinsic",
        Value::Array(vec![Value::String(extrinsic_hex)]),
    )?;

    // 提交结果必须是交易哈希字符串；其它形态说明节点应答异常，
    // 必须上抛而不是用占位值伪装成功。
    let tx_hash = result
        .as_str()
        .ok_or_else(|| format!("author_submitExtrinsic 返回非字符串: {result}"))?
        .to_string();

    // 被交易池接受 ≠ 已上链（nonce 错位时交易进 future 队列，永不
    // 被打包且不广播）。后台延迟核对一次 nonce 是否被消费，只打日志不阻塞。
    let signer_account_id = signer_account_id_from_public_key(&expected_signer_public_key)?;
    spawn_post_submit_audit(signer_account_id, sign_nonce, tx_hash.clone());

    Ok(VoteSubmitResult { tx_hash })
}

/// 把 dry-run 拒绝结果转成抛给前端的报错文案。
fn dry_run_reject_message(result_bytes: &[u8], raw_hex: &str) -> String {
    chain_signing::dry_run_reject_message(result_bytes, raw_hex)
}

/// 解析 dry-run 返回的 TransactionValidityError。
fn classify_invalid_tx(result_bytes: &[u8]) -> String {
    chain_signing::classify_invalid_tx(result_bytes)
}

/// 提交后的后台核对：短暂等待交易池更新后检查账户 nonce 是否前进。
///
/// `system_accountNextIndex` 包含就绪队列中的交易——nonce 未前进
/// 说明当前观察没有确认交易进入就绪队列或已上链，打告警日志供排查；
/// 该核对纯观测，不影响提交结果，沿用"submit-only + 后台观察"的既定模式。
fn spawn_post_submit_audit(signer_account_id: String, sign_nonce: u32, tx_hash: String) {
    std::thread::spawn(move || {
        std::thread::sleep(POST_SUBMIT_AUDIT_DELAY);
        match fetch_nonce(&signer_account_id) {
            Ok(next) if next > sign_nonce => {
                eprintln!(
                    "[签名提交][后台核对] {tx_hash} nonce 已消费(next={next})，交易已上链或在就绪队列"
                );
            }
            Ok(next) => {
                eprintln!(
                    "[签名提交][后台核对] ⚠ {tx_hash} 尚未在交易池或链上确认 nonce 消费(next={next}, 期望 >{sign_nonce})，请继续查询交易状态"
                );
            }
            Err(e) => {
                eprintln!("[签名提交][后台核对] {tx_hash} nonce 查询失败，无法核对: {e}");
            }
        }
    });
}

// ──── RPC 查询 ────

// chain_query(ADR-017 finalized 收口)复用本封装,放宽到 pub(crate)。
pub(crate) fn rpc_post(method: &str, params: Value) -> Result<Value, String> {
    rpc::rpc_post(
        method,
        params,
        RPC_REQUEST_TIMEOUT,
        RPC_RESPONSE_LIMIT_SMALL,
    )
}

pub(crate) fn fetch_runtime_version() -> Result<(u32, u32), String> {
    let result = rpc_post("state_getRuntimeVersion", Value::Array(vec![]))?;
    let spec = result
        .get("specVersion")
        .and_then(|v| v.as_u64())
        .ok_or("缺少 specVersion")?;
    let tx = result
        .get("transactionVersion")
        .and_then(|v| v.as_u64())
        .ok_or("缺少 transactionVersion")?;
    Ok((spec as u32, tx as u32))
}

/// 构建链交易冷签审阅 payload 与实际签名字节。
pub(crate) fn build_signing_payloads(
    call_data: &[u8],
    genesis_hash: &[u8; 32],
    nonce: u32,
    spec_version: u32,
    tx_version: u32,
) -> Result<(Vec<u8>, Vec<u8>), String> {
    chain_signing::build_signing_payloads(call_data, genesis_hash, nonce, spec_version, tx_version)
}

pub(crate) fn fetch_genesis_hash() -> Result<[u8; 32], String> {
    let result = rpc_post(
        "chain_getBlockHash",
        Value::Array(vec![Value::Number(0.into())]),
    )?;
    let hash_str = result.as_str().ok_or("genesis hash 格式无效")?;
    decode_hash32(hash_str)
}

/// 从当前 sr25519 签名公钥按 runtime 账户识别规则得到唯一账户 ID 文本。
pub(crate) fn signer_account_id_from_public_key(signer_public_key: &str) -> Result<String, String> {
    let signer_public_key = crate::shared::validation::normalize_public_key(signer_public_key)?;
    let public = chain_signing::parse_sr25519_public_key(&signer_public_key)?;
    let account_id = chain_signing::account_id_from_public(public);
    let account_id_bytes: [u8; 32] = account_id.into();
    Ok(format!("0x{}", hex::encode(account_id_bytes)))
}

pub(crate) fn fetch_nonce(account_id: &str) -> Result<u32, String> {
    let account_id = crate::shared::validation::normalize_account_id(account_id)?;
    let ss58 = account_id_to_ss58(
        &hex::decode(account_id.trim_start_matches("0x"))
            .map_err(|e| format!("账户 ID 解码失败: {e}"))?,
    )?;
    let result = rpc_post(
        "system_accountNextIndex",
        Value::Array(vec![Value::String(ss58)]),
    )?;
    result
        .as_u64()
        .map(|v| v as u32)
        .ok_or_else(|| "nonce 格式无效".to_string())
}

// ──── 编码工具 ────

fn decode_hash32(hex_str: &str) -> Result<[u8; 32], String> {
    let clean = hex_str.strip_prefix("0x").unwrap_or(hex_str);
    let bytes = hex::decode(clean).map_err(|e| format!("哈希解码失败: {e}"))?;
    if bytes.len() != 32 {
        return Err(format!("哈希长度无效：期望 32 字节，实际 {}", bytes.len()));
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(&bytes);
    Ok(out)
}

/// Compact<u32> 编码（SCALE）。
pub(crate) fn encode_compact_u32(value: u32) -> Vec<u8> {
    if value < 0x40 {
        vec![(value as u8) << 2]
    } else if value < 0x4000 {
        let v = ((value as u16) << 2) | 0x01;
        vec![v as u8, (v >> 8) as u8]
    } else if value < 0x4000_0000 {
        let v = (value << 2) | 0x02;
        v.to_le_bytes().to_vec()
    } else {
        let mut out = vec![0x03u8]; // big-integer mode
        out.extend_from_slice(&value.to_le_bytes());
        out
    }
}

/// 将 32 字节账户 ID 编码为 SS58 展示地址（prefix=2027）。
pub(crate) fn account_id_to_ss58(account_id: &[u8]) -> Result<String, String> {
    if account_id.len() != 32 {
        return Err("账户 ID 长度必须为 32 字节".to_string());
    }
    // SS58 prefix 2027 的双字节编码：
    // byte0 = ((2027 & 0x00fc) >> 2) | 0x40 = ((2027 & 252) >> 2) | 64
    //        = (8 >> 2) | 64 = 2 | 64 = 66
    // Wait, 2027 in binary: 0b11111101011
    // For two-byte SS58: first_byte = ((prefix & 0xFC) >> 2) | 0x40
    //                     second_byte = (prefix >> 8) | ((prefix & 0x03) << 6)
    let prefix = SS58_PREFIX;
    let first = ((prefix & 0x00fc) >> 2) as u8 | 0x40;
    let second = ((prefix >> 8) as u8) | (((prefix & 0x03) << 6) as u8);

    let mut payload = Vec::with_capacity(2 + 32);
    payload.push(first);
    payload.push(second);
    payload.extend_from_slice(account_id);

    // Blake2b-512 checksum
    let hash = blake2b_simd::Params::new()
        .hash_length(64)
        .to_state()
        .update(b"SS58PRE")
        .update(&payload)
        .finalize();

    payload.push(hash.as_bytes()[0]);
    payload.push(hash.as_bytes()[1]);

    Ok(bs58::encode(&payload).into_string())
}

pub(crate) fn sha256_hash(data: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(data);
    let result = hasher.finalize();
    let mut out = [0u8; 32];
    out.copy_from_slice(&result);
    out
}

/// SHA-256 哈希（供 activation 模块调用）。
pub(crate) fn sha256_hash_public(data: &[u8]) -> [u8; 32] {
    sha256_hash(data)
}

/// 当前 Unix 秒。
///
/// 系统时钟早于 epoch 属于环境故障；静默返回 0 会让 QR 请求一出生
/// 就过期（issued_at=0），冷钱包只报"协议过期"而毫无线索，必须显式失败。
pub(crate) fn now_secs() -> Result<u64, String> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .map_err(|e| format!("系统时钟异常（早于 Unix epoch）: {e}"))
}

pub(crate) fn generate_request_id(prefix: &str) -> String {
    let random_bytes: [u8; 16] = rand::random();
    format!("{}-{}", prefix, hex::encode(random_bytes))
}

/// 生成请求 ID（供 activation 模块调用）。
pub(crate) fn generate_request_id_public(prefix: &str) -> String {
    generate_request_id(prefix)
}

/// 通用签名请求构建：给定 call_data,返回完整的 QR_V1/k=1 签名请求。
///
/// 供 transaction 模块等外部调用方使用，避免重复获取链上参数和构建 review_payload。
pub fn build_sign_request_from_call_data(
    signer_public_key: &str,
    signer_public_key_bytes: &[u8],
    call_data: &[u8],
) -> Result<VoteSignRequestResult, String> {
    let signer_public_key = crate::shared::validation::normalize_public_key(signer_public_key)?;
    let signer_account_id = signer_account_id_from_public_key(&signer_public_key)?;
    let (spec_version, tx_version) = fetch_runtime_version()?;
    let genesis_hash = fetch_genesis_hash()?;
    let nonce = fetch_nonce(&signer_account_id)?;

    let (payload, signing_bytes) =
        build_signing_payloads(call_data, &genesis_hash, nonce, spec_version, tx_version)?;
    let payload_hash = sha256_hash(&payload);
    let payload_hash_hex = hex::encode(payload_hash);
    let signing_payload_hash_hex = hex::encode(sha256_hash(&signing_bytes));
    let request_id = generate_request_id("chain");

    let now = now_secs()?;
    let expires_at = now + DEFAULT_TTL_SECS;
    let request = QrSignRequest {
        proto: QR_V1.to_string(),
        kind: QR_KIND_SIGN_REQUEST,
        id: request_id.clone(),
        expires_at,
        body: SignRequestBody {
            action: chain_action_code(call_data)?,
            sig_alg: 1,
            signer_public_key: public_key_b64(signer_public_key_bytes)?,
            payload: payload_b64(&payload),
        },
    };

    let request_json =
        serde_json::to_string(&request).map_err(|e| format!("序列化签名请求失败: {e}"))?;
    remember_chain_sign_session(
        request_id.clone(),
        ChainSignSession {
            expected_signer_public_key: signer_public_key,
            call_data_hex: hex::encode(call_data),
            payload_hash_hex: payload_hash_hex.clone(),
            signing_payload_hash_hex,
            nonce,
            expires_at,
        },
    )?;

    Ok(VoteSignRequestResult {
        request_json,
        call_data_hex: hex::encode(call_data),
        request_id,
        expected_payload_hash: format!("0x{}", payload_hash_hex),
        sign_nonce: nonce,
        sign_block_number: IMMORTAL_SIGN_BLOCK_NUMBER,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_compact_u32_single_byte() {
        assert_eq!(encode_compact_u32(0), vec![0x00]);
        assert_eq!(encode_compact_u32(1), vec![0x04]);
        assert_eq!(encode_compact_u32(63), vec![0xfc]);
    }

    #[test]
    fn encode_compact_u32_two_bytes() {
        assert_eq!(encode_compact_u32(64), vec![0x01, 0x01]);
    }

    #[test]
    fn sha256_hash_deterministic() {
        let h1 = sha256_hash(b"hello");
        let h2 = sha256_hash(b"hello");
        assert_eq!(h1, h2);
        assert_ne!(h1, sha256_hash(b"world"));
    }

    #[test]
    fn account_id_to_ss58_roundtrip() {
        let account_id = [0xAAu8; 32];
        let ss58 = account_id_to_ss58(&account_id).unwrap();
        assert!(!ss58.is_empty());
        // 验证可以用 bs58 解码回来
        let decoded = bs58::decode(&ss58).into_vec().unwrap();
        assert_eq!(&decoded[2..34], &account_id);
    }

    #[test]
    fn classify_invalid_tx_known_variants() {
        // 0x01 00 xx = Err(InvalidTransaction::xxx)
        assert!(classify_invalid_tx(&[0x01, 0x00, 0x02]).contains("Future"));
        assert!(classify_invalid_tx(&[0x01, 0x00, 0x03]).contains("Stale"));
        assert!(classify_invalid_tx(&[0x01, 0x00, 0x04]).contains("BadProof"));
        assert!(classify_invalid_tx(&[0x01, 0x00, 0x01]).contains("Payment"));
    }

    #[test]
    fn classify_invalid_tx_unknown_transaction() {
        // 0x01 01 xx = Err(UnknownTransaction::xxx)
        assert_eq!(
            classify_invalid_tx(&[0x01, 0x01, 0x00]),
            "UnknownTransaction"
        );
    }

    #[test]
    fn classify_invalid_tx_unrecognized_code_does_not_panic() {
        // 越界/未知变体编号不得 panic，归入 Unknown
        assert!(classify_invalid_tx(&[0x01, 0x00, 0x63]).contains("Unknown"));
        assert_eq!(classify_invalid_tx(&[0x01]), "UnknownTransaction");
    }

    #[test]
    fn dry_run_reject_future_gives_user_hint() {
        // Future = 上一笔还没出块，前端文案必须是人话，不带技术细节
        assert_eq!(
            dry_run_reject_message(&[0x01, 0x00, 0x02], "0x010002"),
            "上一笔交易尚未出块，请稍候再试"
        );
    }

    #[test]
    fn dry_run_reject_other_variants_keep_technical_reason() {
        // Future 之外的变体保持原有技术报错格式（含 hex 便于排查）
        let stale = dry_run_reject_message(&[0x01, 0x00, 0x03], "0x010003");
        assert!(stale.contains("交易校验失败，已拒绝提交"));
        assert!(stale.contains("Stale"));
        assert!(stale.contains("0x010003"));

        let unknown_tx = dry_run_reject_message(&[0x01, 0x01, 0x00], "0x010100");
        assert!(unknown_tx.contains("UnknownTransaction"));
    }

    #[test]
    fn now_secs_returns_positive() {
        // 正常系统时钟下必须返回 epoch 之后的正数秒
        assert!(now_secs().unwrap() > 1_700_000_000);
    }
}
