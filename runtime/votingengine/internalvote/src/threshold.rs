//! 内部投票时长、阈值校验以及阈值生命周期副作用。

use super::*;

impl<T: Config> Pallet<T> {
    pub(crate) fn internal_stage_duration() -> frame_system::pallet_prelude::BlockNumberFor<T> {
        (VOTING_DURATION_BLOCKS as u64).saturated_into()
    }

    pub(crate) fn ensure_all_admin_threshold(admins_len: u32, threshold: u32) -> DispatchResult {
        // 账户链上注册与注销会改变账户生命周期，必须由该账户快照内全体管理员通过。
        ensure!(
            admins_len > 0 && threshold == admins_len,
            Error::<T>::InvalidThresholdSnapshot
        );
        Ok(())
    }

    pub(crate) fn ensure_dynamic_threshold(voter_count: u32, threshold: u32) -> DispatchResult {
        // 机构岗位席位与个人管理员都采用严格过半；统一用 u64 避免乘法溢出。
        ensure!(voter_count >= 2, Error::<T>::InvalidDynamicThreshold);
        ensure!(
            threshold > 0
                && threshold <= voter_count
                && u64::from(threshold).saturating_mul(2) > u64::from(voter_count),
            Error::<T>::InvalidDynamicThreshold
        );
        Ok(())
    }

    pub(crate) fn snapshot_admins_len_or_missing(
        proposal_id: u64,
        subject: ProposalSubject<T::AccountId>,
    ) -> Result<u32, DispatchError> {
        <votingengine::Pallet<T>>::snapshot_admins_len(proposal_id, subject)
            .ok_or(votingengine::Error::<T>::MissingAdminSnapshot.into())
    }
}

impl<T: Config> Pallet<T> {
    pub(crate) fn proposal_personal_account(
        proposal_id: u64,
    ) -> Result<T::AccountId, DispatchError> {
        let proposal =
            Proposals::<T>::get(proposal_id).ok_or(votingengine::Error::<T>::ProposalNotFound)?;
        ensure!(
            proposal.internal_code == Some(votingengine::types::PMUL)
                && proposal.actor_cid_number.is_none(),
            votingengine::Error::<T>::InvalidInstitution
        );
        proposal
            .execution_account_id
            .ok_or(votingengine::Error::<T>::InvalidInstitution.into())
    }

    pub(crate) fn apply_executed_threshold_side_effect(proposal_id: u64) -> DispatchResult {
        match InternalProposalRoles::<T>::get(proposal_id) {
            Some(InternalProposalRole::PersonalCreate) => {
                let personal_account_id = Self::proposal_personal_account(proposal_id)?;
                let threshold = PendingPersonalThresholds::<T>::take(proposal_id)
                    .ok_or(Error::<T>::MissingDynamicThreshold)?;
                ActivePersonalThresholds::<T>::insert(personal_account_id, threshold);
            }
            Some(InternalProposalRole::PersonalClose) => {
                let personal_account_id = Self::proposal_personal_account(proposal_id)?;
                ActivePersonalThresholds::<T>::remove(personal_account_id);
            }
            Some(InternalProposalRole::PersonalAdminChange) => {
                if let Some(pending) = PendingPersonalAdminChangeThresholds::<T>::take(proposal_id)
                {
                    Self::ensure_dynamic_threshold(pending.new_admins_len, pending.new_threshold)?;
                    ActivePersonalThresholds::<T>::insert(
                        pending.personal_account_id,
                        pending.new_threshold,
                    );
                }
            }
            _ => {}
        }
        Ok(())
    }

    pub(crate) fn apply_terminal_threshold_cleanup(proposal_id: u64, status: u8) -> DispatchResult {
        match (InternalProposalRoles::<T>::get(proposal_id), status) {
            (
                Some(InternalProposalRole::PersonalCreate),
                STATUS_REJECTED | STATUS_EXECUTION_FAILED,
            ) => {
                PendingPersonalThresholds::<T>::remove(proposal_id);
            }
            (
                Some(InternalProposalRole::PersonalAdminChange),
                STATUS_REJECTED | STATUS_EXECUTION_FAILED,
            ) => {
                PendingPersonalAdminChangeThresholds::<T>::remove(proposal_id);
            }
            (Some(_), STATUS_EXECUTED) | (None, _) => {}
            _ => {}
        }
        Ok(())
    }
}
