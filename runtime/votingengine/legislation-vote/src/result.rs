//! 立法机关表决终局结果边界。
//!
//! 终局结果由 votingengine 核心交给回调元组；法律、任免和预算业务分别依据
//! `ProposalOwner`/`MODULE_TAG` 认领。任一提案只能由一个业务模块返回非 `Ignored`，
//! 本 pallet 不读取任何业务存储。

/// 立法成功结果统一映射为 votingengine 的通过状态。
pub(crate) const fn approved_status() -> u8 {
    votingengine::STATUS_PASSED
}
use crate::*;

impl<T: Config> Pallet<T> {
    /// 通用阶段切换:写新 stage + 重置计时窗口 + 重排到期桶(签署/会签阶段共用)。
    pub(crate) fn transition_stage(proposal_id: u64, new_stage: u8) -> DispatchResult {
        let now = <frame_system::Pallet<T>>::block_number();
        let end = now.saturating_add(Self::stage_duration());
        with_transaction(|| {
            let old_end = match Proposals::<T>::try_mutate(
                proposal_id,
                |maybe| -> Result<frame_system::pallet_prelude::BlockNumberFor<T>, DispatchError> {
                    let p = maybe
                        .as_mut()
                        .ok_or(votingengine::Error::<T>::ProposalNotFound)?;
                    let old = p.end;
                    p.stage = new_stage;
                    p.start = now;
                    p.end = end;
                    Ok(old)
                },
            ) {
                Ok(v) => v,
                Err(err) => return TransactionOutcome::Rollback(Err(err)),
            };
            let old_expiry = old_end.saturating_add(One::one());
            ProposalsByExpiry::<T>::mutate(old_expiry, |ids| ids.retain(|&i| i != proposal_id));
            if let Err(err) = <votingengine::Pallet<T>>::schedule_proposal_expiry(proposal_id, end)
            {
                return TransactionOutcome::Rollback(Err(err));
            }
            TransactionOutcome::Commit(Ok(()))
        })
    }

    /// 非特别案内部全过 → 进入行政签署阶段(市长/省长/总统)。
    pub(crate) fn advance_to_sign(proposal_id: u64) -> DispatchResult {
        Self::transition_stage(proposal_id, STAGE_LEG_SIGN)?;
        <votingengine::Pallet<T>>::release_internal_proposal_mutexes(proposal_id);
        Self::deposit_event(pallet::Event::<T>::LegislationAdvancedToSign { proposal_id });
        Ok(())
    }

    /// 行政首长否决/超时(省行政区/国家) → 退回立法院三人会签阶段。
    pub(crate) fn advance_to_override(proposal_id: u64) -> DispatchResult {
        pallet::LegOverrideSigns::<T>::remove(proposal_id);
        Self::transition_stage(proposal_id, STAGE_LEG_OVERRIDE)?;
        Self::deposit_event(pallet::Event::<T>::LegislationAdvancedToOverride { proposal_id });
        Ok(())
    }
}
