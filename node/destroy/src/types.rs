use serde::Serialize;

/// 决议销毁提案详情。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolutionDestroyDetail {
    pub proposal_id: u64,
    pub actor_cid_number: String,

    pub institution_account_id: String,
    pub amount_fen: String,
}
