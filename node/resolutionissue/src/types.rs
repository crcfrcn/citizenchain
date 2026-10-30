use serde::Serialize;

/// 决议发行提案详情。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolutionIssuanceDetail {
    pub proposal_id: u64,
    pub actor_cid_number: String,
    pub proposer_role_code: String,

    pub proposer_account_id: String,
    pub reason: String,
    pub total_amount_fen: String,
    pub allocations: Vec<IssuanceAllocationItem>,
}

/// 决议发行分配项。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IssuanceAllocationItem {
    pub recipient_account_id: String,
    pub amount_fen: String,
}
