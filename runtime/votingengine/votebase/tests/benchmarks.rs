//! `votebase` 核心 FRAME benchmark。
//!
//! mode-specific 写票由各 Track benchmark；这里测量统一超时入口、管理员恢复入口和
//! 异步执行队列框架成本。业务执行最重的 `set_code` 另由 WeightInfo 显式叠加系统权重。
#![cfg(feature = "runtime-benchmarks")]

use frame_benchmarking::v2::*;
use frame_support::traits::Get;
use frame_system::RawOrigin;
use sp_runtime::traits::{SaturatedConversion, Saturating};

use crate::{
    pallet::{Config, PendingProposalExecutions, ProposalExecutionRetryStates, Proposals},
    Call, ExecutionRetryState, Pallet, PendingExecutionState, Proposal, STATUS_PASSED,
};

pub trait BenchmarkSetup: Config {
    fn recovery(cancel: bool) -> (u64, Self::AccountId);
}
fn recovery<T: BenchmarkSetup>(cancel: bool) -> (u64, T::AccountId) {
    let (id, who) = T::recovery(cancel);
    Proposals::<T>::mutate(id, |proposal| {
        proposal.as_mut().unwrap().status = STATUS_PASSED
    });
    let now = frame_system::Pallet::<T>::block_number();
    ProposalExecutionRetryStates::<T>::insert(
        id,
        ExecutionRetryState {
            manual_attempts: 0,
            first_auto_failed_at: now,
            retry_deadline: now,
            last_attempt_at: None,
        },
    );
    (id, who)
}

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
        frame_support::traits::ConstU32<{ entitybase::BUSINESS_MODULE_TAG_MAX_BYTES }>,
    > = b"benchmark".to_vec().try_into().expect("benchmark owner");
    let plan = crate::types::VotePlanOf::try_new(
        entitybase::BusinessActionId {
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

// 终态清理消费公开CID键；用所属生成协议覆盖既有256主体编码边界。
fn cleanup_cids() -> sp_std::vec::Vec<sp_std::vec::Vec<u8>> {
    (0..crate::MAX_PROPOSAL_SUBJECT_CIDS)
        .map(|index| {
            let mut key = *b"cleanup-000";
            let hex = b"0123456789abcdef";
            key[8] = hex[((index >> 8) & 15) as usize];
            key[9] = hex[((index >> 4) & 15) as usize];
            key[10] = hex[(index & 15) as usize];
            primitives::cid::generator::generate_cid_number(
                primitives::cid::generator::GenerateCidNumberInput {
                    public_key: core::str::from_utf8(&key).unwrap(),
                    p1: "0",
                    province_code: "ZS",
                    province_name: "国家",
                    city_code: "001",
                    city_name: "国家",
                    year: "2026",
                    institution: "CGOV",
                },
            )
            .unwrap()
            .into_bytes()
        })
        .collect()
}

#[benchmarks(where T:BenchmarkSetup)]
mod benchmarks {
    use super::*;

    #[benchmark]
    fn finalize_proposal() {
        let (proposal_id, who) = T::recovery(false);
        let end = Proposals::<T>::get(proposal_id).unwrap().end;
        frame_system::Pallet::<T>::set_block_number(end.saturating_add(1u32.into()));

        #[extrinsic_call]
        _(RawOrigin::Signed(who), proposal_id);

        assert_eq!(
            Proposals::<T>::get(proposal_id).map(|proposal| proposal.status),
            Some(crate::STATUS_REJECTED)
        );
    }

    #[benchmark]
    fn retry_passed_proposal() {
        let (proposal_id, who) = recovery::<T>(false);
        #[extrinsic_call]
        _(RawOrigin::Signed(who), proposal_id);
        assert_eq!(
            Proposals::<T>::get(proposal_id).unwrap().status,
            crate::STATUS_EXECUTED
        );
    }
    #[benchmark]
    fn cancel_passed_proposal() {
        let (proposal_id, who) = recovery::<T>(true);
        let reason = sp_std::vec![b'r';<T as Config>::MaxProposalDataLen::get() as usize]
            .try_into()
            .unwrap();
        #[extrinsic_call]
        _(RawOrigin::Signed(who), proposal_id, reason);
        assert_eq!(
            Proposals::<T>::get(proposal_id).unwrap().status,
            crate::STATUS_EXECUTION_FAILED
        );
    }
    #[benchmark]
    fn process_pending_execution() {
        let (proposal_id, _) = recovery::<T>(false);
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
            Pallet::<T>::process_pending_proposal_executions(
                now,
                T::MaxExecutionWeightPerBlock::get(),
            );
        }
        assert_eq!(
            Proposals::<T>::get(proposal_id).unwrap().status,
            crate::STATUS_EXECUTED
        );
    }
    #[benchmark]
    fn terminal_base() {
        crate::CleanupCursor::<T>::put(
            frame_support::BoundedVec::<u8, frame_support::traits::ConstU32<64>>::try_from(
                crate::PendingTerminalCleanups::<T>::hashed_key_for(u64::MAX),
            )
            .unwrap(),
        );
        #[block]
        {
            Pallet::<T>::process_pending_terminal_cleanups(frame_support::weights::Weight::MAX);
        }
    }
    #[benchmark]
    fn terminal_item() {
        use frame_support::traits::Get;
        let mut data = sp_std::vec![0u8;T::MaxProposalDataLen::get() as usize];
        data[..5].copy_from_slice(b"other");
        Pallet::<T>::benchmark_proposal(0, b"other", data);
        crate::PendingTerminalCleanups::<T>::insert(0u64, ());
        #[block]
        {
            Pallet::<T>::process_pending_terminal_cleanups(frame_support::weights::Weight::MAX);
        }
        assert!(!crate::PendingTerminalCleanups::<T>::contains_key(0u64));
    }
    #[benchmark]
    fn scheduled_base() {
        #[block]
        {
            crate::cleanup::process_scheduled_cleanups::<T>(
                1u32.saturated_into(),
                frame_support::weights::Weight::MAX,
            );
        }
    }
    #[benchmark]
    fn scheduled_item() {
        use frame_support::traits::Get;
        let (id, _) = setup::<T>(crate::STATUS_EXECUTION_FAILED);
        let cids = cleanup_cids();
        Proposals::<T>::mutate(id, |p| {
            p.as_mut().unwrap().actor_cid_number =
                cids.first().cloned().map(|cid| cid.try_into().unwrap());
            p.as_mut().unwrap().subject_cid_numbers = cids
                .iter()
                .cloned()
                .map(|c| c.try_into().unwrap())
                .collect::<sp_std::vec::Vec<_>>()
                .try_into()
                .unwrap();
        });
        for cid in cids {
            let subject = crate::ProposalSubject::InstitutionCid(cid.try_into().unwrap());
            let ids: frame_support::BoundedVec<u64, T::MaxActiveProposals> = (0
                ..T::MaxActiveProposals::get())
                .map(u64::from)
                .collect::<sp_std::vec::Vec<_>>()
                .try_into()
                .unwrap();
            crate::ActiveProposalsBySubject::<T>::insert(subject, ids);
        }
        crate::cleanup::schedule_cleanup::<T>(id, 0u32.into()).unwrap();
        #[block]
        {
            crate::cleanup::process_scheduled_cleanups::<T>(
                u32::MAX.saturated_into(),
                frame_support::weights::Weight::MAX,
            );
        }
        assert_eq!(crate::ScheduledCleanupHead::<T>::get(), 1);
    }
    #[benchmark]
    fn cleanup_base() {
        #[block]
        {
            Pallet::<T>::process_pending_cleanup_steps(frame_support::weights::Weight::MAX);
        }
    }
    #[benchmark]
    fn cleanup_item() {
        Pallet::<T>::benchmark_proposal(0, b"other", b"other".to_vec());
        crate::PendingProposalCleanups::<T>::insert(0u64, crate::PendingCleanupStage::FinalCleanup);
        crate::cleanup::enqueue_pending_cleanup::<T>(0).unwrap();
        let budget = frame_support::weights::Weight::MAX;
        #[block]
        {
            Pallet::<T>::process_pending_cleanup_steps(budget);
        }
        assert_eq!(crate::PendingCleanupQueueHead::<T>::get(), 1);
    }
    #[benchmark]
    fn cleanup_step(n: Linear<1, 256>) {
        use frame_support::traits::Get;
        let accounts: frame_support::BoundedVec<T::AccountId, T::MaxAdminsPerInstitution> = (0
            ..T::MaxAdminsPerInstitution::get())
            .map(|i| account("snapshot-member", i, 0))
            .collect::<sp_std::vec::Vec<_>>()
            .try_into()
            .unwrap();
        for i in 0..=n {
            let mut cid = sp_std::vec![b'C';32];
            cid[..4].copy_from_slice(&i.to_le_bytes());
            let subject = crate::AuthorizationSubject::Institution(crate::RoleSubject {
                cid_number: cid.try_into().unwrap(),
                role_code: sp_std::vec![b'R';64].try_into().unwrap(),
            });
            crate::VoterSnapshot::<T>::insert(0u64, subject, accounts.clone());
        }
        #[block]
        {
            Pallet::<T>::process_pending_cleanup_step(
                0,
                crate::PendingCleanupStage::VoterSnapshots,
                n,
            );
        }
        assert_eq!(crate::VoterSnapshot::<T>::iter_prefix(0u64).count(), 1);
    }
    #[benchmark]
    fn cleanup_object() {
        use frame_support::traits::Get;
        use sp_runtime::traits::Hash;
        let data = sp_std::vec![b'o';T::MaxProposalObjectLen::get() as usize];
        let hash = T::Hashing::hash(&data);
        let len = data.len() as u32;
        let data: frame_support::BoundedVec<u8, T::MaxProposalObjectLen> = data.try_into().unwrap();
        crate::ProposalObject::<T>::insert(0u64, data);
        crate::ProposalObjectMeta::<T>::insert(
            0u64,
            crate::ProposalObjectMetadata {
                kind: 1,
                object_len: len,
                object_hash: hash,
            },
        );
        #[block]
        {
            Pallet::<T>::process_pending_cleanup_step(
                0,
                crate::PendingCleanupStage::ProposalObject,
                1,
            );
        }
        assert!(!crate::ProposalObject::<T>::contains_key(0u64));
        assert!(!crate::ProposalObjectMeta::<T>::contains_key(0u64));
    }
    #[benchmark]
    fn cleanup_final(n: Linear<0, 256>) {
        use frame_support::traits::Get;
        let (id, _) = setup::<T>(crate::STATUS_EXECUTION_FAILED);
        let data: frame_support::BoundedVec<u8, T::MaxProposalDataLen> =
            sp_std::vec![b'd';T::MaxProposalDataLen::get() as usize]
                .try_into()
                .unwrap();
        crate::ProposalData::<T>::insert(id, data);
        let cids = cleanup_cids();
        Proposals::<T>::mutate(id, |p| {
            p.as_mut().unwrap().subject_cid_numbers = cids
                .into_iter()
                .map(|c| c.try_into().unwrap())
                .collect::<sp_std::vec::Vec<_>>()
                .try_into()
                .unwrap();
        });
        for i in 0..n {
            Pallet::<T>::acquire_internal_proposal_mutex(
                id,
                crate::ProposalSubject::PersonalAccount(account("cleanup-mutex", i, 0)),
                crate::InternalProposalMutexKind::Regular,
            )
            .unwrap();
        }
        #[block]
        {
            Pallet::<T>::process_pending_cleanup_step(
                id,
                crate::PendingCleanupStage::FinalCleanup,
                1,
            );
        }
        assert!(!Proposals::<T>::contains_key(id));
        assert!(crate::ProposalMutexBindings::<T>::get(id).is_empty());
    }
    #[benchmark]
    fn expiry_bucket() {
        let ids: frame_support::BoundedVec<u64, T::MaxProposalsPerExpiry> = (0
            ..T::MaxProposalsPerExpiry::get())
            .map(u64::from)
            .collect::<sp_std::vec::Vec<_>>()
            .try_into()
            .unwrap();
        crate::ProposalsByExpiry::<T>::insert(
            1u32.saturated_into::<frame_system::pallet_prelude::BlockNumberFor<T>>(),
            ids,
        );
        #[block]
        {
            Pallet::<T>::auto_finalize_expiry_bucket(
                1u32.into(),
                1u32.into(),
                0,
                frame_support::weights::Weight::MAX,
            );
        }
        assert_eq!(
            crate::ProposalsByExpiry::<T>::get(
                1u32.saturated_into::<frame_system::pallet_prelude::BlockNumberFor<T>>()
            )
            .len(),
            T::MaxProposalsPerExpiry::get() as usize
        );
    }
    #[benchmark]
    fn retry_bucket(n: Linear<0, 2048>) {
        assert!(n <= T::MaxExecutionRetryDeadlinesPerBlock::get());
        let ids: frame_support::BoundedVec<u64, T::MaxExecutionRetryDeadlinesPerBlock> = (0..n)
            .map(u64::from)
            .collect::<sp_std::vec::Vec<_>>()
            .try_into()
            .unwrap();
        let now = 1u32.saturated_into::<frame_system::pallet_prelude::BlockNumberFor<T>>();
        crate::ExecutionRetryDeadlines::<T>::insert(now, ids);
        #[block]
        {
            Pallet::<T>::process_execution_retry_deadlines(now);
        }
        assert!(crate::ExecutionRetryDeadlines::<T>::get(now).is_empty());
        assert_eq!(
            crate::PendingExecutionRetryExpirations::<T>::iter().count(),
            n as usize
        );
    }
}
