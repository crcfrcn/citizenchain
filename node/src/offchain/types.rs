// 清算行链下网络 Tauri command 用 DTO 集合,与前端 transaction/offchain/types.ts 对齐。
//
//
// - 本文件只保留清算行节点声明、连通性检测、管理员解锁等 offchain 网络类型。
// - 机构身份只读 DTO 在同模块 `institution_read::types`。

use serde::Serialize;

/// 链上 `ClearingBankNodes[cid_number]` 解码后的对前端形态。
///
/// 字段为字符串/u32 友好类型,前端无需做 Bytes/SS58 自行处理。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClearingBankNodeOnChainInfo {
    pub cid_number: String,
    /// libp2p PeerId 字符串("12D3KooW..." 形式)。
    pub peer_id: String,
    pub rpc_domain: String,
    pub rpc_port: u16,
    /// 链上注册区块高度。
    pub registered_at: u64,
    /// 注册管理员账户 ID（小写 `0x` + 64 位十六进制）。
    #[serde(rename = "registered_by_account_id")]
    pub registered_by_account_id: String,
    /// 注册管理员 SS58(便于前端展示)。
    #[serde(rename = "registered_by_ss58_address")]
    pub registered_by_ss58_address: String,
}

/// 连通性自测的逐项结果。每项要么 ok=true,要么带 detail 解释失败原因。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectivityCheck {
    pub label: &'static str,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// 4 重自测汇总报告。`all_ok` 任一项失败即 false,前端据此置灰提交按钮。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectivityTestReport {
    pub all_ok: bool,
    pub checks: Vec<ConnectivityCheck>,
}

/// 当前内存中已解密(可用于自动签 batch)的清算行管理员条目。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DecryptedAdminInfo {
    /// 管理员签名公钥（小写 `0x` + 64 位十六进制）。
    #[serde(rename = "signer_public_key")]
    pub signer_public_key: String,
    pub cid_number: String,
    /// 解密时间(毫秒时间戳)。
    pub decrypted_at_ms: u64,
}

/// 解密管理员密钥的 QR_V1 请求构建结果。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DecryptAdminRequestResult {
    pub request_json: String,
    pub request_id: String,
    pub expected_payload_hash: String,
    /// challenge payload hex(用于本地验证 sr25519 签名)。
    pub payload_hex: String,
}
