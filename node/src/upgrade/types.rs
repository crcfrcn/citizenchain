use serde::Serialize;

/// 构建 propose_runtime_upgrade 签名请求的返回值。
/// 本模块只返回协议升级业务签名请求，不返回或保存任何投票材料。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProposeUpgradeRequestResult {
    pub request_json: String,
    pub request_id: String,
    pub expected_payload_hash: String,
    pub sign_nonce: u32,
    pub sign_block_number: u64,
}
