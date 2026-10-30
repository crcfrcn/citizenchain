use nodemultisig::types::ProposalDetails;

/// 已识别出的业务模块提案动作。
pub(crate) enum ProposalAction {
    /// 多签转账模块动作。
    TransferModule(Box<nodemultisig::types::ProposalAction>),
}

impl ProposalAction {
    /// 转为接口详情字段集合。
    pub fn into_details(self) -> ProposalDetails {
        match self {
            crate::business::ProposalAction::TransferModule(action) => (*action).into_details(),
        }
    }
}

/// 尝试从内部投票 ProposalData 识别业务动作。
pub(crate) fn decode_internal_proposal_data_action(
    proposal_id: u64,
    data: &[u8],
) -> Option<ProposalAction> {
    nodemultisig::proposal::decode_proposal_data_action(proposal_id, data)
        .map(|action| crate::business::ProposalAction::TransferModule(Box::new(action)))
}

/// 查询业务模块独立 storage 中的提案动作。
pub(crate) fn fetch_stored_action(proposal_id: u64) -> Result<Option<ProposalAction>, String> {
    Ok(nodemultisig::proposal::fetch_stored_action(proposal_id)?
        .map(|action| crate::business::ProposalAction::TransferModule(Box::new(action))))
}

/// 生成业务模块列表摘要。
pub(crate) fn format_summary<F>(action: &ProposalAction, resolve_cid_full_name: F) -> String
where
    F: Fn(&str) -> Option<String>,
{
    match action {
        crate::business::ProposalAction::TransferModule(action) => {
            nodemultisig::proposal::format_summary(action, resolve_cid_full_name)
        }
    }
}
