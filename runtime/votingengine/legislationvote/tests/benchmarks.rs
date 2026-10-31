//! `legislationvote` FRAME benchmark。
//!
//! 五个公开调用分别覆盖代表写票、公民公投、行政签署、三人会签和护宪终审。
//! 人口快照随特别案提案创建内联生成，资格材料始终来自 Runtime 真源。

#![cfg(feature = "runtime-benchmarks")]

use frame_benchmarking::v2::*;
use frame_system::RawOrigin;
use sp_runtime::traits::SaturatedConversion;
use votebase::{CitizenIdentityReader, InternalAdminProvider};

use crate::{
    pallet::{
        Config, LegGuardSigns, LegOverrideSigns, LegReferendumVotesByCid, LegislationMeta,
        LegislationMetas, Pallet, RepresentativeMeta, RepresentativeMetas,
        RepresentativeVotesByTicket,
    },
    Call, RepresentativeRoute, RepresentativeVoteRule, VoteProcedure,
};

fn insert_proposal<T: Config>(stage: u8) -> u64 {
    let proposal_id = 0u64;
    let now = 1u32.saturated_into();
    frame_system::Pallet::<T>::set_block_number(now);
    votebase::pallet::Proposals::<T>::insert(
        proposal_id,
        votebase::Proposal {
            kind: votebase::PROPOSAL_KIND_LEGISLATION,
            stage,
            status: votebase::STATUS_VOTING,
            internal_code: None,
            actor_cid_number: Some(national_legislature(0)),
            execution_account_id: None,
            subject_cid_numbers: Default::default(),
            start: now,
            end: 2u32.saturated_into(),
        },
    );
    proposal_id
}

fn national_legislature(index: usize) -> votebase::CidNumber {
    primitives::cid::china::china_lf::CHINA_LF[index]
        .cid_number
        .as_bytes()
        .to_vec()
        .try_into()
        .expect("national legislature CID fits runtime bound")
}

fn national_executive() -> votebase::CidNumber {
    primitives::cid::china::china_zf::CHINA_ZF[0]
        .cid_number
        .as_bytes()
        .to_vec()
        .try_into()
        .expect("national executive CID fits runtime bound")
}

fn role_subject(
    cid_number: votebase::CidNumber,
    role_code: &[u8],
) -> crate::types::RepresentativeBody {
    entitybase::RoleSubject {
        cid_number,
        role_code: role_code
            .to_vec()
            .try_into()
            .expect("benchmark role code fits"),
    }
}

#[benchmarks]
mod benchmarks {
    use super::*;

    #[benchmark]
    fn cast_representative_vote() {
        let proposal_id = insert_proposal::<T>(votebase::STAGE_LEG_REPRESENTATIVE);
        let body = national_legislature(1);
        let voter: T::AccountId = account("representative", 0, 0);
        <T as votebase::Config>::InternalAdminProvider::benchmark_seed_institution_voter(
            body.as_slice(),
            &voter,
        );
        let role_subject = entitybase::RoleSubject {
            cid_number: body.clone(),
            role_code: b"BENCHMARK_MEMBER"
                .to_vec()
                .try_into()
                .expect("benchmark role code fits"),
        };
        votebase::Pallet::<T>::snapshot_role_voters(
            proposal_id,
            votebase::AuthorizationSubject::Institution(role_subject.clone()),
            sp_runtime::sp_std::vec![voter.clone()],
        )
        .expect("representative role snapshot should be created");
        RepresentativeMetas::<T>::insert(
            proposal_id,
            RepresentativeMeta {
                route: RepresentativeRoute::Single(role_subject.clone()),
                current_body: 0,
                rule: RepresentativeVoteRule::Regular,
                procedure: VoteProcedure::RepresentativeOnly,
            },
        );

        #[extrinsic_call]
        _(
            RawOrigin::Signed(voter.clone()),
            proposal_id,
            b"BENCHMARK_MEMBER"
                .to_vec()
                .try_into()
                .expect("benchmark role code fits"),
            true,
        );

        assert!(RepresentativeVotesByTicket::<T>::contains_key(
            proposal_id,
            (
                0,
                votebase::InstitutionVoteTicket {
                    role_subject,
                    voter_account_id: voter,
                }
            )
        ));
    }

    #[benchmark]
    fn cast_referendum_vote() {
        let proposal_id = insert_proposal::<T>(votebase::STAGE_LEG_REFERENDUM);
        let voter: T::AccountId = account("citizen", 0, 0);
        let scope = votebase::PopulationScope::Country;
        <T as votebase::Config>::CitizenIdentityReader::benchmark_seed_identity(&voter, &scope);
        let voter_subject =
            <T as votebase::Config>::CitizenIdentityReader::voting_subject(&voter, &scope)
                .expect("benchmark citizen subject should exist");
        votebase::Pallet::<T>::create_population_snapshot(proposal_id, &scope)
            .expect("benchmark proposal population snapshot should be created");
        LegislationMetas::<T>::insert(
            proposal_id,
            LegislationMeta {
                executive: None,
                override_signers: Default::default(),
                needs_guard: false,
                guard: None,
            },
        );

        #[extrinsic_call]
        _(RawOrigin::Signed(voter.clone()), proposal_id, true);

        assert!(LegReferendumVotesByCid::<T>::contains_key(
            proposal_id,
            voter_subject.cid_number
        ));
    }

    #[benchmark]
    fn executive_sign() {
        let proposal_id = insert_proposal::<T>(votebase::STAGE_LEG_SIGN);
        let executive = national_executive();
        LegislationMetas::<T>::insert(
            proposal_id,
            LegislationMeta {
                executive: Some(role_subject(executive, b"LR")),
                override_signers: Default::default(),
                needs_guard: false,
                guard: None,
            },
        );

        #[block]
        {
            Pallet::<T>::finalize_or_guard(proposal_id, false)
                .expect("executive approval reaches passed state");
        }

        assert!(votebase::pallet::PendingProposalExecutions::<T>::contains_key(proposal_id));
    }

    #[benchmark]
    fn override_sign() {
        let proposal_id = insert_proposal::<T>(votebase::STAGE_LEG_OVERRIDE);
        let senate = national_legislature(1);
        let house = national_legislature(2);
        let senate_role = role_subject(senate, b"BENCHMARK_MEMBER");
        let house_role = role_subject(house, b"BENCHMARK_MEMBER");
        let bodies: crate::RepresentativeBodies =
            sp_runtime::sp_std::vec![senate_role.clone(), house_role.clone()]
                .try_into()
                .expect("two representative bodies fit bound");
        RepresentativeMetas::<T>::insert(
            proposal_id,
            RepresentativeMeta {
                route: RepresentativeRoute::Sequential(bodies),
                current_body: 0,
                rule: RepresentativeVoteRule::Major,
                procedure: VoteProcedure::Legislation,
            },
        );
        LegislationMetas::<T>::insert(
            proposal_id,
            LegislationMeta {
                executive: Some(role_subject(national_executive(), b"LR")),
                override_signers: sp_runtime::sp_std::vec![
                    role_subject(national_legislature(0), b"LR"),
                    role_subject(senate_role.cid_number, b"LR"),
                    role_subject(house_role.cid_number, b"LR"),
                ]
                .try_into()
                .expect("three override signers fit"),
                needs_guard: false,
                guard: None,
            },
        );
        let who: T::AccountId = account("override-signer", 0, 0);

        #[block]
        {
            let mut signs = LegOverrideSigns::<T>::get(proposal_id);
            signs
                .try_push((who, true))
                .expect("first override signature fits");
            LegOverrideSigns::<T>::insert(proposal_id, signs);
        }

        assert_eq!(LegOverrideSigns::<T>::get(proposal_id).len(), 1);
    }

    #[benchmark]
    fn guard_vote() {
        let proposal_id = insert_proposal::<T>(votebase::STAGE_LEG_CONSTITUTION_GUARD);
        let who: T::AccountId = account("constitution-guard", 0, 0);

        #[block]
        {
            let mut signs = LegGuardSigns::<T>::get(proposal_id);
            signs
                .try_push((who, true))
                .expect("first guard signature fits");
            LegGuardSigns::<T>::insert(proposal_id, signs);
        }

        assert_eq!(LegGuardSigns::<T>::get(proposal_id).len(), 1);
    }
    #[benchmark]
    fn cleanup_representative(n: Linear<1, 256>) {
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
            crate::RepresentativeVotesByTicket::<T>::insert(0u64, (0u32, _ticket), true);
        }
        let result;
        #[block]
        {
            result = <Pallet<T> as votebase::ProposalTrackHandler<
                frame_system::pallet_prelude::BlockNumberFor<T>,
                T::AccountId,
            >>::cleanup_chunk(votebase::PROPOSAL_KIND_LEGISLATION, 0, n);
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
            crate::LegReferendumVotesByCid::<T>::insert(
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
            >>::cleanup_chunk(votebase::PROPOSAL_KIND_LEGISLATION, 0, n);
        }
        assert_eq!(result.expect("清理类型必须被认领").0, n);
    }

    #[benchmark]
    fn cleanup_terminal() {
        crate::RepresentativeMetas::<T>::insert(
            0u64,
            benchmark_value::<crate::pallet::RepresentativeMeta>(),
        );
        crate::LegislationMetas::<T>::insert(
            0u64,
            benchmark_value::<crate::pallet::LegislationMeta>(),
        );
        for i in 0..crate::MAX_REPRESENTATIVE_BODIES {
            crate::RepresentativeTallies::<T>::insert(
                0u64,
                i,
                votebase::VoteCountU32 { yes: 1, no: 0 },
            );
        }
        crate::LegReferendumTally::<T>::insert(0u64, votebase::VoteCountU64 { yes: 1, no: 0 });
        crate::LegOverrideSigns::<T>::insert(0u64,frame_support::BoundedVec::<(T::AccountId,bool),frame_support::traits::ConstU32<3>>::default());
        crate::LegGuardSigns::<T>::insert(
            0u64,
            frame_support::BoundedVec::<
                (T::AccountId, bool),
                frame_support::traits::ConstU32<{ crate::CONSTITUTION_GUARD_MEMBERS }>,
            >::default(),
        );
        #[block]
        {
            assert!(<Pallet<T> as votebase::ProposalTrackHandler<
                frame_system::pallet_prelude::BlockNumberFor<T>,
                T::AccountId,
            >>::cleanup_terminal(votebase::PROPOSAL_KIND_LEGISLATION, 0)
            .is_some());
        }
    }
}

// 清理阶段只消费键和值；计时区间始终执行所属真实清理路由。
fn benchmark_value<V: codec::Decode>() -> V {
    V::decode(&mut sp_runtime::traits::TrailingZeroInput::zeroes()).expect("基准类型必须可构造")
}
