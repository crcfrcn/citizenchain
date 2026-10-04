//! `election-vote` FRAME benchmark。
//!
//! 两个用例都构造“最后一票”路径，并按候选人数 `c` 读取全部候选计票、生成结果，
//! 覆盖普通写票之外最重的终结分支。

#![cfg(feature = "runtime-benchmarks")]

use codec::Decode;
use frame_benchmarking::v2::*;
use frame_system::RawOrigin;
use sp_runtime::traits::SaturatedConversion;
use votingengine::{CitizenIdentityReader, InternalAdminProvider};

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
) -> (
    u64,
    T::AccountId,
    votingengine::CitizenSubject<T::AccountId>,
) {
    let proposal_id = 0u64;
    // 互选入口会按当前机构管理员真源解析签名账户，不能使用任意 benchmark 账户。
    // fresh spec 中 NRC 第一个在册管理员同时适用于普选夹具。
    let voter =
        T::AccountId::decode(&mut &primitives::cid::china::china_cb::CHINA_CB[0].admins[0][..])
            .expect("NRC benchmark admin account decodes");
    let actor_cid_number: votingengine::CidNumber = primitives::cid::china::china_cb::CHINA_CB[0]
        .cid_number
        .as_bytes()
        .to_vec()
        .try_into()
        .expect("NRC CID fits runtime bound");
    let scope = votingengine::PopulationScope::Country;
    let candidate_accounts: sp_std::vec::Vec<T::AccountId> =
        (0..c).map(|index| account("candidate", index, 0)).collect();
    let candidates: sp_std::vec::Vec<votingengine::CitizenSubject<T::AccountId>> =
        candidate_accounts
            .iter()
            .map(|candidate| {
                <T as votingengine::Config>::CitizenIdentityReader::benchmark_seed_identity(
                    candidate, &scope,
                );
                <T as votingengine::Config>::CitizenIdentityReader::candidate_subject(
                    candidate, &scope,
                )
                .expect("benchmark candidate has a valid candidate identity")
            })
            .collect();
    let selected = candidates[0].clone();
    let bounded: frame_support::BoundedVec<
        votingengine::CitizenSubject<T::AccountId>,
        T::MaxElectionCandidates,
    > = candidates
        .try_into()
        .expect("runtime candidate bound covers benchmark range");
    let now = 1u32.saturated_into();
    frame_system::Pallet::<T>::set_block_number(now);
    votingengine::pallet::Proposals::<T>::insert(
        proposal_id,
        votingengine::Proposal {
            kind: votingengine::PROPOSAL_KIND_ELECTION,
            stage: mode.stage(),
            status: votingengine::STATUS_VOTING,
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
                .then_some(votingengine::PopulationScope::Country),
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
        <T as votingengine::Config>::CitizenIdentityReader::benchmark_seed_identity(&voter, &scope);
        let mut population_data =
            <T as votingengine::Config>::CitizenIdentityReader::population_data(&scope)
                .expect("benchmark population data is ready");
        // 本用例只压测最后一票终结路径；候选身份不属于本场选民分母。
        population_data.eligible_total = 1;
        votingengine::ProposalPopulationSnapshots::<T>::insert(
            proposal_id,
            votingengine::ProposalPopulationSnapshot {
                population_data,
                created_at: now,
            },
        );
    } else {
        <T as votingengine::Config>::InternalAdminProvider::benchmark_seed_institution_voter(
            actor_cid_number.as_slice(),
            &voter,
        );
        let subject =
            votingengine::AuthorizationSubject::Institution(entity_primitives::RoleSubject {
                cid_number: actor_cid_number,
                role_code: b"BENCHMARK_MEMBER"
                    .to_vec()
                    .try_into()
                    .expect("benchmark role code"),
            });
        let canonical_voter =
            <T as votingengine::Config>::InternalAdminProvider::resolve_institution_voter(
                match &subject {
                    votingengine::AuthorizationSubject::Institution(role) => {
                        role.cid_number.as_slice()
                    }
                    votingengine::AuthorizationSubject::PersonalMultisig(_) => unreachable!(),
                },
                &voter,
            )
            .expect("benchmark institution voter resolves to a canonical admin account");
        votingengine::Pallet::<T>::snapshot_role_voters(
            proposal_id,
            subject.clone(),
            sp_std::vec![canonical_voter],
        )
        .expect("benchmark mutual role snapshot");
        assert!(
            votingengine::Pallet::<T>::is_subject_voter_in_snapshot(proposal_id, subject, &voter,),
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
}
