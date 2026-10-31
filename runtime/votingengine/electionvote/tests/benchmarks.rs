//! `electionvote` FRAME benchmark。
//!
//! 两个用例都构造“最后一票”路径，并按候选人数 `c` 读取全部候选计票、生成结果，
//! 覆盖普通写票之外最重的终结分支。

#![cfg(feature = "runtime-benchmarks")]

use codec::Decode;
use frame_benchmarking::v2::*;
use frame_system::RawOrigin;
use sp_runtime::traits::SaturatedConversion;
use votebase::{CitizenIdentityReader, InternalAdminProvider};

use crate::{
    pallet::{
        Config, ElectionCandidates, ElectionMetaStore, ElectionResults, ElectionTallyStore, Pallet,
    },
    types::{ElectionMeta, ElectionMode, ElectionTally},
    Call,
};

fn setup_election<T: Config>(
    c: u32,
    mode: ElectionMode,
) -> (u64, T::AccountId, votebase::CitizenSubject<T::AccountId>) {
    let proposal_id = 0u64;
    // 互选入口会按当前机构管理员真源解析签名账户，不能使用任意 benchmark 账户。
    // fresh spec 中 NRC 第一个在册管理员同时适用于普选夹具。
    let voter =
        T::AccountId::decode(&mut &primitives::cid::china::china_cb::CHINA_CB[0].admins[0][..])
            .expect("NRC benchmark admin account decodes");
    let actor_cid_number: votebase::CidNumber = primitives::cid::china::china_cb::CHINA_CB[0]
        .cid_number
        .as_bytes()
        .to_vec()
        .try_into()
        .expect("NRC CID fits runtime bound");
    let scope = votebase::PopulationScope::Country;
    let candidate_accounts: sp_std::vec::Vec<T::AccountId> =
        (0..c).map(|index| account("candidate", index, 0)).collect();
    let candidates: sp_std::vec::Vec<votebase::CitizenSubject<T::AccountId>> = candidate_accounts
        .iter()
        .map(|candidate| {
            <T as votebase::Config>::CitizenIdentityReader::benchmark_seed_identity(
                candidate, &scope,
            );
            <T as votebase::Config>::CitizenIdentityReader::candidate_subject(candidate, &scope)
                .expect("benchmark candidate has a valid candidate identity")
        })
        .collect();
    let selected = candidates[0].clone();
    let bounded: frame_support::BoundedVec<
        votebase::CitizenSubject<T::AccountId>,
        T::MaxElectionCandidates,
    > = candidates
        .try_into()
        .expect("runtime candidate bound covers benchmark range");
    let now = 1u32.saturated_into();
    frame_system::Pallet::<T>::set_block_number(now);
    votebase::pallet::Proposals::<T>::insert(
        proposal_id,
        votebase::Proposal {
            kind: votebase::PROPOSAL_KIND_ELECTION,
            stage: mode.stage(),
            status: votebase::STATUS_VOTING,
            internal_code: None,
            actor_cid_number: Some(actor_cid_number.clone()),
            execution_account_id: None,
            subject_cid_numbers: Default::default(),
            start: now,
            end: 2u32.saturated_into(),
        },
    );
    ElectionMetaStore::<T>::insert(
        proposal_id,
        ElectionMeta {
            mode,
            population_scope: (mode == ElectionMode::Popular)
                .then_some(votebase::PopulationScope::Country),
            actor_cid_number: actor_cid_number.clone(),
            role_code: b"BENCHMARK_TARGET"
                .to_vec()
                .try_into()
                .expect("benchmark target role code"),
            seat_count: 1,
            term_start: 0,
            term_end: 1,
        },
    );
    ElectionCandidates::<T>::insert(proposal_id, bounded);
    if mode == ElectionMode::Popular {
        <T as votebase::Config>::CitizenIdentityReader::benchmark_seed_identity(&voter, &scope);
        let mut population_data =
            <T as votebase::Config>::CitizenIdentityReader::population_data(&scope)
                .expect("benchmark population data is ready");
        // 本用例只压测最后一票终结路径；候选身份不属于本场选民分母。
        population_data.eligible_total = 1;
        votebase::ProposalPopulationSnapshots::<T>::insert(
            proposal_id,
            votebase::ProposalPopulationSnapshot {
                population_data,
                created_at: now,
            },
        );
    } else {
        <T as votebase::Config>::InternalAdminProvider::benchmark_seed_institution_voter(
            actor_cid_number.as_slice(),
            &voter,
        );
        let subject = votebase::AuthorizationSubject::Institution(entitybase::RoleSubject {
            cid_number: actor_cid_number,
            role_code: b"BENCHMARK_MEMBER"
                .to_vec()
                .try_into()
                .expect("benchmark role code"),
        });
        let canonical_voter =
            <T as votebase::Config>::InternalAdminProvider::resolve_institution_voter(
                match &subject {
                    votebase::AuthorizationSubject::Institution(role) => role.cid_number.as_slice(),
                    votebase::AuthorizationSubject::PersonalMultisig(_) => unreachable!(),
                },
                &voter,
            )
            .expect("benchmark institution voter resolves to a canonical admin account");
        votebase::Pallet::<T>::snapshot_role_voters(
            proposal_id,
            subject.clone(),
            sp_runtime::sp_std::vec![canonical_voter],
        )
        .expect("benchmark mutual role snapshot");
        assert!(
            votebase::Pallet::<T>::is_subject_voter_in_snapshot(proposal_id, subject, &voter,),
            "benchmark mutual voter must pass the production snapshot resolver"
        );
    }
    ElectionTallyStore::<T>::insert(proposal_id, ElectionTally::default());
    (proposal_id, voter, selected)
}

#[benchmarks]
mod benchmarks {
    use super::*;

    #[benchmark]
    fn cast_popular_vote(c: Linear<1, 256>) {
        let (proposal_id, voter, candidate) = setup_election::<T>(c, ElectionMode::Popular);

        #[extrinsic_call]
        _(RawOrigin::Signed(voter), proposal_id, candidate);

        assert!(ElectionResults::<T>::contains_key(proposal_id));
    }

    #[benchmark]
    fn cast_mutual_vote(c: Linear<1, 256>) {
        let (proposal_id, voter, candidate) = setup_election::<T>(c, ElectionMode::Mutual);

        #[extrinsic_call]
        _(
            RawOrigin::Signed(voter),
            proposal_id,
            b"BENCHMARK_MEMBER"
                .to_vec()
                .try_into()
                .expect("benchmark role code"),
            candidate,
        );

        assert!(ElectionResults::<T>::contains_key(proposal_id));
    }
    #[benchmark]
    fn cleanup_popular(n: Linear<1, 256>) {
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
            crate::PopularElectionVotesByCid::<T>::insert(
                0u64,
                cid,
                benchmark_value::<crate::types::PopularElectionVote<T::AccountId>>(),
            );
        }
        let result;
        #[block]
        {
            result = <Pallet<T> as votebase::ProposalTrackHandler<
                frame_system::pallet_prelude::BlockNumberFor<T>,
                T::AccountId,
            >>::cleanup_chunk(votebase::PROPOSAL_KIND_ELECTION, 0, n);
        }
        assert_eq!(result.expect("清理类型必须被认领").0, n);
    }

    #[benchmark]
    fn cleanup_mutual(n: Linear<1, 256>) {
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
            crate::MutualElectionVotesByTicket::<T>::insert(
                0u64,
                _ticket,
                benchmark_value::<votebase::CitizenSubject<T::AccountId>>(),
            );
        }
        let result;
        #[block]
        {
            result = <Pallet<T> as votebase::ProposalTrackHandler<
                frame_system::pallet_prelude::BlockNumberFor<T>,
                T::AccountId,
            >>::cleanup_chunk(votebase::PROPOSAL_KIND_ELECTION, 0, n);
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
            crate::ElectionCandidateTallies::<T>::insert(0u64, cid, 1u32);
        }
        let result;
        #[block]
        {
            result = <Pallet<T> as votebase::ProposalTrackHandler<
                frame_system::pallet_prelude::BlockNumberFor<T>,
                T::AccountId,
            >>::cleanup_chunk(votebase::PROPOSAL_KIND_ELECTION, 0, n);
        }
        assert_eq!(result.expect("清理类型必须被认领").0, n);
    }

    #[benchmark]
    fn cleanup_terminal() {
        crate::ElectionMetaStore::<T>::insert(
            0u64,
            benchmark_value::<crate::types::ElectionMeta>(),
        );
        crate::ElectionCandidates::<T>::insert(
            0u64,
            frame_support::BoundedVec::<
                votebase::CitizenSubject<T::AccountId>,
                T::MaxElectionCandidates,
            >::default(),
        );
        crate::ElectionResults::<T>::insert(
            0u64,
            frame_support::BoundedVec::<
                crate::types::ElectionWinner<T::AccountId>,
                T::MaxElectionCandidates,
            >::default(),
        );
        #[block]
        {
            assert!(<Pallet<T> as votebase::ProposalTrackHandler<
                frame_system::pallet_prelude::BlockNumberFor<T>,
                T::AccountId,
            >>::cleanup_terminal(votebase::PROPOSAL_KIND_ELECTION, 0)
            .is_some());
        }
    }
}

// 清理阶段只消费键和值；计时区间始终执行所属真实清理路由。
fn benchmark_value<V: codec::Decode>() -> V {
    V::decode(&mut sp_runtime::traits::TrailingZeroInput::zeroes()).expect("基准类型必须可构造")
}
