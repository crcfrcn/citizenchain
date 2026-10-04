//! 提案业务详情适配层。
//!
//! governance 提案聚合只依赖本文件的通用函数；具体业务模块的详情结构、
//! 解码和独立 storage 查询由各业务模块自己维护。

use crate::transaction::multisig::proposal as transfer_module;

pub use transfer_module::ProposalDetails;

/// 已识别出的业务模块提案动作。
pub enum ProposalAction {
    /// 多签转账模块动作。
    TransferModule(Box<transfer_module::ProposalAction>),
}

impl ProposalAction {
    /// 转为接口详情字段集合。
    pub fn into_details(self) -> ProposalDetails {
        match self {
            ProposalAction::TransferModule(action) => (*action).into_details(),
        }
    }
}

/// 尝试从内部投票 ProposalData 识别业务动作。
pub fn decode_internal_proposal_data_action(
    proposal_id: u64,
    data: &[u8],
) -> Option<ProposalAction> {
    transfer_module::decode_proposal_data_action(proposal_id, data)
        .map(|action| ProposalAction::TransferModule(Box::new(action)))
}

/// 查询业务模块独立 storage 中的提案动作。
pub fn fetch_stored_action(proposal_id: u64) -> Result<Option<ProposalAction>, String> {
    Ok(transfer_module::fetch_stored_action(proposal_id)?
        .map(|action| ProposalAction::TransferModule(Box::new(action))))
}

/// 生成业务模块列表摘要。
pub fn format_summary<F>(action: &ProposalAction, resolve_cid_full_name: F) -> String
where
    F: Fn(&str) -> Option<String>,
{
    match action {
        ProposalAction::TransferModule(action) => {
            transfer_module::format_summary(action, resolve_cid_full_name)
        }
    }
}
