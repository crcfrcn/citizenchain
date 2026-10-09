//! 管理员敏感动作的冷钱包扫码签名工具(PasskeyColdSign 档)。
//!
//! 敏感动作 step-up 统一为
//! "会话(链上已证管理员)+ 冷钱包扫码签名"。本模块只承载扫码签名 payload 构造、
//! 哈希与验签工具,不含任何设备本地因子。

// Axum 安全辅助函数直接返回统一 HTTP Response，错误只走拒绝路径，装箱会扩散整条鉴权接口。
#![allow(clippy::result_large_err)]

use axum::http::StatusCode;
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::crypto::pubkey::same_account_id;
use crate::*;

/// 敏感动作挑战有效期(秒)。
pub(crate) const ADMIN_ACTION_TTL_SECONDS: i64 = 300;

/// 冷钱包扫码签名的结构化 payload(序列化为 JSON 文本后冷签)。
#[derive(Serialize)]
pub(crate) struct AdminSignedPayload<'a> {
    pub(crate) domain: &'static str,
    pub(crate) qr_proto: &'static str,
    pub(crate) action_id: &'a str,
    pub(crate) action_type: &'a str,
    pub(crate) account_id: &'a str,
    pub(crate) actor_cid_number: &'a str,
    pub(crate) actor_province_name: &'a str,
    pub(crate) target: &'a str,
    pub(crate) request_hash: &'a str,
    pub(crate) before_hash: &'a str,
    pub(crate) after_hash: &'a str,
    pub(crate) expires_at: i64,
}

pub(crate) fn signed_payload_text(payload: AdminSignedPayload<'_>) -> String {
    serde_json::to_string(&payload).unwrap_or_default()
}

pub(crate) fn payload_hash_for_text(text: &str) -> String {
    format!("0x{}", hex::encode(Sha256::digest(text.as_bytes())))
}

pub(crate) fn hash_json(value: &serde_json::Value) -> String {
    let encoded = serde_json::to_vec(value).unwrap_or_default();
    format!("0x{}", hex::encode(Sha256::digest(&encoded)))
}

/// 校验签名钱包对动作 payload 的扫码签名。
///
/// ① signer 必须等于动作发起人;② 提交摘要与服务端预期摘要一致;
/// ③ sr25519 验签通过。调用方(actions::commit)还会额外校验 signer ∈ 本机构链上 Active 集合。
pub(crate) fn verify_account_signature(
    expected_actor_account_id: &str,
    account_id: &str,
    signature: &str,
    submitted_payload_hash: &str,
    expected_payload_hash: &str,
    payload_text: &str,
) -> Result<(), axum::response::Response> {
    if !same_account_id(expected_actor_account_id, account_id) {
        return Err(api_error(
            StatusCode::FORBIDDEN,
            1003,
            "account_id does not match actor_account_id",
        ));
    }
    if submitted_payload_hash.trim().to_lowercase() != expected_payload_hash {
        return Err(api_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            2004,
            "payload hash mismatch",
        ));
    }
    // 治理动作走统一哈希域:signing_message(OP_SIGN_ONCHINA_ADMIN, payload)
    // = blake2_256(GMB || 0x20 || payload)。此前对裸 JSON 文本直签,无 GMB 前缀、
    // 无 op_tag、不进 SIGN_OP_TAGS,域分离只靠 JSON 里一个 domain 字符串字段。
    let signing_bytes = primitives::sign::signing_message(
        primitives::sign::OP_SIGN_ONCHINA_ADMIN,
        payload_text.as_bytes(),
    );
    if !crate::auth::login::verify_admin_signature_bytes(account_id, &signing_bytes, signature) {
        return Err(api_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            2004,
            "signature verify failed",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_admin_payload_includes_actor_cid_number_in_fixed_order() {
        let text = signed_payload_text(AdminSignedPayload {
            domain: "onchina_admin_governance",
            qr_proto: "QR_V1",
            action_id: "action-1",
            action_type: "INSTITUTION_CREATE_ACCOUNT",
            account_id: "0x1111111111111111111111111111111111111111111111111111111111111111",
            actor_cid_number: "LN001-FRG0G-000000001-2026",
            actor_province_name: "北京市",
            target: "target",
            request_hash: "0xaa",
            before_hash: "0xbb",
            after_hash: "0xcc",
            expires_at: 123,
        });
        assert_eq!(
            text,
            "{\"domain\":\"onchina_admin_governance\",\"qr_proto\":\"QR_V1\",\"action_id\":\"action-1\",\"action_type\":\"INSTITUTION_CREATE_ACCOUNT\",\"account_id\":\"0x1111111111111111111111111111111111111111111111111111111111111111\",\"actor_cid_number\":\"LN001-FRG0G-000000001-2026\",\"actor_province_name\":\"北京市\",\"target\":\"target\",\"request_hash\":\"0xaa\",\"before_hash\":\"0xbb\",\"after_hash\":\"0xcc\",\"expires_at\":123}"
        );
    }
}
