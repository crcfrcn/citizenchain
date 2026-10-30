use serde::Serialize;

/// 普通多签转账提案详情（从 `VotingEngine::ProposalData` 解码）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferProposalDetail {
    /// 提案主键 ID。
    pub proposal_id: u64,
    /// 发起机构唯一 CID；个人多签转账为空。
    pub actor_cid_number: Option<String>,
    /// 实际转出资金的机构账户或个人多签账户。
    pub funding_account_id: String,
    /// 收款账户 ID。
    pub beneficiary_account_id: String,
    /// 金额（分）。
    pub amount_fen: String,
    /// 转账备注。
    pub remark: String,
    /// 提案人账户 ID。
    pub proposer_account_id: String,
}

/// 安全基金转账提案详情。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SafetyFundProposalDetail {
    /// 提案主键 ID。
    pub proposal_id: u64,
    /// 国家储委会唯一 CID。
    pub actor_cid_number: String,
    /// 实际转出的安全基金账户。
    pub institution_account_id: String,
    /// 收款账户 ID。
    pub beneficiary_account_id: String,
    /// 金额（分）。
    pub amount_fen: String,
    /// 转账备注。
    pub remark: String,
}

/// 手续费划转提案详情。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SweepProposalDetail {
    /// 提案主键 ID。
    pub proposal_id: u64,
    /// 发起机构唯一 CID。
    pub actor_cid_number: String,
    /// 实际转出的机构费用账户。
    pub institution_account_id: String,
    /// 金额（分）。
    pub amount_fen: String,
}

/// node 提案详情接口中由 multisig-transfer 模块导出的字段集合。
///
/// 使用 `flatten` 挂到治理聚合返回值上，保持前端当前 JSON 字段，
/// 同时避免 governance/proposal 继续定义多签转账详情结构。
#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProposalDetails {
    /// 普通多签转账详情。
    pub transfer_detail: Option<TransferProposalDetail>,
    /// 安全基金转账详情。
    pub safety_fund_detail: Option<SafetyFundProposalDetail>,
    /// 手续费划转详情。
    pub sweep_detail: Option<SweepProposalDetail>,
}

/// multisig-transfer 业务动作解码结果。
#[derive(Debug, Clone)]
pub enum ProposalAction {
    /// 普通多签转账。
    Transfer(Box<TransferProposalDetail>),
    /// 安全基金转账。
    SafetyFund(Box<SafetyFundProposalDetail>),
    /// 手续费划转。
    Sweep(Box<SweepProposalDetail>),
}

impl ProposalAction {
    /// 转为提案详情接口字段集合。
    pub fn into_details(self) -> ProposalDetails {
        match self {
            crate::types::ProposalAction::Transfer(detail) => ProposalDetails {
                transfer_detail: Some(*detail),
                ..crate::types::ProposalDetails::default()
            },
            crate::types::ProposalAction::SafetyFund(detail) => ProposalDetails {
                safety_fund_detail: Some(*detail),
                ..crate::types::ProposalDetails::default()
            },
            crate::types::ProposalAction::Sweep(detail) => ProposalDetails {
                sweep_detail: Some(*detail),
                ..crate::types::ProposalDetails::default()
            },
        }
    }
}
