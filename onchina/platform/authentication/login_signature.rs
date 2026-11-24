//! 解析账户公钥、验证岗位人员登录签名并清理过期挑战。

use hex::FromHex;
use schnorrkel::{signing_context, PublicKey as Sr25519PublicKey, Signature as Sr25519Signature};

/// 校验签名钱包对**规范文本**的 sr25519 / ML-DSA 签名。
///
/// 只接受一种编码。此前还额外接受 `<Bytes>{message}</Bytes>` 包裹形式,
/// 即同一条逻辑消息有两种合法字节 —— 而全仓**没有任何一端产生**这种包裹
/// (那是 polkadot.js 签任意文本的约定,本项目冷端签的是裸字节)。
/// 纯粹是白白扩大的攻击面,已删除。
pub(crate) fn verify_admin_signature(
    account_id: &str,
    message: &str,
    signature_text: &str,
) -> bool {
    verify_admin_signature_bytes(account_id, message.as_bytes(), signature_text)
}

/// 校验岗位人员使用的签名钱包对原始字节的 sr25519 / ML-DSA 签名。
///
/// 登录、机构接收证明和既有治理动作共用底层验签，消息域由调用方固定。
/// - 扫码登录:`verify_admin_signature` 传管道分隔的登录原文
///   (`QR_V1|k|id|system|expires|principal`,与冷端 `signature_message.dart` 同源);
/// - 治理动作:`action_sign::verify_account_signature` 传
///   `signing_message(OP_SIGN_ONCHINA_CONFIRM, payload)` 的 32 字节摘要。
pub(crate) fn verify_admin_signature_bytes(
    account_id: &str,
    message: &[u8],
    signature_text: &str,
) -> bool {
    // 当前绑定钱包首次登录也可携带ML-DSA公钥信封；签名原文不变。
    if let Ok(wire) = hex::decode(signature_text.trim_start_matches("0x")) {
        if let Ok((public, signature)) = chain_signing::mldsa::decode_pq_signature(&wire) {
            if let Some(account) = parse_account_id_bytes(account_id) {
                return chain_signing::mldsa::verify_account_signature(
                    &account, &public, message, &signature,
                );
            }
            return false;
        }
    }
    let Some(public_key_bytes) = parse_sr25519_public_key_bytes(account_id) else {
        return false;
    };
    let normalized_sig = strip_0x_prefix(signature_text);
    let sig_bytes = match Vec::from_hex(normalized_sig) {
        Ok(v) if v.len() == 64 => v,
        _ => return false,
    };
    let sig_arr: [u8; 64] = match sig_bytes.as_slice().try_into() {
        Ok(v) => v,
        Err(_) => return false,
    };
    let public_key = match Sr25519PublicKey::from_bytes(&public_key_bytes) {
        Ok(v) => v,
        Err(_) => return false,
    };
    let signature = match Sr25519Signature::from_bytes(&sig_arr) {
        Ok(v) => v,
        Err(_) => return false,
    };
    let ctx = signing_context(b"substrate");
    public_key.verify(ctx.bytes(message), &signature).is_ok()
}

/// 解析 sr25519 签名公钥，返回小写 `0x` 加 64 位十六进制。
pub(crate) fn parse_sr25519_public_key(public_key: &str) -> Option<String> {
    let public_key = public_key.trim();
    if public_key.len() != 66 || !public_key.starts_with("0x") {
        return None;
    }
    let raw = &public_key[2..];
    if raw
        .bytes()
        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Some(public_key.to_string());
    }
    None
}

pub(crate) fn parse_sr25519_public_key_bytes(public_key: &str) -> Option<[u8; 32]> {
    let public_key = parse_sr25519_public_key(public_key)?;
    let bytes = Vec::from_hex(strip_0x_prefix(&public_key)).ok()?;
    bytes.as_slice().try_into().ok()
}

/// 把规范账户 ID 解码为当前 runtime 使用的 32 字节账户值。
pub fn parse_account_id_bytes(account_id: &str) -> Option<[u8; 32]> {
    let account_id = crate::crypto::pubkey::normalize_account_id(account_id)?;
    let bytes = Vec::from_hex(&account_id[2..]).ok()?;
    bytes.as_slice().try_into().ok()
}

/// 去掉 0x/0X 前缀，仅用于 hex::decode 前的临时处理，不用于存储。
fn strip_0x_prefix(value: &str) -> &str {
    value
        .trim()
        .strip_prefix("0x")
        .or_else(|| value.trim().strip_prefix("0X"))
        .unwrap_or(value.trim())
}

pub(super) fn extract_domain_from_origin(origin: &str) -> Option<String> {
    let url = reqwest::Url::parse(origin.trim()).ok()?;
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.path() != "/"
    {
        return None;
    }
    Some(url.host_str()?.to_string())
}
