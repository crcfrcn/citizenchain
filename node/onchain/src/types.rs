//! 定义转账签名准备及提交结果的公开模型。

use serde::{Deserialize, Serialize};

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
    /// 全部成功时的整分报价，不代表已经扣款。
    pub fee_fen: String,
    /// 最终结果分页查询的起点。
    pub scan_from: u32,
}

/// 转账提交结果。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferSubmitResult {
    pub tx_hash: String,
    /// 最终结果分页查询的起点。
    pub scan_from: u32,
}

/// 一套清单合同供首页、冷热签和矿工RPC共用；单笔不改变链上4/0调用。
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransferMode { Transfer, Batch, BatchAll, ForceBatch }

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TransferItem {
    pub to_address: String,
    pub amount_fen: String,
    pub remark: String,
}
