use crate::types::DecryptAdminRequestResult;
use crate::types::DecryptedAdminInfo;
use nodeclient::signing::payload_b64;
use nodeclient::signing::public_key_b64;
use nodeclient::signing::sha256_hash_public;
use nodeclient::signing::QrSignRequest;
use nodeclient::signing::QrSignResponse;
use nodeclient::signing::SignRequestBody;
use nodeclient::signing::QR_KIND_SIGN_REQUEST;
use nodeclient::signing::QR_KIND_SIGN_RESPONSE;
use nodeclient::signing::QR_V1;
use primitives::sign::binary_domain_prefix;
use primitives::sign::decrypt_admin_payload;
use primitives::sign::BINARY_PREFIX_LEN;
use primitives::sign::DECRYPT_ADMIN_CID_LEN;
use primitives::sign::DECRYPT_ADMIN_PAYLOAD_LEN;
use primitives::sign::OP_SIGN_DECRYPT;
use rand::Rng;
use serde::Deserialize;
use serde::Serialize;
use sha2::Digest;
use sha2::Sha256;
use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::OnceLock;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

const DEFAULT_TTL_SECS: u64 = 90;

/// 当前正在内存中"已解密"的管理员表(节点重启清空)。
///
/// key = 小写 `0x` + 64 位十六进制签名公钥，value = (cid_number, decrypted_at_ms)。
static DECRYPTED_ADMINS: OnceLock<Mutex<HashMap<String, MemoryEntry>>> = OnceLock::new();

/// 等待"解密"响应的进行中 challenge 上下文。前端拿到 request_id,扫描签名响应时
/// 由 verify 阶段从此表查回 payload 做本地验签。
static PENDING_CHALLENGES: OnceLock<Mutex<HashMap<String, ChallengeContext>>> = OnceLock::new();

#[derive(Clone)]
struct MemoryEntry {
    cid_number: String,
    decrypted_at_ms: u64,
}

#[derive(Clone)]
struct ChallengeContext {
    signer_public_key: String,
    cid_number: String,
    payload: Vec<u8>,
    issued_at_ms: u64,
}

fn decrypted_map() -> &'static Mutex<HashMap<String, MemoryEntry>> {
    DECRYPTED_ADMINS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn pending_map() -> &'static Mutex<HashMap<String, ChallengeContext>> {
    PENDING_CHALLENGES.get_or_init(|| Mutex::new(HashMap::new()))
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// 复用 primitives 唯一原语拼装固定 92B challenge payload。
fn build_challenge_payload(
    public_key_bytes: &[u8; 32],
    cid_number: &str,
    timestamp: u64,
) -> Option<Vec<u8>> {
    let nonce: [u8; 16] = rand::thread_rng().gen();
    decrypt_admin_payload(cid_number.as_bytes(), public_key_bytes, timestamp, &nonce)
}

/// 验签前锁死当前共享协议长度与二进制域，旧 108B 布局直接拒绝。
fn validate_challenge_payload(payload: &[u8]) -> Result<(), String> {
    if payload.len() != DECRYPT_ADMIN_PAYLOAD_LEN {
        return Err(format!(
            "解密 challenge 长度无效:期望 {DECRYPT_ADMIN_PAYLOAD_LEN},实际 {}",
            payload.len()
        ));
    }
    if payload[..BINARY_PREFIX_LEN] != binary_domain_prefix(OP_SIGN_DECRYPT) {
        return Err("解密 challenge 二进制域无效".to_string());
    }
    Ok(())
}

fn generate_request_id() -> String {
    let bytes: [u8; 16] = rand::thread_rng().gen();
    format!("decrypt-{}", hex::encode(bytes))
}

/// 构造解密请求 QR JSON,把 ChallengeContext 暂存以备验签。
pub(crate) fn build_decrypt_admin_request(
    signer_public_key: &str,
    cid_number: &str,
) -> Result<DecryptAdminRequestResult, String> {
    let signer_public_key = nodedesktop::validation::normalize_public_key(signer_public_key)?;
    if cid_number.is_empty() || cid_number.len() > DECRYPT_ADMIN_CID_LEN {
        return Err("cid_number 超出链上 CID_NUMBER_MAX_BYTES 范围".to_string());
    }
    let public_key_bytes = hex::decode(signer_public_key.trim_start_matches("0x"))
        .map_err(|e| format!("公钥解码失败:{e}"))?;
    let signer_public_key_array: [u8; 32] = public_key_bytes
        .as_slice()
        .try_into()
        .map_err(|_| "公钥长度必须为 32 字节".to_string())?;

    let timestamp = now_secs();
    let payload = build_challenge_payload(&signer_public_key_array, cid_number, timestamp)
        .ok_or_else(|| "cid_number 超出解密签名协议范围".to_string())?;
    let payload_hex = format!("0x{}", hex::encode(&payload));
    let payload_hash = sha256_hash_public(&payload);
    let payload_hash_hex = format!("0x{}", hex::encode(payload_hash));
    let request_id = generate_request_id();

    let now = now_secs();
    let request = QrSignRequest {
        proto: QR_V1.to_string(),
        kind: QR_KIND_SIGN_REQUEST,
        id: request_id.clone(),
        expires_at: now + DEFAULT_TTL_SECS,
        body: SignRequestBody {
            action: primitives::sign::QR_ACTION_DECRYPT_ADMIN,
            sig_alg: 1,
            signer_public_key: public_key_b64(&public_key_bytes)?,
            payload: payload_b64(&payload),
        },
    };

    let request_json =
        serde_json::to_string(&request).map_err(|e| format!("序列化签名请求失败:{e}"))?;

    pending_map()
        .lock()
        .map_err(|_| "decrypt 待处理表锁异常".to_string())?
        .insert(
            request_id.clone(),
            ChallengeContext {
                signer_public_key,
                cid_number: cid_number.to_string(),
                payload,
                issued_at_ms: now_ms(),
            },
        );

    Ok(DecryptAdminRequestResult {
        request_json,
        request_id,
        expected_payload_hash: payload_hash_hex,
        payload_hex,
    })
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct VerifyDecryptAdminInput {
    pub request_id: String,

    pub signer_public_key: String,
    pub expected_payload_hash: String,
    pub response_json: String,
}

/// 验证 CitizenWallet 签名响应，通过则把 `(signer_public_key, cid_number)` 写入内存解密表。
pub(crate) fn verify_and_decrypt_admin(
    input: VerifyDecryptAdminInput,
) -> Result<DecryptedAdminInfo, String> {
    let response: QrSignResponse =
        serde_json::from_str(&input.response_json).map_err(|e| format!("解析签名响应失败:{e}"))?;

    if response.proto != QR_V1 {
        return Err(format!(
            "协议版本不匹配:期望 {QR_V1},实际 {}",
            response.proto
        ));
    }
    if response.kind != QR_KIND_SIGN_RESPONSE {
        return Err(format!(
            "二维码类型不匹配:期望 k={QR_KIND_SIGN_RESPONSE},实际 k={}",
            response.kind
        ));
    }
    if response.id != input.request_id {
        return Err("请求 ID 不匹配".to_string());
    }

    let signer_public_key =
        nodedesktop::validation::normalize_public_key(&input.signer_public_key)?;
    if response.body.signer_public_key != signer_public_key {
        return Err("公钥不匹配".to_string());
    }

    let expected_hash = input
        .expected_payload_hash
        .strip_prefix("0x")
        .unwrap_or(&input.expected_payload_hash)
        .to_ascii_lowercase();
    if expected_hash.is_empty() {
        return Err("本地签名 session 缺少 payload hash".to_string());
    }

    // 拉回原 challenge payload 做本地 sr25519 验签。
    let context = {
        let mut guard = pending_map()
            .lock()
            .map_err(|_| "decrypt 待处理表锁异常".to_string())?;
        guard
            .remove(&input.request_id)
            .ok_or_else(|| "未找到对应的 challenge 上下文(已过期或被消费)".to_string())?
    };
    if context.signer_public_key != signer_public_key {
        return Err("challenge 上下文公钥与请求不一致".to_string());
    }
    validate_challenge_payload(&context.payload)?;

    // expected_payload_hash 必须等于 SHA-256(payload)
    let local_hash = {
        let mut h = Sha256::new();
        h.update(&context.payload);
        let r = h.finalize();
        let mut out = [0u8; 32];
        out.copy_from_slice(&r);
        format!("0x{}", hex::encode(out))
    };
    if local_hash != format!("0x{expected_hash}") {
        return Err("本地重新计算的 payload hash 与请求不一致".to_string());
    }

    // sr25519 验签
    let sig_hex = response
        .body
        .signature
        .strip_prefix("0x")
        .unwrap_or(&response.body.signature);
    if sig_hex.len() != 128 {
        return Err(format!("签名长度无效:期望 128 hex,实际 {}", sig_hex.len()));
    }
    let signature_bytes = hex::decode(sig_hex).map_err(|e| format!("签名解码失败:{e}"))?;
    let public_key_bytes = hex::decode(signer_public_key.trim_start_matches("0x"))
        .map_err(|e| format!("公钥解码失败:{e}"))?;
    use sp_core::crypto::Pair;
    use sp_core::sr25519::{Public, Signature};
    let public = Public::from_raw(
        <[u8; 32]>::try_from(public_key_bytes.as_slice()).map_err(|_| "公钥长度必须为 32 字节")?,
    );
    let signature = Signature::from_raw(
        <[u8; 64]>::try_from(signature_bytes.as_slice()).map_err(|_| "签名长度必须为 64 字节")?,
    );
    if !sp_core::sr25519::Pair::verify(&signature, &context.payload, &public) {
        return Err("sr25519 签名验证失败,无法证明对该公钥的控制".to_string());
    }

    let now = now_ms();
    decrypted_map()
        .lock()
        .map_err(|_| "decrypt 内存表锁异常".to_string())?
        .insert(
            signer_public_key.clone(),
            MemoryEntry {
                cid_number: context.cid_number.clone(),
                decrypted_at_ms: now,
            },
        );

    log::info!(
        "[ClearingBank] 管理员 {} 已解密(cid={},耗时 {} ms)",
        &signer_public_key[..10],
        context.cid_number,
        now.saturating_sub(context.issued_at_ms),
    );

    Ok(DecryptedAdminInfo {
        signer_public_key,
        cid_number: context.cid_number,
        decrypted_at_ms: now,
    })
}

/// 列出某机构当前在内存中已解密的管理员。
pub(crate) fn list_decrypted_admins(cid_number: &str) -> Vec<DecryptedAdminInfo> {
    let guard = match decrypted_map().lock() {
        Ok(g) => g,
        Err(e) => e.into_inner(),
    };
    guard
        .iter()
        .filter(|(_, v)| v.cid_number == cid_number)
        .map(|(k, v)| DecryptedAdminInfo {
            signer_public_key: k.clone(),
            cid_number: v.cid_number.clone(),
            decrypted_at_ms: v.decrypted_at_ms,
        })
        .collect()
}

/// 将某管理员从内存解密表移除。前端"重新加锁"用。
pub(crate) fn lock_decrypted_admin(signer_public_key: &str) -> Result<(), String> {
    let signer_public_key = nodedesktop::validation::normalize_public_key(signer_public_key)?;
    let mut guard = decrypted_map()
        .lock()
        .map_err(|_| "decrypt 内存表锁异常".to_string())?;
    if guard.remove(&signer_public_key).is_none() {
        return Err("该公钥未在解密状态".to_string());
    }
    Ok(())
}
#[cfg(test)]
#[path = "../tests/unlock.rs"]
mod tests;
