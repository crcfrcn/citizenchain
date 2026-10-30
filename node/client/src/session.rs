use crate::signing::normalize_hash_hex;
use crate::signing::now_secs;

use sha2::Digest;

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::OnceLock;

#[derive(Debug, Clone)]
pub struct ChainSignSession {
    pub expected_public_key: String,
    pub call_data_hex: String,
    /// QR `b.d` 携带的完整审阅载荷 SHA-256。
    pub payload_hash_hex: String,
    /// sr25519 实际签名输入 SHA-256，用于提交前重建校验。
    pub payload_hash: String,
    pub nonce: u32,
    pub expires_at: u64,
}

static CHAIN_SIGN_SESSIONS: OnceLock<Mutex<HashMap<String, ChainSignSession>>> = OnceLock::new();

pub(crate) fn chain_sign_sessions() -> &'static Mutex<HashMap<String, ChainSignSession>> {
    CHAIN_SIGN_SESSIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn remember_chain_sign_session(
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

pub fn remember_chain_sign_request_session(
    request_id: &str,
    expected_public_key: &str,
    call_data: &[u8],
    payload_hash_hex: &str,
    payload_hash: &str,
    nonce: u32,
    expires_at: u64,
) -> Result<(), String> {
    remember_chain_sign_session(
        request_id.to_string(),
        ChainSignSession {
            expected_public_key: nodedesktop::validation::normalize_public_key(
                expected_public_key,
            )?,
            call_data_hex: hex::encode(call_data),
            payload_hash_hex: normalize_hash_hex(payload_hash_hex, "payload_hash")?,
            payload_hash: normalize_hash_hex(payload_hash, "signing_payload_hash")?,
            nonce,
            expires_at,
        },
    )
}
