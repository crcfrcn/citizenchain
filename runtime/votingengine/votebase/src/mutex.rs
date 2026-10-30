//! 内部提案互斥锁。
//!
//! 同一提案主体下的内部提案互斥规则:
//! - **普通提案**(`Regular`)允许同主体多个并发,但**禁止**与管理员变更提案共存。
//! - **管理员变更提案**(`AdminSetMutationExclusive`)同主体下必须独占,
//!   且发起时该主体不得有任何普通活跃提案。
//!
//! 本文件提供三个对外入口:
//! - `acquire_internal_proposal_mutex` — 创建提案时获取互斥锁,登记反向绑定。
//! - `release_internal_proposal_mutexes` — 终态/阶段切换时释放该提案持有的全部锁。
//! - `ensure_admin_set_mutation_lock_owner` — 校验某主体当前是否被指定 proposal_id
//!   的管理员变更提案占用。

use frame_support::ensure;
use frame_support::pallet_prelude::DispatchResult;

use crate::pallet::{self, Error, InternalProposalMutexes, ProposalMutexBindings};
use crate::{
    InternalProposalMutexBinding, InternalProposalMutexKind, InternalProposalMutexState,
    ProposalSubject,
};

impl<T: pallet::Config> pallet::Pallet<T> {
    pub fn acquire_internal_proposal_mutex(
        proposal_id: u64,
        subject: ProposalSubject<T::AccountId>,
        kind: InternalProposalMutexKind,
    ) -> DispatchResult {
        InternalProposalMutexes::<T>::try_mutate_exists(
            subject.clone(),
            |maybe| -> DispatchResult {
                let state = maybe.get_or_insert_with(InternalProposalMutexState::default);
                match kind {
                    InternalProposalMutexKind::Regular => {
                        ensure!(
                            state.admin_set_mutation_proposal.is_none(),
                            Error::<T>::AdminSetMutationProposalActive
                        );
                        state.regular_active_count = state
                            .regular_active_count
                            .checked_add(1)
                            .ok_or(Error::<T>::InternalProposalMutexOverflow)?;
                    }
                    InternalProposalMutexKind::AdminSetMutationExclusive => {
                        ensure!(
                            state.admin_set_mutation_proposal.is_none(),
                            Error::<T>::AdminSetMutationProposalActive
                        );
                        ensure!(
                            state.regular_active_count == 0,
                            Error::<T>::RegularInternalProposalActive
                        );
                        state.admin_set_mutation_proposal = Some(proposal_id);
                    }
                }
                Ok(())
            },
        )?;

        ProposalMutexBindings::<T>::try_mutate(proposal_id, |bindings| {
            bindings
                .try_push(InternalProposalMutexBinding { subject, kind })
                .map_err(|_| Error::<T>::TooManyInternalProposalMutexBindings)?;
            Ok(())
        })
    }

    pub fn release_internal_proposal_mutexes(proposal_id: u64) {
        let bindings = ProposalMutexBindings::<T>::take(proposal_id);
        for binding in bindings {
            InternalProposalMutexes::<T>::mutate_exists(binding.subject, |maybe| {
                let Some(state) = maybe.as_mut() else {
                    return;
                };
                match binding.kind {
                    InternalProposalMutexKind::Regular => {
                        state.regular_active_count = state.regular_active_count.saturating_sub(1);
                    }
                    InternalProposalMutexKind::AdminSetMutationExclusive => {
                        if state.admin_set_mutation_proposal == Some(proposal_id) {
                            state.admin_set_mutation_proposal = None;
                        }
                    }
                }
                if state.is_empty() {
                    *maybe = None;
                }
            });
        }
    }

    pub fn ensure_admin_set_mutation_lock_owner(
        subject: ProposalSubject<T::AccountId>,
        proposal_id: u64,
    ) -> DispatchResult {
        let state = InternalProposalMutexes::<T>::get(subject)
            .ok_or(Error::<T>::InternalProposalMutexOwnerMismatch)?;
        ensure!(
            state.admin_set_mutation_proposal == Some(proposal_id),
            Error::<T>::InternalProposalMutexOwnerMismatch
        );
        Ok(())
    }
}
