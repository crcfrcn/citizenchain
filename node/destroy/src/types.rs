use serde::Serialize;

/// 决议销毁提案详情。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolutionDestroyDetail {
    #[serde(serialize_with = "nodeclient::codec::serialize_decimal")]
    pub proposal_id: u128,
    pub actor_cid_number: String,

    pub institution_account_id: String,
    pub amount_fen: String,
}
