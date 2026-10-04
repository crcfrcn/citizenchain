//! QR_V1/k=1 签名请求构造工具。
//!
//! 这里只负责把已确定的签名原文包装成统一二维码 envelope;
//! 业务模块仍负责决定签名内容和权限语义。

// QR 构造失败必须保留统一 HTTP 拒绝响应，禁止业务调用方重新映射为不同错误口径。
#![allow(clippy::result_large_err)]

use crate::{
    api_error,
    core::qr::{bytes_to_b64, public_key_hex_to_b64, QR_V1},
};
use axum::http::StatusCode;

pub(crate) fn build_sign_request(
    request_id: &str,
    issued_at: i64,
    expires_at: i64,
    account_id: &str,
    payload_text: &str,
    action: u16,
) -> Result<String, axum::response::Response> {
    build_sign_request_bytes(
        request_id,
        issued_at,
        expires_at,
        account_id,
        payload_text.as_bytes(),
        action,
    )
}

/// 把已确定的待签 payload **裸字节**包装成 QR_V1/k=1 envelope。
///
/// 普通链交易传入值必须是完整 `review_payload`，钱包依赖它完整解码和中文展示；
/// 32 字节 `signing_bytes` 只允许 Runtime 升级 hash-only 专用入口使用。
pub(crate) fn build_sign_request_bytes(
    request_id: &str,
    _issued_at: i64,
    expires_at: i64,
    account_id: &str,
    payload_bytes: &[u8],
    action: u16,
) -> Result<String, axum::response::Response> {
    let Some(public_key_b64) = public_key_hex_to_b64(account_id) else {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            1001,
            "account_id must be lowercase 0x plus 64 hexadecimal characters",
        ));
    };
    let sign_request = serde_json::json!({
        "p": QR_V1,
        "k": 1,
        "i": request_id,
        "e": expires_at,
        "b": {
            "a": action,
            "g": 1,
            "u": public_key_b64,
            "d": bytes_to_b64(payload_bytes),
        }
    });
    serde_json::to_string(&sign_request).map_err(|_| {
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            1503,
            "encode sign request failed",
        )
    })
}

/// 构造「域签名」QR(注册局首次绑定/换绑):签名账户在段1**未知**，
/// 生成端把完整授权 SCALE 中的 account_id 槽写成 32 字节零，钱包扫码后严格解析并替换。
///
/// 与 [`build_sign_request_bytes`] 的两处差异:
/// - `u`(签名账户)**留空** —— 钱包据 `action ∈ {citizen_occupy, citizen_rebind}` 自填本机账户,
///   故不走 `public_key_hex_to_b64`(它对空/非法账户会 1001 拒);
/// - `d` = 带零账户槽的完整授权模板，已包含创世哈希、CID、revision 和过期时间。
///
/// occupy/rebind 的字段顺序不同，钱包不得再把账户简单追加到 `d` 末尾。
pub(crate) fn build_domain_sign_request_bytes(
    request_id: &str,
    expires_at: i64,
    authorization_template: &[u8],
    action: u16,
) -> Result<String, axum::response::Response> {
    let sign_request = serde_json::json!({
        "p": QR_V1,
        "k": 1,
        "i": request_id,
        "e": expires_at,
        "b": {
            "a": action,
            "g": 1,
            "u": "",
            "d": bytes_to_b64(authorization_template),
        }
    });
    serde_json::to_string(&sign_request).map_err(|_| {
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            1503,
            "encode domain sign request failed",
        )
    })
}
