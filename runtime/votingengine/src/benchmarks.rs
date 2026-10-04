//! `votingengine` 核心 FRAME benchmark。
//!
//! mode-specific 写票由各 Track benchmark；这里测量统一超时入口、管理员恢复入口和
//! 异步执行队列框架成本。业务执行最重的 `set_code` 另由 WeightInfo 显式叠加系统权重。
#![cfg(feature = "runtime-benchmarks")]

use frame_benchmarking::v2::*;
use frame_system::RawOrigin;
use sp_runtime::traits::SaturatedConversion;

use crate::{
    pallet::{Config, PendingProposalExecutions, ProposalExecutionRetryStates, Proposals},
    Call, ExecutionRetryState, Pallet, PendingExecutionState, Proposal, STATUS_PASSED,
    STATUS_VOTING,
};

fn setup<T: Config>(status: u8) -> (u64, T::AccountId) {
    let proposal_id = 0u64;
    let institution: T::AccountId = account("institution", 0, 0);
    let who: T::AccountId = account("admin", 0, 0);
    let actor_cid_number: crate::types::CidNumber = b"BENCHMARK-CID"
        .to_vec()
        .try_into()
        .expect("benchmark CID fits runtime bound");
    let now = 1u32.saturated_into();
    frame_system::Pallet::<T>::set_block_number(now);
    Proposals::<T>::insert(
        proposal_id,
        Proposal {
            kind: crate::PROPOSAL_KIND_INTERNAL,
            stage: crate::STAGE_INTERNAL,
            status,
            internal_code: Some(primitives::cid::code::PMUL),
            actor_cid_number: Some(actor_cid_number.clone()),
            execution_account_id: Some(institution.clone()),
            subject_cid_numbers: Default::default(),
            start: 0u32.saturated_into(),
            end: 0u32.saturated_into(),
        },
    );
    let role_subject = crate::types::RoleSubject {
        cid_number: actor_cid_number.clone(),
        role_code: b"BENCHMARK_ROLE"
            .to_vec()
            .try_into()
            .expect("benchmark role fits"),
    };
    let subject = crate::types::AuthorizationSubject::Institution(role_subject.clone());
    crate::Pallet::<T>::snapshot_role_voters(
        proposal_id,
        subject.clone(),
        sp_std::vec![who.clone()],
    )
    .expect("benchmark role snapshot");
    let owner: frame_support::BoundedVec<
        u8,
        frame_support::traits::ConstU32<{ entity_primitives::BUSINESS_MODULE_TAG_MAX_BYTES }>,
    > = b"benchmark".to_vec().try_into().expect("benchmark owner");
    let plan = crate::types::VotePlanOf::try_new(
        entity_primitives::BusinessActionId {
            module_tag: owner.clone(),
            action_code: 0,
        },
        owner,
        subject.clone(),
        sp_std::vec![subject],
        crate::types::VotingEngineKind::Internal,
        [0u8; 32],
    )
    .expect("benchmark vote plan");
    crate::Pallet::<T>::bind_vote_plan(proposal_id, plan).expect("bind benchmark vote plan");
    if status == STATUS_PASSED {
        ProposalExecutionRetryStates::<T>::insert(
            proposal_id,
            ExecutionRetryState {
                manual_attempts: 0,
                first_auto_failed_at: now,
                retry_deadline: now,
                last_attempt_at: None,
            },
        );
    }
    (proposal_id, who)
}

#[benchmarks]
mod benchmarks {
    use super::*;

    #[benchmark]
    fn finalize_proposal() {
        let (proposal_id, who) = setup::<T>(STATUS_VOTING);

        #[extrinsic_call]
        _(RawOrigin::Signed(who), proposal_id);

        assert_eq!(
            Proposals::<T>::get(proposal_id).map(|proposal| proposal.status),
            Some(crate::STATUS_REJECTED)
        );
    }

    #[benchmark]
    fn retry_passed_proposal() {
        let (proposal_id, who) = setup::<T>(STATUS_PASSED);

        #[block]
        {
            let _ = Pallet::<T>::retry_passed_proposal(RawOrigin::Signed(who).into(), proposal_id);
        }

        assert!(Proposals::<T>::contains_key(proposal_id));
    }

    #[benchmark]
    fn cancel_passed_proposal() {
        let (proposal_id, who) = setup::<T>(STATUS_PASSED);
        let reason = sp_std::vec::Vec::new()
            .try_into()
            .expect("empty reason fits");

        #[block]
        {
            let _ = Pallet::<T>::cancel_passed_proposal(
                RawOrigin::Signed(who).into(),
                proposal_id,
                reason,
            );
        }

        assert!(Proposals::<T>::contains_key(proposal_id));
    }

    #[benchmark]
    fn process_pending_execution() {
        let (proposal_id, _) = setup::<T>(STATUS_PASSED);
        let now = frame_system::Pallet::<T>::block_number();
        PendingProposalExecutions::<T>::insert(
            proposal_id,
            PendingExecutionState {
                attempts: 0,
                next_attempt_at: now,
            },
        );

        #[block]
        {
            let _ = Pallet::<T>::process_pending_proposal_executions(now);
        }

        assert!(PendingProposalExecutions::<T>::contains_key(proposal_id));
    }
}
