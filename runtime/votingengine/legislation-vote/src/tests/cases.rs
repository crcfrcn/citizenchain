//! 立法投票单测:阈值纯函数 + 单院/两院/特别案投票机制。

use super::*;
use crate::{
    rules::{representative_decided, representative_final_passed},
    Error, RepresentativeVoteRule,
};
use frame_support::{assert_noop, assert_ok};
use votingengine::{
    STAGE_LEG_CONSTITUTION_GUARD, STAGE_LEG_OVERRIDE, STAGE_LEG_REFERENDUM,
    STAGE_LEG_REPRESENTATIVE, STAGE_LEG_SIGN, STATUS_EXECUTED, STATUS_REJECTED, STATUS_VOTING,
};

// ───────────────── 阈值纯函数(宪法第45/46条精确端点,五类立法表决)─────────────────

#[test]
fn representative_final_passed_thresholds() {
    // 常规案:>80% 参与 且 ≥60% 赞成(参与者基数)。total=10。
    assert!(representative_final_passed(
        RepresentativeVoteRule::Regular,
        10,
        6,
        3
    )); // casted=9>8, 6/9≥60%
    assert!(!representative_final_passed(
        RepresentativeVoteRule::Regular,
        10,
        5,
        3
    )); // 5/8=62.5%但 casted=8 不>8
    assert!(!representative_final_passed(
        RepresentativeVoteRule::Regular,
        10,
        5,
        4
    )); // 5/9=55%<60%
        // 重要案:>90% 参与 且 ≥70% 赞成。
    assert!(representative_final_passed(
        RepresentativeVoteRule::Major,
        10,
        7,
        3
    )); // casted=10>9, 7/10≥70%
    assert!(!representative_final_passed(
        RepresentativeVoteRule::Major,
        10,
        8,
        1
    )); // casted=9 不>9
        // 特别案内部:全员 且 ≥70% 赞成。
    assert!(representative_final_passed(
        RepresentativeVoteRule::Special,
        10,
        7,
        3
    )); // 全员10,7赞成
    assert!(!representative_final_passed(
        RepresentativeVoteRule::Special,
        10,
        6,
        4
    )); // 6<70%
}

#[test]
fn representative_decided_early() {
    // 全员已投 → 立即判定。
    assert_eq!(
        representative_decided(RepresentativeVoteRule::Regular, 10, 7, 3),
        Some(true)
    );
    // 结果已无法达到常规案赞成阈值 → 提前否决。
    assert_eq!(
        representative_decided(RepresentativeVoteRule::Regular, 10, 0, 5),
        Some(false)
    ); // 5>4
       // 未全员、未超限 → 未决。
    assert_eq!(
        representative_decided(RepresentativeVoteRule::Regular, 10, 3, 2),
        None
    );
}

#[test]
fn referendum_threshold() {
    // ≥70% 参与 且 ≥70% 赞成。eligible=100。
    assert!(primitives::constitution::referendum_passed(100, 56, 14)); // 参与70,赞成56/70=80%
    assert!(!primitives::constitution::referendum_passed(100, 50, 19)); // 参与69<70
    assert!(!primitives::constitution::referendum_passed(100, 48, 22)); // 参与70,赞成48/70≈68%<70%
}

fn regional_actor_cid(institution: &str) -> votingengine::CidNumber {
    primitives::cid::generator::generate_cid_number(
        primitives::cid::generator::GenerateCidNumberInput {
            public_key: "legislation-scope-test",
            p1: "0",
            province_code: "GD",
            province_name: "广东省",
            city_code: "002",
            city_name: "测试市",
            year: "2026",
            institution,
        },
    )
    .expect("regional actor CID should generate")
    .into_bytes()
    .try_into()
    .expect("regional actor CID should fit")
}

#[test]
fn special_population_scope_is_derived_only_from_actor_cid() {
    assert_eq!(
        Lib::population_scope_for_actor(*b"NRP\0", &actor_cid_number()).expect("NRP is national"),
        votingengine::PopulationScope::Country
    );

    let province_actor = regional_actor_cid("PRP");
    match Lib::population_scope_for_actor(*b"PRP\0", &province_actor).expect("PRP is provincial") {
        votingengine::PopulationScope::Province(province_code) => {
            assert_eq!(province_code.as_slice(), b"GD")
        }
        _ => panic!("PRP must derive province scope"),
    }

    let city_actor = regional_actor_cid("CSLF");
    match Lib::population_scope_for_actor(*b"CSLF", &city_actor).expect("CSLF is municipal") {
        votingengine::PopulationScope::City(province_code, city_code) => {
            assert_eq!(province_code.as_slice(), b"GD");
            assert_eq!(city_code.as_slice(), b"002");
        }
        _ => panic!("CSLF must derive city scope"),
    }
}

// ───────────────── 单院投票 ─────────────────

#[test]
fn representative_only_finishes_without_law_procedure() {
    new_test_ext().execute_with(|| {
        let route = crate::RepresentativeRoute::Single(house1());
        let vote_plan = test_vote_plan_with_owner(&route, None, b"personnel");
        let pid = <Lib as crate::LegislationVoteEngine<AccountId32>>::create_representative_vote(
            member(1),
            actor_cid_number(),
            vote_plan,
            route,
            RepresentativeVoteRule::Regular,
            votingengine::types::ProposalSubjectCidNumbers::new(),
            b"personnel",
            vec![1],
        )
        .expect("representative-only proposal");
        assert!(votingengine::ProposalPopulationSnapshots::<Test>::get(pid).is_none());
        // 发起人属于当前机构，创建时已经自动投下第一张赞成票。
        for i in 2u8..=7 {
            assert_ok!(cast(member(i), pid, true));
        }
        for i in 8u8..=10 {
            assert_ok!(cast(member(i), pid, false));
        }
        assert_eq!(status(pid), STATUS_EXECUTED);
        assert!(crate::pallet::LegislationMetas::<Test>::get(pid).is_none());
    });
}

#[test]
fn representative_only_rejects_special_rule() {
    new_test_ext().execute_with(|| {
        let route = crate::RepresentativeRoute::Single(house1());
        let vote_plan = test_vote_plan_with_owner(&route, None, b"personnel");
        let result = <Lib as crate::LegislationVoteEngine<AccountId32>>::create_representative_vote(
            member(1),
            actor_cid_number(),
            vote_plan,
            route,
            RepresentativeVoteRule::Special,
            votingengine::types::ProposalSubjectCidNumbers::new(),
            b"personnel",
            vec![1],
        );
        assert_eq!(result, Err(Error::<Test>::InvalidRepresentativeRule.into()));
    });
}

#[test]
fn single_house_regular_passes_then_mayor_signs() {
    new_test_ext().execute_with(|| {
        let pid = create(member(1), single_house(), RepresentativeVoteRule::Regular);
        // 10 名议员全投:7 赞成 3 反对 → 院通过 → 进入行政签署阶段(市行政区)。
        for i in 1u8..=7 {
            assert_ok!(cast(member(i), pid, true));
        }
        for i in 8u8..=10 {
            assert_ok!(cast(member(i), pid, false));
        }
        assert_eq!(stage(pid), STAGE_LEG_SIGN);
        assert_eq!(status(pid), STATUS_VOTING);
        // 市长(行政机构法定代表人)签署 → 生效。
        assert_ok!(exec_sign(exec_rep(), pid, true));
        assert_eq!(status(pid), STATUS_EXECUTED);
    });
}

#[test]
fn single_house_mayor_veto_rejects_without_rescue() {
    new_test_ext().execute_with(|| {
        let pid = create(member(1), single_house(), RepresentativeVoteRule::Regular);
        for i in 1u8..=7 {
            assert_ok!(cast(member(i), pid, true));
        }
        for i in 8u8..=10 {
            assert_ok!(cast(member(i), pid, false));
        }
        assert_eq!(stage(pid), STAGE_LEG_SIGN);
        // 市长否决 → 市行政区无救济 → 否决。
        assert_ok!(exec_sign(exec_rep(), pid, false));
        assert_eq!(status(pid), STATUS_REJECTED);
    });
}

#[test]
fn single_house_sign_timeout_passes() {
    new_test_ext().execute_with(|| {
        let pid = create(member(1), single_house(), RepresentativeVoteRule::Regular);
        for i in 1u8..=7 {
            assert_ok!(cast(member(i), pid, true));
        }
        for i in 8u8..=10 {
            assert_ok!(cast(member(i), pid, false));
        }
        assert_eq!(stage(pid), STAGE_LEG_SIGN);
        // 市行政区:市长 30 天未表态 → 超时视为通过。
        run_to_expiry(pid);
        assert_eq!(status(pid), STATUS_EXECUTED);
    });
}

#[test]
fn executive_sign_rejected_for_non_representative() {
    new_test_ext().execute_with(|| {
        let pid = create(member(1), single_house(), RepresentativeVoteRule::Regular);
        for i in 1u8..=10 {
            assert_ok!(cast(member(i), pid, i <= 7));
        }
        assert_eq!(stage(pid), STAGE_LEG_SIGN);
        // 非法定代表人签署被拒。
        assert!(exec_sign(member(2), pid, true).is_err());
        assert_eq!(status(pid), STATUS_VOTING);
    });
}

#[test]
fn single_house_regular_rejected_when_result_cannot_pass() {
    new_test_ext().execute_with(|| {
        let pid = create(member(1), single_house(), RepresentativeVoteRule::Regular);
        // 剩余赞成票已不足以达到常规案阈值 → 提前否决。
        for i in 1u8..=5 {
            assert_ok!(cast(member(i), pid, false));
        }
        assert_eq!(status(pid), STATUS_REJECTED);
    });
}

#[test]
fn double_vote_rejected() {
    new_test_ext().execute_with(|| {
        let pid = create(member(1), single_house(), RepresentativeVoteRule::Regular);
        assert_ok!(cast(member(1), pid, true));
        assert_noop!(
            Lib::do_cast_representative_vote(
                member(1),
                pid,
                REPRESENTATIVE_ROLE
                    .to_vec()
                    .try_into()
                    .expect("test role fits"),
                true,
            ),
            votingengine::Error::<Test>::AlreadyVoted
        );
    });
}

#[test]
fn non_member_cannot_vote() {
    new_test_ext().execute_with(|| {
        let pid = create(member(1), single_house(), RepresentativeVoteRule::Regular);
        // member(15) 属 house2,不在 house1 快照内。
        assert_noop!(
            Lib::do_cast_representative_vote(
                member(15),
                pid,
                REPRESENTATIVE_ROLE
                    .to_vec()
                    .try_into()
                    .expect("test role fits"),
                true,
            ),
            votingengine::Error::<Test>::NoPermission
        );
    });
}

#[test]
fn create_requires_actor_cid_admin_even_when_vote_route_is_valid() {
    new_test_ext().execute_with(|| {
        let route = crate::RepresentativeRoute::Single(house1());
        let meta = legislation_meta(&route, RepresentativeVoteRule::Regular, false);
        let vote_plan = test_vote_plan(&route, meta.as_ref());
        assert_eq!(
            Lib::do_create_representative_proposal(
                member(50),
                actor_cid_number(),
                vote_plan,
                route,
                RepresentativeVoteRule::Regular,
                crate::VoteProcedure::Legislation,
                votingengine::types::ProposalSubjectCidNumbers::new(),
                meta,
            ),
            Err(votingengine::Error::<Test>::NoPermission.into())
        );
    });
}

// ───────────────── 两院顺序 + 签署 + 三人会签 ─────────────────

/// 两院全过后推进至行政签署阶段(辅助):返回处于 STAGE_LEG_SIGN 的提案。
fn two_houses_passed_to_sign() -> u64 {
    let pid = create(member(1), two_houses(), RepresentativeVoteRule::Major);
    for i in 1u8..=8 {
        assert_ok!(cast(member(i), pid, true));
    }
    for i in 9u8..=10 {
        assert_ok!(cast(member(i), pid, false));
    }
    assert_eq!(stage(pid), STAGE_LEG_REPRESENTATIVE);
    assert_eq!(
        RepresentativeMetas::<Test>::get(pid).unwrap().current_body,
        1
    );
    for i in 11u8..=18 {
        assert_ok!(cast(member(i), pid, true));
    }
    for i in 19u8..=20 {
        assert_ok!(cast(member(i), pid, false));
    }
    // 两院通过(重要案无公投)→ 行政签署阶段(省行政区/国家)。
    assert_eq!(stage(pid), STAGE_LEG_SIGN);
    assert_eq!(status(pid), STATUS_VOTING);
    pid
}

#[test]
fn two_houses_pass_then_governor_signs() {
    new_test_ext().execute_with(|| {
        let pid = two_houses_passed_to_sign();
        assert_ok!(exec_sign(exec_rep(), pid, true));
        assert_eq!(status(pid), STATUS_EXECUTED);
    });
}

#[test]
fn same_account_id_can_vote_once_in_each_representative_body() {
    new_test_ext().execute_with(|| {
        let pid = create(
            member(1),
            overlapping_bodies(),
            RepresentativeVoteRule::Major,
        );
        for i in 1u8..=10 {
            assert_ok!(cast(member(i), pid, i <= 8));
        }
        assert_eq!(
            RepresentativeMetas::<Test>::get(pid)
                .expect("representative meta")
                .current_body,
            1
        );
        // 同一账户在第二个机构具有独立席位，不受第一机构去重记录影响。
        assert_ok!(cast(member(1), pid, true));
        assert!(
            crate::pallet::RepresentativeVotesByTicket::<Test>::contains_key(
                pid,
                (0, representative_ticket(house1_cid(), member(1)))
            )
        );
        assert!(
            crate::pallet::RepresentativeVotesByTicket::<Test>::contains_key(
                pid,
                (1, representative_ticket(house3_cid(), member(1)))
            )
        );
    });
}

#[test]
fn two_houses_exec_veto_then_three_sign_passes() {
    new_test_ext().execute_with(|| {
        let pid = two_houses_passed_to_sign();
        // 省长/总统否决 → 退回三人会签。
        assert_ok!(exec_sign(exec_rep(), pid, false));
        assert_eq!(stage(pid), STAGE_LEG_OVERRIDE);
        // 三人:院长(leg_rep)+ 众议长(member 1)+ 参议长(member 11) 全签 → 生效。
        assert_ok!(override_sign(leg_rep(), pid, true));
        assert_ok!(override_sign(member(1), pid, true));
        assert_eq!(status(pid), STATUS_VOTING);
        assert_ok!(override_sign(member(11), pid, true));
        assert_eq!(status(pid), STATUS_EXECUTED);
    });
}

#[test]
fn two_houses_override_one_veto_rejects() {
    new_test_ext().execute_with(|| {
        let pid = two_houses_passed_to_sign();
        assert_ok!(exec_sign(exec_rep(), pid, false));
        assert_eq!(stage(pid), STAGE_LEG_OVERRIDE);
        assert_ok!(override_sign(leg_rep(), pid, true));
        // 任一否决即否决。
        assert_ok!(override_sign(member(1), pid, false));
        assert_eq!(status(pid), STATUS_REJECTED);
    });
}

#[test]
fn two_houses_sign_timeout_goes_to_override() {
    new_test_ext().execute_with(|| {
        let pid = two_houses_passed_to_sign();
        // 省行政区/国家:行政首长 30 天未表态 → 退回三人会签。
        run_to_expiry(pid);
        assert_eq!(stage(pid), STAGE_LEG_OVERRIDE);
        assert_eq!(status(pid), STATUS_VOTING);
    });
}

#[test]
fn two_houses_override_timeout_rejects() {
    new_test_ext().execute_with(|| {
        let pid = two_houses_passed_to_sign();
        assert_ok!(exec_sign(exec_rep(), pid, false));
        assert_eq!(stage(pid), STAGE_LEG_OVERRIDE);
        // 三人会签 30 天未完成 → 否决。
        run_to_expiry(pid);
        assert_eq!(status(pid), STATUS_REJECTED);
    });
}

#[test]
fn override_sign_rejected_for_non_signer() {
    new_test_ext().execute_with(|| {
        let pid = two_houses_passed_to_sign();
        assert_ok!(exec_sign(exec_rep(), pid, false));
        // member(5) 不是院长/参议长/众议长。
        assert!(override_sign(member(5), pid, true).is_err());
    });
}

// ───────────────── 特别案 → 强制公投 ─────────────────

#[test]
fn special_case_advances_to_referendum_then_passes() {
    new_test_ext().execute_with(|| {
        // 特别案创建时由投票引擎内联生成全国人口快照，分母 100。
        let pid = create(member(1), single_house(), RepresentativeVoteRule::Special);
        assert_eq!(
            votingengine::ProposalPopulationSnapshots::<Test>::get(pid)
                .map(|snapshot| snapshot.population_data.eligible_total),
            Some(100)
        );
        // 全员 10:8 赞成 2 反对 → 内部段通过(≥70%)→ 推进至公投阶段。
        for i in 1u8..=8 {
            assert_ok!(cast(member(i), pid, true));
        }
        for i in 9u8..=10 {
            assert_ok!(cast(member(i), pid, false));
        }
        assert_eq!(stage(pid), STAGE_LEG_REFERENDUM);
        assert_eq!(status(pid), STATUS_VOTING); // 公投尚未结算

        // 公投:参与 70(56 赞成 14 反对)→ 达 ≥70% 参与 + ≥70% 赞成。
        for i in 0u64..56 {
            cast_referendum(pid, i, true);
        }
        for i in 56u64..70 {
            cast_referendum(pid, i, false);
        }
        // 期满结算(本入口不提前判定)。
        let p = votingengine::pallet::Proposals::<Test>::get(pid).unwrap();
        System::set_block_number(p.end + 1);
        finalize_referendum(pid);
        assert_eq!(status(pid), STATUS_EXECUTED);
    });
}

#[test]
fn legislation_referendum_rejects_votes_beyond_population_snapshot_denominator() {
    new_test_ext().execute_with(|| {
        let pid = create(member(1), single_house(), RepresentativeVoteRule::Special);
        for i in 1u8..=8 {
            assert_ok!(cast(member(i), pid, true));
        }
        for i in 9u8..=10 {
            assert_ok!(cast(member(i), pid, false));
        }
        crate::pallet::LegReferendumTally::<Test>::insert(
            pid,
            votingengine::VoteCountU64 { yes: 70, no: 30 },
        );

        assert_noop!(
            Lib::do_cast_referendum_vote(member(100), pid, true),
            Error::<Test>::ReferendumSnapshotExhausted
        );
        assert!(
            !crate::pallet::LegReferendumVotesByCid::<Test>::contains_key(
                pid,
                test_citizen_subject(&member(100)).cid_number,
            )
        );
    });
}

#[test]
fn legislation_referendum_deduplicates_permanent_cid_after_account_id_replacement() {
    new_test_ext().execute_with(|| {
        let pid = create(member(1), single_house(), RepresentativeVoteRule::Special);
        for i in 1u8..=8 {
            assert_ok!(cast(member(i), pid, true));
        }
        for i in 9u8..=10 {
            assert_ok!(cast(member(i), pid, false));
        }
        assert_eq!(stage(pid), STAGE_LEG_REFERENDUM);

        assert_ok!(Lib::do_cast_referendum_vote(member(100), pid, true));
        assert_noop!(
            Lib::do_cast_referendum_vote(member(201), pid, false),
            votingengine::Error::<Test>::AlreadyVoted
        );

        let subject = test_citizen_subject(&member(100));
        let ticket = crate::pallet::LegReferendumVotesByCid::<Test>::get(pid, &subject.cid_number)
            .expect("complete citizen referendum ticket should exist");
        assert_eq!(ticket.voter_subject, subject);
        assert!(ticket.approve);
        assert_eq!(crate::pallet::LegReferendumTally::<Test>::get(pid).yes, 1);
        assert_eq!(crate::pallet::LegReferendumTally::<Test>::get(pid).no, 0);
    });
}

/// 公投投一票：资格由 CitizenIdentityReader 返回完整主体，票据按 CID 去重。
fn cast_referendum(pid: u64, seed: u64, approve: bool) {
    frame_support::storage::with_transaction(
        || -> frame_support::storage::TransactionOutcome<sp_runtime::DispatchResult> {
            let r = Lib::do_cast_referendum_vote(member((seed % 200) as u8), pid, approve);
            match r {
                Ok(()) => frame_support::storage::TransactionOutcome::Commit(Ok(())),
                Err(e) => frame_support::storage::TransactionOutcome::Rollback(Err(e)),
            }
        },
    )
    .expect("referendum vote ok");
}

fn finalize_referendum(pid: u64) {
    let proposal = votingengine::pallet::Proposals::<Test>::get(pid).unwrap();
    frame_support::storage::with_transaction(
        || -> frame_support::storage::TransactionOutcome<sp_runtime::DispatchResult> {
            match Lib::do_finalize_referendum_timeout(&proposal, pid) {
                Ok(()) => frame_support::storage::TransactionOutcome::Commit(Ok(())),
                Err(e) => frame_support::storage::TransactionOutcome::Rollback(Err(e)),
            }
        },
    )
    .expect("finalize ok");
    process_current_block();
}

// ───────────────── 护宪大法官终审(仅修宪,宪法第21条)─────────────────

/// 修宪重要案(单院)推进到护宪大法官终审阶段(辅助)。
fn constitution_amend_to_guard() -> u64 {
    let pid = create_guard(member(1), single_house(), RepresentativeVoteRule::Major);
    // 重要案:>90% 参与 + ≥70% 赞成 → 全员 10 投,8 赞成 2 反对 → 院通过 → 行政签署。
    for i in 1u8..=8 {
        assert_ok!(cast(member(i), pid, true));
    }
    for i in 9u8..=10 {
        assert_ok!(cast(member(i), pid, false));
    }
    assert_eq!(stage(pid), STAGE_LEG_SIGN);
    // 总统签署 → 修宪转护宪大法官终审(而非直接生效)。
    assert_ok!(exec_sign(exec_rep(), pid, true));
    assert_eq!(stage(pid), STAGE_LEG_CONSTITUTION_GUARD);
    assert_eq!(status(pid), STATUS_VOTING);
    pid
}

#[test]
fn constitution_amend_passes_on_four_guard_approvals() {
    new_test_ext().execute_with(|| {
        let pid = constitution_amend_to_guard();
        // 7 名护宪大法官中 4 名及以上赞成 → 生效。
        assert_ok!(guard_vote(member(101), pid, true));
        assert_ok!(guard_vote(member(102), pid, true));
        assert_ok!(guard_vote(member(103), pid, true));
        assert_eq!(status(pid), STATUS_VOTING);
        assert_ok!(guard_vote(member(104), pid, true));
        assert_eq!(status(pid), STATUS_EXECUTED);
    });
}

#[test]
fn constitution_amend_rejected_on_four_guard_rejections() {
    new_test_ext().execute_with(|| {
        let pid = constitution_amend_to_guard();
        assert_ok!(guard_vote(member(101), pid, false));
        assert_ok!(guard_vote(member(102), pid, false));
        assert_ok!(guard_vote(member(103), pid, false));
        assert_eq!(status(pid), STATUS_VOTING);
        // 7 人制下 4 名反对 → 已不可能达到 4 名赞成,否决。
        assert_ok!(guard_vote(member(104), pid, false));
        assert_eq!(status(pid), STATUS_REJECTED);
    });
}

#[test]
fn constitution_amend_stays_voting_with_three_guard_approvals() {
    new_test_ext().execute_with(|| {
        let pid = constitution_amend_to_guard();
        assert_ok!(guard_vote(member(101), pid, true));
        assert_ok!(guard_vote(member(102), pid, true));
        assert_ok!(guard_vote(member(103), pid, true));
        assert_eq!(status(pid), STATUS_VOTING);
    });
}

#[test]
fn invalid_guard_member_count_rejected() {
    new_test_ext().execute_with(|| {
        let cases: &[&[u8]] = &[
            &[],
            &[101, 102, 103, 104, 105, 106],
            &[101, 102, 103, 104, 105, 106, 107, 108],
        ];
        for ids in cases {
            set_guard_member_ids(ids);
            assert_noop!(
                try_create_guard(member(1), single_house(), RepresentativeVoteRule::Major),
                Error::<Test>::InvalidGuardMembersLen
            );
        }
    });
}

#[test]
fn duplicate_guard_member_list_rejected() {
    new_test_ext().execute_with(|| {
        set_guard_member_ids(&[101, 102, 103, 104, 105, 106, 106]);
        assert_noop!(
            try_create_guard(member(1), single_house(), RepresentativeVoteRule::Major),
            Error::<Test>::InvalidGuardMembersLen
        );
    });
}

#[test]
fn constitution_amend_guard_timeout_rejects() {
    new_test_ext().execute_with(|| {
        let pid = constitution_amend_to_guard();
        // 护宪大法官 30 天未达 4 名及以上赞成 → 超时否决。
        run_to_expiry(pid);
        assert_eq!(status(pid), STATUS_REJECTED);
    });
}

#[test]
fn non_guard_cannot_guard_vote() {
    new_test_ext().execute_with(|| {
        let pid = constitution_amend_to_guard();
        // member(5) 不是护宪大法官。
        assert_noop!(
            guard_vote(member(5), pid, true),
            Error::<Test>::NotConstitutionGuard
        );
        assert_eq!(status(pid), STATUS_VOTING);
    });
}

#[test]
fn guard_member_cannot_vote_twice() {
    new_test_ext().execute_with(|| {
        let pid = constitution_amend_to_guard();
        assert_ok!(guard_vote(member(101), pid, true));
        assert_noop!(
            guard_vote(member(101), pid, false),
            Error::<Test>::AlreadySigned
        );
        assert_eq!(status(pid), STATUS_VOTING);
    });
}

#[test]
fn non_amend_skips_guard() {
    new_test_ext().execute_with(|| {
        // 非修宪(needs_guard=false)单院重要案:院通过→签署→直接生效,不进护宪阶段。
        let pid = create(member(1), single_house(), RepresentativeVoteRule::Major);
        for i in 1u8..=8 {
            assert_ok!(cast(member(i), pid, true));
        }
        for i in 9u8..=10 {
            assert_ok!(cast(member(i), pid, false));
        }
        assert_eq!(stage(pid), STAGE_LEG_SIGN);
        assert_ok!(exec_sign(exec_rep(), pid, true));
        assert_eq!(status(pid), STATUS_EXECUTED);
    });
}
