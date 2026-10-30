use serde::Serialize;

/// 转账签名请求结果（前端用于显示 QR 码）。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferSignRequestResult {
    pub request_json: String,
    pub request_id: String,
    pub expected_payload_hash: String,
    pub sign_nonce: u32,
    pub sign_block_number: u64,
    /// call_data 的 hex 编码（提交时需要回传）。
    pub call_data_hex: String,
    /// 预估手续费（元）。
    pub fee_yuan: f64,
}

/// 转账提交结果。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferSubmitResult {
    pub tx_hash: String,
}
