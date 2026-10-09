// 清算行管理员密钥解密(unlock)流程。
//
// 与 admins/management/activation.rs 中的"激活"语义一致——citizenwallet 冷钱包扫码签 challenge,
// 节点本地验签——但本流程**仅在清算行 tab 用**,术语为"解密"以区别于 NRC/PRC/PRB
// 的"激活"。区别:
// - 激活(activation):写入 activated-admins.json 长期持久化
// - 解密(decrypt):仅写入内存 HashMap,节点重启自动清空,无 TTL
//
// 实际私钥的 AES-GCM 加密文件由 CLI 启动路径(`--clearing-bank-password`)生成,
// `settlement::keystore::OffchainKeystore` 加载到 `KeystoreBatchSigner` 的
// `Arc<RwLock<Option<SigningKey>>>` 槽位。本模块的"解密"含义是:
//   1. citizenwallet 签 challenge → 节点 sr25519 验签 → 证明操作员持有该公钥的冷钱包
//   2. 把 (signer_public_key, cid_number) 标记为内存内“授权可用”，packer 攒批前 cross-check
//      该入口存在才会启动签名(防误用启动密码加载的 SigningKey)

use rand::Rng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::governance::signing::{
    payload_b64, public_key_b64, sha256_hash_public, QrSignRequest, QrSignResponse,
    SignRequestBody, QR_KIND_SIGN_REQUEST, QR_KIND_SIGN_RESPONSE, QR_V1,
};
use primitives::sign::{
    binary_domain_prefix, decrypt_admin_payload, BINARY_PREFIX_LEN, DECRYPT_ADMIN_CID_LEN,
    DECRYPT_ADMIN_PAYLOAD_LEN, OP_SIGN_DECRYPT,
};

use crate::transaction::offchain::types::{DecryptAdminRequestResult, DecryptedAdminInfo};

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
    signer_public_key_bytes: &[u8; 32],
    cid_number: &str,
    timestamp: u64,
) -> Option<Vec<u8>> {
    let nonce: [u8; 16] = rand::thread_rng().gen();
    decrypt_admin_payload(
        cid_number.as_bytes(),
        signer_public_key_bytes,
        timestamp,
        &nonce,
    )
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
pub fn build_decrypt_admin_request(
    signer_public_key: &str,
    cid_number: &str,
) -> Result<DecryptAdminRequestResult, String> {
    let signer_public_key = crate::shared::validation::normalize_public_key(signer_public_key)?;
    if cid_number.is_empty() || cid_number.len() > DECRYPT_ADMIN_CID_LEN {
        return Err("cid_number 超出链上 CID_NUMBER_MAX_BYTES 范围".to_string());
    }
    let signer_public_key_bytes = hex::decode(signer_public_key.trim_start_matches("0x"))
        .map_err(|e| format!("公钥解码失败:{e}"))?;
    let signer_public_key_array: [u8; 32] = signer_public_key_bytes
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
            signer_public_key: public_key_b64(&signer_public_key_bytes)?,
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
pub struct VerifyDecryptAdminInput {
    pub request_id: String,
    #[serde(rename = "signer_public_key")]
    pub signer_public_key: String,
    pub expected_payload_hash: String,
    pub response_json: String,
}

/// 验证 CitizenWallet 签名响应，通过则把 `(signer_public_key, cid_number)` 写入内存解密表。
pub fn verify_and_decrypt_admin(
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
        crate::shared::validation::normalize_public_key(&input.signer_public_key)?;
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
    let signer_public_key_bytes = hex::decode(signer_public_key.trim_start_matches("0x"))
        .map_err(|e| format!("公钥解码失败:{e}"))?;
    use sp_core::crypto::Pair;
    use sp_core::sr25519::{Public, Signature};
    let public = Public::from_raw(
        <[u8; 32]>::try_from(signer_public_key_bytes.as_slice())
            .map_err(|_| "公钥长度必须为 32 字节")?,
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
pub fn list_decrypted_admins(cid_number: &str) -> Vec<DecryptedAdminInfo> {
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
pub fn lock_decrypted_admin(signer_public_key: &str) -> Result<(), String> {
    let signer_public_key = crate::shared::validation::normalize_public_key(signer_public_key)?;
    let mut guard = decrypted_map()
        .lock()
        .map_err(|_| "decrypt 内存表锁异常".to_string())?;
    if guard.remove(&signer_public_key).is_none() {
        return Err("该公钥未在解密状态".to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn challenge_payload_layout_uses_shared_protocol_length() {
        let p = build_challenge_payload(&[0xAA; 32], "AH001-SCB0V-123456789-2026", 1234567890)
            .expect("valid payload");
        assert_eq!(p.len(), DECRYPT_ADMIN_PAYLOAD_LEN);
        assert_eq!(
            p[..BINARY_PREFIX_LEN],
            binary_domain_prefix(OP_SIGN_DECRYPT)
        );
    }

    #[test]
    fn challenge_payload_signer_public_key_position() {
        let p = build_challenge_payload(&[0xCC; 32], "AH001-FCB0P-123456789-2026", 0)
            .expect("valid payload");
        assert_eq!(
            &p[BINARY_PREFIX_LEN + DECRYPT_ADMIN_CID_LEN
                ..BINARY_PREFIX_LEN + DECRYPT_ADMIN_CID_LEN + 32],
            &[0xCC; 32]
        );
    }

    #[test]
    fn legacy_108_byte_payload_is_rejected() {
        assert!(validate_challenge_payload(&[0u8; 108]).is_err());
    }

    #[test]
    fn build_decrypt_admin_request_rejects_short_signer_public_key() {
        let err = build_decrypt_admin_request("0xAA", "AH001-SCB0V-123456789-2026").unwrap_err();
        assert!(err.contains("公钥格式"));
    }

    #[test]
    fn list_decrypted_admins_filters_by_cid() {
        decrypted_map().lock().unwrap().insert(
            format!("0x{}", "aa".repeat(32)),
            MemoryEntry {
                cid_number: "AH001-SCB0V-123456789-2026".to_string(),
                decrypted_at_ms: 1,
            },
        );
        decrypted_map().lock().unwrap().insert(
            format!("0x{}", "bb".repeat(32)),
            MemoryEntry {
                cid_number: "AH001-SCB0H-202605070-2026".to_string(),
                decrypted_at_ms: 2,
            },
        );
        let r = list_decrypted_admins("AH001-SCB0V-123456789-2026");
        assert_eq!(r.len(), 1);
        assert!(r[0].signer_public_key.contains("aa"));

        // 清理:只移除本用例插入的两个 key,绝不 clear() 整表——否则并行执行时
        // 会误删其他用例(如 lock_decrypted_admin_removes_entry 的 cc… )刚插入的条目。
        let mut guard = decrypted_map().lock().unwrap();
        guard.remove(&format!("0x{}", "aa".repeat(32)));
        guard.remove(&format!("0x{}", "bb".repeat(32)));
    }

    #[test]
    fn lock_decrypted_admin_removes_entry() {
        decrypted_map().lock().unwrap().insert(
            format!("0x{}", "cc".repeat(32)),
            MemoryEntry {
                cid_number: "AH001-SCB0E-202605180-2026".to_string(),
                decrypted_at_ms: 10,
            },
        );
        assert!(lock_decrypted_admin(&format!("0x{}", "cc".repeat(32))).is_ok());
        assert!(lock_decrypted_admin(&format!("0x{}", "cc".repeat(32))).is_err());
    }
}
