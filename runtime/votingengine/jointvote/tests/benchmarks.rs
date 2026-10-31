//! `jointvote` FRAME benchmark。
//!
//! 覆盖机构形成最终票、公民公投写票及两个超时阶段；人口快照随提案创建内联生成。
#![cfg(feature = "runtime-benchmarks")]

use codec::Decode;
use frame_benchmarking::v2::*;
use frame_system::RawOrigin;
use sp_runtime::traits::SaturatedConversion;
use votebase::CitizenIdentityReader;

use crate::pallet::{
    Config, JointInstitutionTallies, JointVotesByInstitution, Pallet, ReferendumVotesByCid,
};
use crate::Call;

fn decode<T: frame_system::Config>(raw: &[u8; 32]) -> T::AccountId {
    T::AccountId::decode(&mut &raw[..]).expect("fixed governance account decodes")
}

fn setup_proposal<T: Config>(
    stage: u8,
) -> (
    u64,
    votebase::Proposal<frame_system::pallet_prelude::BlockNumberFor<T>, T::AccountId>,
) {
    let proposal_id = 0u64;
    let nrc = &primitives::cid::china::china_cb::CHINA_CB[0];
    let now = 1u32.saturated_into();
    frame_system::Pallet::<T>::set_block_number(now);
    let proposal = votebase::Proposal {
        kind: votebase::PROPOSAL_KIND_JOINT,
        stage,
        status: votebase::STATUS_VOTING,
        internal_code: None,
        actor_cid_number: Some(
            nrc.cid_number
                .as_bytes()
                .to_vec()
                .try_into()
                .expect("NRC CID fits"),
        ),
        execution_account_id: None,
        subject_cid_numbers: Default::default(),
        start: 0u32.saturated_into(),
        end: 2u32.saturated_into(),
    };
    votebase::pallet::Proposals::<T>::insert(proposal_id, proposal.clone());
    (proposal_id, proposal)
}

#[benchmarks]
mod benchmarks {
    use super::*;

    #[benchmark]
    fn cast_admin() {
        let (proposal_id, _) = setup_proposal::<T>(votebase::STAGE_JOINT);
        let entry = &primitives::cid::china::china_cb::CHINA_CB[0];
        let actor_cid_number: votebase::CidNumber = entry
            .cid_number
            .as_bytes()
            .to_vec()
            .try_into()
            .expect("NRC CID fits runtime bound");
        let voter = decode::<T>(&entry.admins[0]);
        let voter_role_code: votebase::RoleCode =
            primitives::governance_skeleton::ROLE_CODE_COMMITTEE_MEMBER
                .to_vec()
                .try_into()
                .expect("committee role fits");
        votebase::Pallet::<T>::snapshot_role_voters(
            proposal_id,
            votebase::AuthorizationSubject::Institution(votebase::RoleSubject {
                cid_number: actor_cid_number.clone(),
                role_code: voter_role_code.clone(),
            }),
            entry.admins.iter().map(decode::<T>).collect(),
        )
        .expect("NRC role snapshot");
        let threshold =
            votebase::fixed_governance_pass_threshold(&votebase::NRC).expect("NRC threshold");
        JointInstitutionTallies::<T>::insert(
            proposal_id,
            &actor_cid_number,
            votebase::VoteCountU32 {
                yes: threshold.saturating_sub(1),
                no: 0,
            },
        );

        #[extrinsic_call]
        _(
            RawOrigin::Signed(voter),
            proposal_id,
            actor_cid_number.clone(),
            voter_role_code,
            true,
        );

        assert!(JointVotesByInstitution::<T>::contains_key(
            proposal_id,
            actor_cid_number
        ));
    }

    #[benchmark]
    fn cast_referendum() {
        let (proposal_id, _) = setup_proposal::<T>(votebase::STAGE_REFERENDUM);
        let voter: T::AccountId = account("citizen", 0, 0);
        let scope = votebase::PopulationScope::Country;
        <T as votebase::Config>::CitizenIdentityReader::benchmark_seed_identity(&voter, &scope);
        let voter_subject =
            <T as votebase::Config>::CitizenIdentityReader::voting_subject(&voter, &scope)
                .expect("benchmark citizen subject should exist");
        votebase::Pallet::<T>::create_population_snapshot(proposal_id, &scope)
            .expect("benchmark proposal should create population snapshot");

        #[extrinsic_call]
        _(RawOrigin::Signed(voter.clone()), proposal_id, true);

        assert!(ReferendumVotesByCid::<T>::contains_key(
            proposal_id,
            voter_subject.cid_number
        ));
    }

    #[benchmark]
    fn finalize_joint_timeout() {
        let (proposal_id, proposal) = setup_proposal::<T>(votebase::STAGE_JOINT);
        let population_seed: T::AccountId = account("population", 0, 0);
        <T as votebase::Config>::CitizenIdentityReader::benchmark_seed_identity(
            &population_seed,
            &votebase::PopulationScope::Country,
        );
        votebase::Pallet::<T>::create_population_snapshot(
            proposal_id,
            &votebase::PopulationScope::Country,
        )
        .expect("joint proposal population snapshot");
        frame_system::Pallet::<T>::set_block_number(3u32.saturated_into());

        #[block]
        {
            Pallet::<T>::do_finalize_joint_timeout(&proposal, proposal_id)
                .expect("expired joint stage advances");
        }

        assert_eq!(
            votebase::pallet::Proposals::<T>::get(proposal_id).map(|item| item.stage),
            Some(votebase::STAGE_REFERENDUM)
        );
    }

    #[benchmark]
    fn finalize_jointreferendum_timeout() {
        let (proposal_id, proposal) = setup_proposal::<T>(votebase::STAGE_REFERENDUM);
        let population_seed: T::AccountId = account("population", 0, 0);
        <T as votebase::Config>::CitizenIdentityReader::benchmark_seed_identity(
            &population_seed,
            &votebase::PopulationScope::Country,
        );
        votebase::Pallet::<T>::create_population_snapshot(
            proposal_id,
            &votebase::PopulationScope::Country,
        )
        .expect("joint proposal population snapshot");
        frame_system::Pallet::<T>::set_block_number(3u32.saturated_into());

        #[block]
        {
            Pallet::<T>::do_finalize_jointreferendum_timeout(&proposal, proposal_id)
                .expect("expired referendum finalizes");
        }

        assert_eq!(
            votebase::pallet::Proposals::<T>::get(proposal_id).map(|item| item.status),
            Some(votebase::STATUS_REJECTED)
        );
    }
    #[benchmark]
    fn cleanup_votes(n: Linear<1, 256>) {
        for i in 0..=n {
            let mut bytes = sp_runtime::sp_std::vec![b'C';32];
            bytes[..4].copy_from_slice(&i.to_le_bytes());
            let cid: votebase::CidNumber = bytes.try_into().unwrap();
            let _ticket = votebase::InstitutionVoteTicket {
                role_subject: votebase::RoleSubject {
                    cid_number: cid.clone(),
                    role_code: sp_runtime::sp_std::vec![b'R';64].try_into().unwrap(),
                },
                voter_account_id: account::<T::AccountId>("cleanup", i, 0),
            };
            crate::JointVotesByTicket::<T>::insert(0u64, _ticket, true);
        }
        let result;
        #[block]
        {
            result = <Pallet<T> as votebase::ProposalTrackHandler<
                frame_system::pallet_prelude::BlockNumberFor<T>,
                T::AccountId,
            >>::cleanup_chunk(votebase::PROPOSAL_KIND_JOINT, 0, n);
        }
        assert_eq!(result.expect("清理类型必须被认领").0, n);
    }

    #[benchmark]
    fn cleanup_institutions(n: Linear<1, 256>) {
        for i in 0..=n {
            let mut bytes = sp_runtime::sp_std::vec![b'C';32];
            bytes[..4].copy_from_slice(&i.to_le_bytes());
            let cid: votebase::CidNumber = bytes.try_into().unwrap();
            let _ticket = votebase::InstitutionVoteTicket {
                role_subject: votebase::RoleSubject {
                    cid_number: cid.clone(),
                    role_code: sp_runtime::sp_std::vec![b'R';64].try_into().unwrap(),
                },
                voter_account_id: account::<T::AccountId>("cleanup", i, 0),
            };
            crate::JointVotesByInstitution::<T>::insert(0u64, cid, true);
        }
        let result;
        #[block]
        {
            result = <Pallet<T> as votebase::ProposalTrackHandler<
                frame_system::pallet_prelude::BlockNumberFor<T>,
                T::AccountId,
            >>::cleanup_chunk(votebase::PROPOSAL_KIND_JOINT, 0, n);
        }
        assert_eq!(result.expect("清理类型必须被认领").0, n);
    }

    #[benchmark]
    fn cleanup_tallies(n: Linear<1, 256>) {
        for i in 0..=n {
            let mut bytes = sp_runtime::sp_std::vec![b'C';32];
            bytes[..4].copy_from_slice(&i.to_le_bytes());
            let cid: votebase::CidNumber = bytes.try_into().unwrap();
            let _ticket = votebase::InstitutionVoteTicket {
                role_subject: votebase::RoleSubject {
                    cid_number: cid.clone(),
                    role_code: sp_runtime::sp_std::vec![b'R';64].try_into().unwrap(),
                },
                voter_account_id: account::<T::AccountId>("cleanup", i, 0),
            };
            crate::JointInstitutionTallies::<T>::insert(
                0u64,
                cid,
                votebase::VoteCountU32 { yes: 1, no: 0 },
            );
        }
        let result;
        #[block]
        {
            result = <Pallet<T> as votebase::ProposalTrackHandler<
                frame_system::pallet_prelude::BlockNumberFor<T>,
                T::AccountId,
            >>::cleanup_chunk(votebase::PROPOSAL_KIND_JOINT, 0, n);
        }
        assert_eq!(result.expect("清理类型必须被认领").0, n);
    }

    #[benchmark]
    fn cleanup_referendum(n: Linear<1, 256>) {
        for i in 0..=n {
            let mut bytes = sp_runtime::sp_std::vec![b'C';32];
            bytes[..4].copy_from_slice(&i.to_le_bytes());
            let cid: votebase::CidNumber = bytes.try_into().unwrap();
            let _ticket = votebase::InstitutionVoteTicket {
                role_subject: votebase::RoleSubject {
                    cid_number: cid.clone(),
                    role_code: sp_runtime::sp_std::vec![b'R';64].try_into().unwrap(),
                },
                voter_account_id: account::<T::AccountId>("cleanup", i, 0),
            };
            crate::ReferendumVotesByCid::<T>::insert(
                0u64,
                cid,
                benchmark_value::<votebase::CitizenReferendumTicket<T::AccountId>>(),
            );
        }
        let result;
        #[block]
        {
            result = <Pallet<T> as votebase::ProposalTrackHandler<
                frame_system::pallet_prelude::BlockNumberFor<T>,
                T::AccountId,
            >>::cleanup_chunk(votebase::PROPOSAL_KIND_JOINT, 0, n);
        }
        assert_eq!(result.expect("清理类型必须被认领").0, n);
    }

    #[benchmark]
    fn cleanup_terminal() {
        crate::JointTallies::<T>::insert(0u64, votebase::VoteCountU32 { yes: 1, no: 0 });
        crate::ReferendumTallies::<T>::insert(0u64, votebase::VoteCountU64 { yes: 1, no: 0 });
        #[block]
        {
            assert!(<Pallet<T> as votebase::ProposalTrackHandler<
                frame_system::pallet_prelude::BlockNumberFor<T>,
                T::AccountId,
            >>::cleanup_terminal(votebase::PROPOSAL_KIND_JOINT, 0)
            .is_some());
        }
    }
}

// 清理阶段只消费键和值；计时区间始终执行所属真实清理路由。
fn benchmark_value<V: codec::Decode>() -> V {
    V::decode(&mut sp_runtime::traits::TrailingZeroInput::zeroes()).expect("基准类型必须可构造")
}
