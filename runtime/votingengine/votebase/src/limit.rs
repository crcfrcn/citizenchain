//! 全局活跃提案数量限制。
//!
//! 每个提案主体同时最多允许 Runtime 配置数量的活跃提案，
//! 不区分提案类型（转账、销毁、换管理员等），由投票引擎统一管控。
//!
//! 使用方式：
//! - 创建提案时调用 `try_add_active_proposal`
//! - 提案完成/清理时调用 `remove_active_proposal`
//! - App 端查询活跃数调用 `active_proposal_count`

use crate::pallet::{self, Config, Error};
use crate::ProposalSubject;
use frame_support::pallet_prelude::*;

/// 尝试为主体新增一个活跃提案。
/// 成功返回 Ok(())，达到上限返回 Err。
pub fn try_add_active_proposal<T: Config>(
    subject: ProposalSubject<T::AccountId>,
    proposal_id: u64,
) -> DispatchResult {
    pallet::ActiveProposalsBySubject::<T>::try_mutate(subject, |ids| {
        ensure!(
            (ids.len() as u32) < T::MaxActiveProposals::get(),
            Error::<T>::ActiveProposalLimitReached
        );
        ids.try_push(proposal_id)
            .map_err(|_| Error::<T>::ActiveProposalLimitReached)?;
        Ok(())
    })
}

/// 尝试为多个主体新增同一个活跃提案。
///
/// 多机构提案会关联多个 CID,在创建事务中逐个写入;
/// 任一主体达到上限时事务回滚,不会留下部分写入。
pub fn try_add_active_proposals<T: Config>(
    subjects: sp_std::vec::Vec<ProposalSubject<T::AccountId>>,
    proposal_id: u64,
) -> DispatchResult {
    for subject in subjects {
        try_add_active_proposal::<T>(subject, proposal_id)?;
    }
    Ok(())
}

/// 从主体的活跃提案列表中移除指定提案。
pub fn remove_active_proposal<T: Config>(subject: ProposalSubject<T::AccountId>, proposal_id: u64) {
    pallet::ActiveProposalsBySubject::<T>::mutate(subject, |ids| {
        ids.retain(|&id| id != proposal_id);
    });
}

/// 查询主体当前活跃提案数量。
pub fn active_proposal_count<T: Config>(subject: ProposalSubject<T::AccountId>) -> u32 {
    pallet::ActiveProposalsBySubject::<T>::get(subject).len() as u32
}
