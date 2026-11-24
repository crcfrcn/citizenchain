//! 基准测量内部投票最后一票、超时终结及账本清理。

#![cfg(feature = "runtime-benchmarks")]
use engingcore::CitizenIdentityReader as _;

/// 清理基准只需有界且互异的票据键，无须配置身份。
fn cleanup_cid(index: u32) -> engingcore::types::CidNumber {
    let mut bytes = sp_runtime::sp_std::vec![b'C'; 32];
    bytes[..4].copy_from_slice(&index.to_le_bytes());
    bytes.try_into().expect("清理基准 CID 键长度")
}

use frame_benchmarking::v2::*;
use frame_system::RawOrigin;
use sp_runtime::traits::SaturatedConversion;

use crate::{
    pallet::{Config, InternalThresholdSnapshot, Pallet},
    Call, Proposals,
};

fn setup<T: Config>() -> (
    u128,
    T::AccountId,
    engingcore::Proposal<frame_system::pallet_prelude::BlockNumberFor<T>, T::AccountId>,
) {
    let proposal_id = engingcore::Pallet::<T>::allocate_proposal_id().expect("基准提案编号已分配");
    let institution: T::AccountId = account("institution", 0, 0);
    let voter: T::AccountId = account("voter", 0, 0);
    let actor_cid_number: engingcore::types::CidNumber = primitives::cid::china::china_lf::CHINA_LF
        [1]
    .cid_number
    .as_bytes()
    .to_vec()
    .try_into()
    .unwrap();
    // 按生产cast使用的冻结岗位票据和当前身份准备状态，保持基准独立。
    T::CitizenIdentityReader::benchmark_seed_identity(
        &voter,
        &engingcore::PopulationScope::Country,
    );
    let now = 1u32.saturated_into();
    frame_system::Pallet::<T>::set_block_number(now);
    let proposal = engingcore::Proposal {
        kind: engingcore::PROPOSAL_KIND_INTERNAL,
        stage: engingcore::STAGE_INTERNAL,
        status: engingcore::STATUS_VOTING,
        internal_code: Some(
            engingcore::types::institution_code_from_cid_number(
                core::str::from_utf8(actor_cid_number.as_slice()).expect("基准机构CID文本"),
            )
            .expect("基准机构分类"),
        ),
        actor_cid_number: Some(actor_cid_number.clone()),
        execution_account_id: Some(institution.clone()),
        subject_cid_numbers: Default::default(),
        start: 0u32.saturated_into(),
        end: 2u32.saturated_into(),
    };
    Proposals::<T>::insert(proposal_id, proposal.clone());
    engingcore::Pallet::<T>::register_proposal_data(
        proposal_id,
        b"benchmark",
        b"institution-vote".to_vec(),
        now,
    )
    .expect("基准业务归属和载荷完整");
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
        sp_runtime::sp_std::vec![T::CitizenIdentityReader::person_cid(&voter)
            .expect("基准选民身份")
            .into_inner()],
    )
    .expect("benchmark role snapshot");
    InternalThresholdSnapshot::<T>::insert(proposal_id, 1);
    (proposal_id, voter, proposal)
}

#[benchmarks]
mod benchmarks {
    use super::*;

    /// 单独实测个人64人成员快照中的最后一票；生成共用cast权重时取两分支上界。
    #[benchmark(extra)]
    fn cast_personal() {
        let proposal_id =
            engingcore::Pallet::<T>::allocate_proposal_id().expect("基准个人提案编号已分配");
        let count = primitives::multisig::MAX_PERSONAL_ADMINS;
        let members: sp_std::vec::Vec<T::AccountId> = (0..count)
            .map(|i| account("personal-voter", i, 0))
            .collect();
        let voter = members.last().unwrap().clone();
        let owner: T::AccountId = account("personal-owner", 0, 0);
        frame_system::Pallet::<T>::set_block_number(1u32.saturated_into());
        Proposals::<T>::insert(
            proposal_id,
            engingcore::Proposal {
                kind: engingcore::PROPOSAL_KIND_INTERNAL,
                stage: engingcore::STAGE_INTERNAL,
                status: engingcore::STATUS_VOTING,
                internal_code: Some(engingcore::types::PMUL),
                actor_cid_number: None,
                execution_account_id: Some(owner.clone()),
                subject_cid_numbers: Default::default(),
                start: 0u32.saturated_into(),
                end: 2u32.saturated_into(),
            },
        );
        let frozen: frame_support::BoundedVec<
            T::AccountId,
            frame_support::traits::ConstU32<{ primitives::multisig::MAX_PERSONAL_ADMINS }>,
        > = members.clone().try_into().unwrap();
        engingcore::AdminSnapshot::<T>::insert(
            proposal_id,
            engingcore::ProposalSubject::PersonalAccount(owner),
            frozen,
        );
        for member in members.iter().take(count as usize - 1) {
            crate::InternalVotesByTicket::<T>::insert(
                proposal_id,
                crate::InternalVoteTicket::Personal(member.clone()),
                true,
            );
        }
        crate::InternalTallies::<T>::insert(
            proposal_id,
            engingcore::VoteCountU32 {
                yes: count - 1,
                no: 0,
            },
        );
        InternalThresholdSnapshot::<T>::insert(proposal_id, count);
        engingcore::Pallet::<T>::register_proposal_data(
            proposal_id,
            b"benchmark",
            b"personal-vote".to_vec(),
            1u32.saturated_into(),
        )
        .unwrap();
        #[extrinsic_call]
        cast(
            RawOrigin::Signed(voter),
            proposal_id,
            crate::InternalVoteTicketClaim::Personal,
            true,
        );
        assert!(engingcore::pallet::PendingProposalExecutions::<T>::contains_key(proposal_id));
    }

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
                person_cid: cleanup_cid(i),
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
