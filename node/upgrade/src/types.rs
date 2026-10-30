use serde::Serialize;

/// 协议升级提案详情（从 VotingEngine::ProposalData 解码）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeUpgradeDetail {
    pub proposal_id: u64,

    pub proposer_account_id: String,
    pub reason: String,
    pub code_hash_hex: String,
    pub expected_pow_hash: String,
    pub pow_params: difficulty::PowDifficultyParams,
}

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
