//! `internalvote` FRAME benchmark。
//!
//! `cast` 覆盖命中门槛并进入异步执行队列的最后一票；超时用例覆盖拒绝终态、
//! 业务拒绝通知、名额释放与 90 天清理登记。
#![cfg(feature = "runtime-benchmarks")]

use frame_benchmarking::v2::*;
use frame_system::RawOrigin;
use sp_runtime::traits::SaturatedConversion;
use engingcore::InternalAdminProvider;

use crate::{
    pallet::{Config, InternalThresholdSnapshot, Pallet},
    Call, Proposals,
};

fn setup<T: Config>() -> (
    u128,
    T::AccountId,
    engingcore::Proposal<frame_system::pallet_prelude::BlockNumberFor<T>, T::AccountId>,
) {
    let proposal_id = 0u128;
    let institution: T::AccountId = account("institution", 0, 0);
    let voter: T::AccountId = account("voter", 0, 0);
    let actor_cid_number: engingcore::types::CidNumber = primitives::cid::china::china_lf::CHINA_LF
        [1]
    .cid_number
    .as_bytes()
    .to_vec()
    .try_into()
    .unwrap();
    <T as engingcore::Config>::InternalAdminProvider::benchmark_seed_institution_voter(
        actor_cid_number.as_slice(),
        &voter,
    );
    let now = 1u32.saturated_into();
    frame_system::Pallet::<T>::set_block_number(now);
    let proposal = engingcore::Proposal {
        kind: engingcore::PROPOSAL_KIND_INTERNAL,
        stage: engingcore::STAGE_INTERNAL,
        status: engingcore::STATUS_VOTING,
        internal_code: Some(primitives::cid::code::PMUL),
        actor_cid_number: Some(actor_cid_number.clone()),
        execution_account_id: Some(institution.clone()),
        subject_cid_numbers: Default::default(),
        start: 0u32.saturated_into(),
        end: 2u32.saturated_into(),
    };
    Proposals::<T>::insert(proposal_id, proposal.clone());
    let role_subject = engingcore::RoleSubject {
        cid_number: actor_cid_number.clone(),
        role_code: b"BENCHMARK_ROLE"
            .to_vec()
            .try_into()
            .expect("benchmark role fits"),
    };
    engingcore::Pallet::<T>::snapshot_role_voters(
        proposal_id,
        engingcore::AuthorizationSubject::Institution(role_subject),
        sp_runtime::sp_std::vec![voter.clone()],
    )
    .expect("benchmark role snapshot");
    InternalThresholdSnapshot::<T>::insert(proposal_id, 1);
    (proposal_id, voter, proposal)
}

#[benchmarks]
mod benchmarks {
    use super::*;

    #[benchmark]
    fn cast() {
        let (proposal_id, voter, _) = setup::<T>();

        #[extrinsic_call]
        _(
            RawOrigin::Signed(voter),
            proposal_id,
            crate::InternalVoteTicketClaim::InstitutionRole(
                b"BENCHMARK_ROLE"
                    .to_vec()
                    .try_into()
                    .expect("benchmark role fits"),
            ),
            true,
        );

        assert!(engingcore::pallet::PendingProposalExecutions::<T>::contains_key(proposal_id));
    }

    #[benchmark]
    fn finalize_internal_timeout() {
        let (proposal_id, _, proposal) = setup::<T>();
        frame_system::Pallet::<T>::set_block_number(3u32.saturated_into());

        #[block]
        {
            Pallet::<T>::do_finalize_internal_timeout(&proposal, proposal_id)
                .expect("expired internal proposal finalizes");
        }

        assert_eq!(
            Proposals::<T>::get(proposal_id).map(|item| item.status),
            Some(engingcore::STATUS_REJECTED)
        );
    }
    #[benchmark]
    fn cleanup_votes(n: Linear<1, 256>) {
        for i in 0..=n {
            let mut bytes = sp_runtime::sp_std::vec![b'C';32];
            bytes[..4].copy_from_slice(&i.to_le_bytes());
            let cid: engingcore::CidNumber = bytes.try_into().unwrap();
            let _ticket = engingcore::InstitutionVoteTicket {
                role_subject: engingcore::RoleSubject {
                    cid_number: cid.clone(),
                    role_code: sp_runtime::sp_std::vec![b'R';64].try_into().unwrap(),
                },
                voter_account_id: account::<T::AccountId>("cleanup", i, 0),
            };
            crate::InternalVotesByTicket::<T>::insert(
                0u128,
                crate::InternalVoteTicket::Institution(_ticket),
                true,
            );
        }
        let result;
        #[block]
        {
            result = <Pallet<T> as engingcore::ProposalTrackHandler<
                frame_system::pallet_prelude::BlockNumberFor<T>,
                T::AccountId,
            >>::cleanup_chunk(engingcore::PROPOSAL_KIND_INTERNAL, 0, n);
        }
        assert_eq!(result.expect("清理类型必须被认领").0, n);
    }

    #[benchmark]
    fn cleanup_terminal() {
        crate::InternalTallies::<T>::insert(0u128, engingcore::VoteCountU32 { yes: 1, no: 0 });
        crate::InternalThresholdSnapshot::<T>::insert(0u128, 2u32);
        crate::PendingPersonalThresholds::<T>::insert(0u128, 2u32);
        crate::PendingPersonalAdminChangeThresholds::<T>::insert(
            0u128,
            crate::PendingPersonalAdminChangeThreshold {
                personal_account_id: account::<T::AccountId>("owner", 0, 0),
                new_admins_len: 3,
                new_threshold: 2,
            },
        );
        crate::InternalProposalRoles::<T>::insert(
            0u128,
            crate::InternalProposalRole::PersonalAdminChange,
        );
        #[block]
        {
            assert!(<Pallet<T> as engingcore::ProposalTrackHandler<
                frame_system::pallet_prelude::BlockNumberFor<T>,
                T::AccountId,
            >>::cleanup_terminal(engingcore::PROPOSAL_KIND_INTERNAL, 0)
            .is_some());
        }
    }
}
