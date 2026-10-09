// 管理员激活模块：为机构 CID 生成管理员账户级本地激活凭证。
//
// 激活流程：
// 1. 用户点击管理员行的"激活"按钮；
// 2. 后端按 CID 读取对应管理员 pallet 的 AdminAccounts，确认签名公钥对应当前管理员账户；
// 3. 后端生成管理员激活签名请求 QR JSON；
// 4. 用户用 citizenwallet 冷钱包扫码确认并签名；
// 5. 后端验证签名、payload、链上账户仍一致后，写入本地激活记录；
// 6. 管理员状态变为已激活，提案按钮解锁。
//
// 激活 payload 格式（非链上交易，二进制前缀域）：
//   GMB(3B) || OP_SIGN_ACTIVATE_ADMIN(1B = 0x18)  ← 4B 二进制前缀
//   + cid_number(32B,右补零) + institution_code(4B) + kind(1B) + signer_public_key(32B)
//   + timestamp(8B) + nonce(16B) = 4 + 93 = 97 bytes
// 冷钱包对整段 payload 直接 sr25519 签名，node 按上述偏移解析。

use crate::home;
use crate::settings::device_password;
use crate::shared::security;
use primitives::cid::code::{
    code_bytes, is_fixed_governance_code, is_private_legal_code, is_public_legal_code,
    is_unincorporated_code, InstitutionCode,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs,
    io::ErrorKind,
    sync::{Mutex, OnceLock},
    time::{SystemTime, UNIX_EPOCH},
};
use tauri::AppHandle;

use crate::admins::management::storage;
use crate::governance::signing;
use primitives::sign::{
    activate_admin_payload, binary_domain_prefix, ACTIVATE_ADMIN_CID_LEN,
    ACTIVATE_ADMIN_PAYLOAD_LEN, BINARY_PREFIX_LEN, OP_SIGN_ACTIVATE_ADMIN,
};

use super::account_id;
use super::types::{institution_code_label, InstitutionAdminsState};

/// 把前端传入的机构码字符串(如 "NRC"/"CGOV")转成链上 [u8;4]。空串/缺省 → None。
fn parse_expected_code(expected: Option<&str>) -> Option<InstitutionCode> {
    expected
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(code_bytes)
}

/// CID 级激活管理员存储文件名；旧账户绑定文件不读取、不迁移。
const ACTIVATED_ADMINS_FILE: &str = "activated-institution-admins.json";

// 管理员本地激活签名 payload 前缀 = GMB || OP_SIGN_ACTIVATE_ADMIN(4B 二进制前缀，
// 单一真源 primitives::sign)。
// cid_number(32) + institution_code(4) + kind(1) + signer_public_key(32)
// + timestamp(8) + nonce(16)。这里只改语义名称，不改变任何 payload 字节。

#[derive(Debug, Clone)]
struct ActivationSignSession {
    signer_public_key: String,
    payload_hash_hex: String,
    payload_hex: String,
    expires_at: u64,
}

static ACTIVATION_SIGN_SESSIONS: OnceLock<Mutex<HashMap<String, ActivationSignSession>>> =
    OnceLock::new();

fn activation_sign_sessions() -> &'static Mutex<HashMap<String, ActivationSignSession>> {
    ACTIVATION_SIGN_SESSIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

// ──── 数据结构 ────

/// 已激活的管理员信息（返回给前端）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivatedAdmin {
    /// 已激活管理员账户 ID，固定为小写 `0x` + 64 位十六进制。
    #[serde(rename = "account_id")]
    pub account_id: String,
    /// 机构唯一 CID。
    pub cid_number: String,
    /// 链上机构码（CID institution_code，[u8;4]）。
    pub institution_code: InstitutionCode,
    /// Node 按实际命中的机构管理员 pallet 派生的机构类型编码。
    pub kind: u8,
    /// 激活时间（毫秒时间戳）。
    pub activated_at_ms: u64,
}

/// 本地存储的激活凭证。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredActivation {
    /// 已激活管理员账户 ID，固定为小写 `0x` + 64 位十六进制。
    account_id: String,
    /// 机构唯一 CID。
    cid_number: String,
    /// 链上机构码（CID institution_code，[u8;4]）。
    institution_code: InstitutionCode,
    /// Node 按实际命中的机构管理员 pallet 派生的机构类型编码。
    kind: u8,
    /// 激活时间（毫秒时间戳）。
    activated_at_ms: u64,
    /// 激活时的签名（用于凭证校验）。
    signature_hex: String,
    /// 激活时签名的 payload hash。
    payload_hash_hex: String,
}

/// 解码后的 CID 级激活 payload。
struct ActivationPayload {
    cid_number: String,
    institution_code: InstitutionCode,
    kind: u8,
    signer_public_key: String,
}

/// 存储文件根结构。
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredActivations {
    #[serde(default)]
    activations: Vec<StoredActivation>,
}

/// 构建激活请求的返回结果。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivateRequestResult {
    /// 完整的 QR 签名请求 JSON 字符串。
    pub request_json: String,
    /// 请求 ID（用于后续验证匹配）。
    pub request_id: String,
    /// 签名 payload 的 SHA-256 哈希（用于验证响应）。
    pub expected_payload_hash: String,
    /// 激活 payload hex（用于本地验证）。
    pub payload_hex: String,
}

// ──── 存储操作 ────

fn activations_path(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    Ok(security::app_data_dir(app)?.join(ACTIVATED_ADMINS_FILE))
}

fn load_activations(app: &AppHandle) -> Result<Vec<StoredActivation>, String> {
    let path = activations_path(app)?;
    let raw = match fs::read_to_string(&path) {
        Ok(v) => v,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(format!("读取激活记录文件失败: {e}")),
    };
    serde_json::from_str::<StoredActivations>(&raw)
        .map(|stored| stored.activations)
        .map_err(|e| format!("解析 CID 管理员激活记录文件失败: {e}"))
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

fn remember_activation_session(
    request_id: String,
    session: ActivationSignSession,
) -> Result<(), String> {
    let mut sessions = activation_sign_sessions()
        .lock()
        .map_err(|_| "管理员激活签名 session 状态异常".to_string())?;
    let now = now_secs();
    sessions.retain(|_, item| item.expires_at >= now);
    sessions.insert(request_id, session);
    Ok(())
}

fn take_activation_session(request_id: &str) -> Result<ActivationSignSession, String> {
    let mut sessions = activation_sign_sessions()
        .lock()
        .map_err(|_| "管理员激活签名 session 状态异常".to_string())?;
    sessions
        .remove(request_id)
        .ok_or_else(|| "未找到管理员激活签名 session，请重新生成二维码".to_string())
}

fn save_activations(app: &AppHandle, activations: &[StoredActivation]) -> Result<(), String> {
    let raw = serde_json::to_string_pretty(&StoredActivations {
        activations: activations.to_vec(),
    })
    .map_err(|e| format!("序列化激活记录失败: {e}"))?;
    let path = activations_path(app)?;
    security::write_text_atomic_restricted(&path, &format!("{raw}\n")).map_err(|e| {
        format!(
            "写入激活记录文件失败 ({}): {e}",
            security::sanitize_path(&path)
        )
    })
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn validate_activation_account(
    state: &InstitutionAdminsState,
    expected_code: Option<InstitutionCode>,
) -> Result<(), String> {
    if let Some(code) = expected_code {
        if state.institution_code != code {
            return Err(format!(
                "管理员账户机构码不匹配：请求 {}，链上 {}",
                institution_code_label(&code),
                institution_code_label(&state.institution_code)
            ));
        }
    }
    match state.kind {
        0 if is_fixed_governance_code(&state.institution_code)
            || is_public_legal_code(&state.institution_code)
            || is_unincorporated_code(&state.institution_code) =>
        {
            Ok(())
        }
        1 if is_private_legal_code(&state.institution_code)
            || is_unincorporated_code(&state.institution_code) =>
        {
            Ok(())
        }
        _ => Err("管理员账户类型与机构码不匹配，不能激活".to_string()),
    }
}

fn fetch_chain_account(
    cid_number: &str,
    expected_code: Option<InstitutionCode>,
) -> Result<InstitutionAdminsState, String> {
    let state = storage::fetch_institution_admins_state_by_cid_number(cid_number)?
        .ok_or_else(|| "链上不存在该 CID 的管理员集合".to_string())?;
    validate_activation_account(&state, expected_code)?;
    Ok(state)
}

/// 构建 CID 级激活 payload；字节布局唯一真源在 `primitives::sign`。
fn build_activate_payload(
    cid_number: &str,
    institution_code: &InstitutionCode,
    kind: u8,
    signer_public_key: &[u8; 32],
    timestamp: u64,
) -> Result<Vec<u8>, String> {
    let nonce: [u8; 16] = rand::random();
    activate_admin_payload(
        cid_number.as_bytes(),
        institution_code,
        kind,
        signer_public_key,
        timestamp,
        &nonce,
    )
    .ok_or_else(|| "cid_number 超出管理员激活签名协议上限".to_string())
}

fn decode_activate_payload(payload_bytes: &[u8]) -> Result<ActivationPayload, String> {
    if payload_bytes.len() != ACTIVATE_ADMIN_PAYLOAD_LEN {
        return Err(format!(
            "激活 payload 长度无效：期望 {} 字节，实际 {} 字节",
            ACTIVATE_ADMIN_PAYLOAD_LEN,
            payload_bytes.len()
        ));
    }
    let expected_prefix = binary_domain_prefix(OP_SIGN_ACTIVATE_ADMIN);
    if payload_bytes[..BINARY_PREFIX_LEN] != expected_prefix {
        return Err("激活 payload 前缀无效".to_string());
    }
    let mut offset = BINARY_PREFIX_LEN;
    let cid_bytes = &payload_bytes[offset..offset + ACTIVATE_ADMIN_CID_LEN];
    offset += ACTIVATE_ADMIN_CID_LEN;
    let cid_end = cid_bytes
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(cid_bytes.len());
    if cid_end == 0 || cid_bytes[cid_end..].iter().any(|byte| *byte != 0) {
        return Err("激活 payload CID 固定槽无效".to_string());
    }
    let cid_number = String::from_utf8(cid_bytes[..cid_end].to_vec())
        .map_err(|_| "激活 payload CID 不是 UTF-8".to_string())?;
    let institution_code: InstitutionCode = payload_bytes[offset..offset + 4]
        .try_into()
        .map_err(|_| "激活 payload 机构码长度无效".to_string())?;
    offset += 4;
    let kind = payload_bytes[offset];
    offset += 1;
    let signer_public_key = format!("0x{}", hex::encode(&payload_bytes[offset..offset + 32]));
    Ok(ActivationPayload {
        cid_number,
        institution_code,
        kind,
        signer_public_key,
    })
}

fn activated_admin_from_stored(item: &StoredActivation) -> ActivatedAdmin {
    ActivatedAdmin {
        account_id: item.account_id.clone(),
        cid_number: item.cid_number.clone(),
        institution_code: item.institution_code,
        kind: item.kind,
        activated_at_ms: item.activated_at_ms,
    }
}

// ──── Tauri 命令 ────

/// 构建管理员激活签名请求 QR JSON（需要节点运行）。
///
/// 验证公钥确实在该 CID 的链上管理员列表中，
/// 然后生成 QR_V1/k=1 格式的 CID 级签名请求。
#[tauri::command(rename_all = "snake_case")]
pub async fn build_activate_admin_request(
    app: AppHandle,
    signer_public_key: String,
    cid_number: String,
    expected_institution_code: Option<String>,
) -> Result<ActivateRequestResult, String> {
    let status = home::current_status(&app)?;
    if !status.running {
        return Err("节点未运行，无法验证管理员身份".to_string());
    }

    let signer_public_key = crate::shared::validation::normalize_public_key(&signer_public_key)?;
    let signer_account_id = signing::signer_account_id_from_public_key(&signer_public_key)?;
    let public_key_bytes = hex::decode(signer_public_key.trim_start_matches("0x"))
        .map_err(|e| format!("签名公钥解码失败: {e}"))?;
    let public_key_array: [u8; 32] = public_key_bytes
        .as_slice()
        .try_into()
        .map_err(|_| "公钥长度必须为 32 字节".to_string())?;

    let expected_code = parse_expected_code(expected_institution_code.as_deref());
    let sid = cid_number.clone();
    let state =
        tauri::async_runtime::spawn_blocking(move || fetch_chain_account(&sid, expected_code))
            .await
            .map_err(|e| format!("查询管理员账户失败: {e}"))??;

    if !state
        .admins
        .iter()
        .any(|admin| admin.account_id == signer_account_id)
    {
        return Err("该签名公钥对应的账户不在此机构的链上管理员列表中".to_string());
    }

    let activations = load_activations(&app)?;
    if activations.iter().any(|a| {
        a.account_id == signer_account_id
            && a.cid_number == state.cid_number
            && a.institution_code == state.institution_code
            && a.kind == state.kind
    }) {
        return Err("该管理员已激活，无需重复操作".to_string());
    }

    let timestamp = now_secs();
    let payload = build_activate_payload(
        &state.cid_number,
        &state.institution_code,
        state.kind,
        &public_key_array,
        timestamp,
    )?;
    let payload_hex = format!("0x{}", hex::encode(&payload));

    let payload_hash = signing::sha256_hash_public(&payload);
    let payload_hash_hex = format!("0x{}", hex::encode(payload_hash));
    let request_id = signing::generate_request_id_public("activate-institution-admin");

    let now = now_secs();
    let expires_at = now + signing::DEFAULT_TTL_SECS;
    let request = signing::QrSignRequest {
        proto: signing::QR_V1.to_string(),
        kind: signing::QR_KIND_SIGN_REQUEST,
        id: request_id.clone(),
        expires_at,
        body: signing::SignRequestBody {
            action: primitives::sign::QR_ACTION_ACTIVATE_ADMIN,
            sig_alg: 1,
            signer_public_key: signing::public_key_b64(&public_key_bytes)?,
            payload: signing::payload_b64(&payload),
        },
    };

    let request_json =
        serde_json::to_string(&request).map_err(|e| format!("序列化签名请求失败: {e}"))?;
    remember_activation_session(
        request_id.clone(),
        ActivationSignSession {
            signer_public_key,
            payload_hash_hex: normalize_hash_hex(&payload_hash_hex, "payload_hash")?,
            payload_hex: payload_hex
                .strip_prefix("0x")
                .unwrap_or(&payload_hex)
                .to_ascii_lowercase(),
            expires_at,
        },
    )?;

    Ok(ActivateRequestResult {
        request_json,
        request_id,
        expected_payload_hash: payload_hash_hex,
        payload_hex,
    })
}

/// 验证管理员激活签名并写入本地加密存储。
///
/// 本地验证 sr25519 签名，不提交链上交易；写入前重新确认链上账户仍 Active。
#[tauri::command(rename_all = "snake_case")]
pub async fn verify_activate_admin(
    app: AppHandle,
    request_id: String,
    signer_public_key: String,
    expected_payload_hash: String,
    payload_hex: String,
    response_json: String,
) -> Result<ActivatedAdmin, String> {
    if let Err(e) = security::append_audit_log(&app, "activate_institution_admin", "attempt") {
        eprintln!("[审计] activate_institution_admin attempt 日志写入失败: {e}");
    }

    let signer_public_key = crate::shared::validation::normalize_public_key(&signer_public_key)?;
    let signer_account_id = signing::signer_account_id_from_public_key(&signer_public_key)?;

    let response: signing::QrSignResponse =
        serde_json::from_str(&response_json).map_err(|e| format!("解析签名响应失败: {e}"))?;

    if response.proto != signing::QR_V1 {
        return Err(format!(
            "协议版本不匹配：期望 {}，实际 {}",
            signing::QR_V1,
            response.proto
        ));
    }
    if response.kind != signing::QR_KIND_SIGN_RESPONSE {
        return Err(format!(
            "二维码类型不匹配：期望 k={}，实际 k={}",
            signing::QR_KIND_SIGN_RESPONSE,
            response.kind
        ));
    }
    if response.expires_at < now_secs() {
        return Err("签名响应已过期，请重新生成激活二维码".to_string());
    }
    if response.id != request_id {
        return Err("请求 ID 不匹配".to_string());
    }
    let session = take_activation_session(&request_id)?;
    if session.expires_at < now_secs() {
        return Err("管理员激活签名 session 已过期，请重新生成二维码".to_string());
    }
    if response.expires_at != session.expires_at {
        return Err("签名响应过期时间与本地激活 session 不匹配".to_string());
    }
    if session.signer_public_key != signer_public_key {
        return Err("提交参数公钥与本地激活 session 不匹配".to_string());
    }

    // QR_V1 压缩键仍为 `u`；内部语义字段统一为 `signer_public_key`。
    let response_public_key = response
        .body
        .signer_public_key
        .strip_prefix("0x")
        .unwrap_or(&response.body.signer_public_key)
        .to_ascii_lowercase();
    if format!("0x{response_public_key}") != signer_public_key {
        return Err("公钥不匹配".to_string());
    }

    let expected_hash = expected_payload_hash
        .strip_prefix("0x")
        .unwrap_or(&expected_payload_hash)
        .to_ascii_lowercase();
    let expected_hash = normalize_hash_hex(&expected_hash, "expected_payload_hash")?;
    if expected_hash != session.payload_hash_hex {
        return Err("提交参数 payload hash 与本地激活 session 不匹配".to_string());
    }

    let payload_clean = payload_hex.strip_prefix("0x").unwrap_or(&payload_hex);
    if payload_clean.to_ascii_lowercase() != session.payload_hex {
        return Err("提交参数 payload 与本地激活 session 不匹配".to_string());
    }
    let payload_bytes = hex::decode(payload_clean).map_err(|e| format!("payload 解码失败: {e}"))?;
    let actual_hash = hex::encode(signing::sha256_hash_public(&payload_bytes));
    if actual_hash != session.payload_hash_hex {
        return Err(format!(
            "激活 payload hash 与本地 session 不匹配：expected={}, actual={actual_hash}",
            session.payload_hash_hex
        ));
    }
    let decoded = decode_activate_payload(&payload_bytes)?;
    if decoded.signer_public_key != signer_public_key {
        return Err("激活 payload 中的管理员公钥与签名公钥不一致".to_string());
    }

    let decoded_cid_number = decoded.cid_number.clone();
    let state = tauri::async_runtime::spawn_blocking({
        let cid_number = decoded.cid_number.clone();
        move || {
            storage::fetch_institution_admins(&cid_number)?
                .ok_or_else(|| "链上不存在该 CID 的管理员集合".to_string())
        }
    })
    .await
    .map_err(|e| format!("查询管理员账户失败: {e}"))??;
    if state.institution_code != decoded.institution_code {
        return Err(format!(
            "激活 payload 机构码与链上账户不一致：payload={}，链上={}",
            institution_code_label(&decoded.institution_code),
            institution_code_label(&state.institution_code)
        ));
    }
    if state.kind != decoded.kind {
        return Err(format!(
            "激活 payload kind 与链上账户不一致：payload={}，链上={}",
            decoded.kind, state.kind
        ));
    }
    validate_activation_account(&state, Some(decoded.institution_code))?;
    if !state
        .admins
        .iter()
        .any(|admin| admin.account_id == signer_account_id)
    {
        return Err("该签名公钥对应的账户不在此机构的链上管理员列表中".to_string());
    }

    let sig_hex = response
        .body
        .signature
        .strip_prefix("0x")
        .unwrap_or(&response.body.signature);
    if sig_hex.len() != 128 {
        return Err(format!(
            "签名长度无效：期望 128 hex，实际 {}",
            sig_hex.len()
        ));
    }
    let signature_bytes = hex::decode(sig_hex).map_err(|e| format!("签名解码失败: {e}"))?;
    let public_key_bytes = hex::decode(signer_public_key.trim_start_matches("0x"))
        .map_err(|e| format!("签名公钥解码失败: {e}"))?;

    use sp_core::crypto::Pair;
    use sp_core::sr25519::{Public, Signature};
    let public = Public::from_raw(
        <[u8; 32]>::try_from(public_key_bytes.as_slice()).map_err(|_| "公钥长度必须为 32 字节")?,
    );
    let signature = Signature::from_raw(
        <[u8; 64]>::try_from(signature_bytes.as_slice()).map_err(|_| "签名长度必须为 64 字节")?,
    );
    if !sp_core::sr25519::Pair::verify(&signature, &payload_bytes, &public) {
        return Err("sr25519 签名验证失败，无法证明管理员身份".to_string());
    }

    let mut activations = load_activations(&app)?;
    activations
        .retain(|a| !(a.account_id == signer_account_id && a.cid_number == decoded_cid_number));
    let activated_at = now_ms();
    activations.push(StoredActivation {
        account_id: signer_account_id.clone(),
        cid_number: decoded_cid_number.clone(),
        institution_code: decoded.institution_code,
        kind: decoded.kind,
        activated_at_ms: activated_at,
        signature_hex: sig_hex.to_string(),
        payload_hash_hex: expected_hash,
    });
    save_activations(&app, &activations)?;

    if let Err(e) = security::append_audit_log(
        &app,
        "activate_institution_admin",
        &format!(
            "success account_id={} cid_number={}",
            &signer_account_id[..10],
            decoded_cid_number
        ),
    ) {
        eprintln!("[审计] activate_institution_admin success 日志写入失败: {e}");
    }

    Ok(ActivatedAdmin {
        account_id: signer_account_id,
        cid_number: decoded_cid_number,
        institution_code: decoded.institution_code,
        kind: decoded.kind,
        activated_at_ms: activated_at,
    })
}

/// 获取指定管理员账户的已激活管理员列表。
///
/// 每次调用时与链上当前管理员列表交叉校验：
/// - 链上已移除的管理员 → 自动删除本地激活记录；
/// - institution_code/kind 已变化或账户已关闭 → 不再返回本地激活记录；
/// - 返回仍有效的已激活管理员。
#[tauri::command(rename_all = "snake_case")]
pub async fn get_activated_admins(
    app: AppHandle,
    cid_number: String,
    expected_institution_code: Option<String>,
) -> Result<Vec<ActivatedAdmin>, String> {
    let expected_code = parse_expected_code(expected_institution_code.as_deref());
    let mut activations = load_activations(&app)?;

    let account_activations: Vec<&StoredActivation> = activations
        .iter()
        .filter(|a| a.cid_number == cid_number)
        .collect();

    if account_activations.is_empty() {
        return Ok(Vec::new());
    }

    let status = home::current_status(&app)?;
    if status.running {
        let sid = cid_number.clone();
        let state =
            tauri::async_runtime::spawn_blocking(move || fetch_chain_account(&sid, expected_code))
                .await
                .map_err(|e| format!("查询管理员账户失败: {e}"));

        match state {
            Ok(Ok(state)) => {
                let before_len = activations.len();
                activations.retain(|a| {
                    if a.cid_number != cid_number {
                        return true;
                    }
                    a.institution_code == state.institution_code
                        && a.kind == state.kind
                        && state
                            .admins
                            .iter()
                            .any(|admin| admin.account_id == a.account_id)
                });
                if activations.len() != before_len {
                    let _ = save_activations(&app, &activations);
                }
            }
            Ok(Err(e)) if e.contains("链上不存在") || e.contains("类型与机构码不匹配") =>
            {
                activations.retain(|a| a.cid_number != cid_number);
                let _ = save_activations(&app, &activations);
                return Ok(Vec::new());
            }
            _ => {}
        }
    }

    Ok(activations
        .iter()
        .filter(|a| a.cid_number == cid_number)
        .map(activated_admin_from_stored)
        .collect())
}

/// 取消管理员激活（需要设备密码）。
#[tauri::command(rename_all = "snake_case")]
pub fn deactivate_admin(
    app: AppHandle,
    account_id: String,
    cid_number: String,
    expected_institution_code: Option<String>,
    unlock_password: String,
) -> Result<(), String> {
    if let Err(e) = security::append_audit_log(&app, "deactivate_admin", "attempt") {
        eprintln!("[审计] deactivate_admin attempt 日志写入失败: {e}");
    }

    let unlock = security::ensure_unlock_password(&unlock_password)?;
    device_password::verify_device_login_password(&app, unlock)?;

    let account_id = account_id::normalize_account_id(&account_id)?;
    let expected_code = parse_expected_code(expected_institution_code.as_deref());
    if let Some(code) = expected_code {
        let state = storage::fetch_institution_admins_state_by_cid_number(&cid_number)?
            .ok_or_else(|| "链上不存在该 CID 的管理员集合".to_string())?;
        validate_activation_account(&state, Some(code))?;
    }

    let mut activations = load_activations(&app)?;
    let before_len = activations.len();
    activations.retain(|a| !(a.account_id == account_id && a.cid_number == cid_number));

    if activations.len() == before_len {
        return Err("未找到该管理员的激活记录".to_string());
    }

    save_activations(&app, &activations)?;

    if let Err(e) = security::append_audit_log(
        &app,
        "deactivate_admin",
        &format!(
            "success account_id={} cid_number={}",
            &account_id[..account_id.len().min(10)],
            cid_number
        ),
    ) {
        eprintln!("[审计] deactivate_admin success 日志写入失败: {e}");
    }

    Ok(())
}

/// 检查本地是否有任何已激活的管理员（纯本地文件读取，不需要节点运行）。
///
/// 用于设置页面判断是否显示引导节点和投票节点设置项。
#[tauri::command]
pub fn has_any_activated_admin(app: AppHandle) -> Result<bool, String> {
    let activations = load_activations(&app)?;
    Ok(!activations.is_empty())
}
