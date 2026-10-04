//! 立法院模块第1步单测(engine = `()`)。
//!
//! 覆盖:write_law_version 写入/升版/废止、on_initialize 按时间戳生效调度、
//! 提案校验(非议员拒绝 / 空标题空条文 / 宪法表决类型 / 不可修改条款 / 宪法不可废)、
//! 合法提案到引擎返回 NotConfigured(确认对接点)。

use super::*;
use crate::pallet::Error;
use frame_support::traits::Time;
use frame_support::{assert_noop, assert_ok, traits::Hooks};

// ───────────────── 执行器写入 / 状态机 ─────────────────

#[test]
fn enact_writes_law_and_schedules_future_activation() {
    new_test_ext().execute_with(|| {
        let mut summary = enact_summary(
            Tier::Municipal,
            1001,
            VoteType::Major,
            b"\xe5\xb8\x82\xe9\x95\xbf\xe9\x80\x89\xe4\xb8\xbe\xe6\xb3\x95",
        );
        summary.effective_at = 2_000; // 未来生效时间戳
        let arts = chapters_of(vec![article(1, b"a"), article(2, b"b")]);

        assert_ok!(Lib::write_law_version(7, summary, arts, Timestamp::now()));

        let law = Laws::<Test>::get(0).expect("law created");
        assert_eq!(law.effective_version, None);
        assert_eq!(law.latest_version, 1);
        assert_eq!(law.pending_version, Some(1));
        assert_eq!(law.status, LawStatus::Pending); // 未到生效时间
        assert!(LawVersions::<Test>::get(0, 1).is_some());
        assert_eq!(NextLawId::<Test>::get(), 1);
        assert_eq!(Lib::list_laws(Tier::Municipal, 1001), vec![0]);
        assert_eq!(
            PendingActivations::<Test>::get().into_inner(),
            vec![(0u64, 1u32)]
        );

        // 未到生效时间时保持待生效。
        Lib::on_initialize(2);
        assert_eq!(Laws::<Test>::get(0).unwrap().status, LawStatus::Pending);
        assert_eq!(
            PendingActivations::<Test>::get().into_inner(),
            vec![(0u64, 1u32)]
        );

        // 到生效时间后自动翻 Effective。
        Timestamp::set_timestamp(2_000);
        Lib::on_initialize(3);
        let law = Laws::<Test>::get(0).unwrap();
        assert_eq!(law.status, LawStatus::Effective);
        assert_eq!(law.effective_version, Some(1));
        assert_eq!(law.pending_version, None);
        assert!(PendingActivations::<Test>::get().is_empty());
    });
}

#[test]
fn enact_with_past_effective_is_immediately_effective() {
    new_test_ext().execute_with(|| {
        let summary = enact_summary(Tier::National, 0, VoteType::Regular, b"law");
        // effective_at = 0 <= 当前链上时间戳 → 立即生效
        assert_ok!(Lib::write_law_version(
            1,
            summary,
            chapters_of(vec![article(1, b"a")]),
            Timestamp::now()
        ));
        let law = Laws::<Test>::get(0).unwrap();
        assert_eq!(law.status, LawStatus::Effective);
        assert_eq!(law.effective_version, Some(1));
        assert_eq!(law.pending_version, None);
    });
}

#[test]
fn amend_bumps_version_and_resets_pending() {
    new_test_ext().execute_with(|| {
        let s0 = enact_summary(Tier::National, 0, VoteType::Regular, b"law");
        assert_ok!(Lib::write_law_version(
            1,
            s0,
            chapters_of(vec![article(1, b"a")]),
            Timestamp::now()
        ));

        let mut s1 = enact_summary(Tier::National, 0, VoteType::Regular, b"amended-law");
        s1.action = LawAction::Amend;
        s1.law_id = 0;
        s1.effective_at = 2_000;
        assert_ok!(Lib::write_law_version(
            2,
            s1,
            chapters_of(vec![article(1, b"a2")]),
            Timestamp::now()
        ));

        let law = Laws::<Test>::get(0).unwrap();
        assert_eq!(law.effective_version, Some(1));
        assert_eq!(law.latest_version, 2);
        assert_eq!(law.pending_version, Some(2));
        assert_eq!(law.status, LawStatus::Pending);
        assert!(LawVersions::<Test>::get(0, 2).is_some());
        assert!(LawVersions::<Test>::get(0, 1).is_some()); // 历史保留
    });
}

#[test]
fn repeal_sets_status_repealed() {
    new_test_ext().execute_with(|| {
        let s0 = enact_summary(Tier::National, 0, VoteType::Regular, b"law");
        assert_ok!(Lib::write_law_version(
            1,
            s0,
            chapters_of(vec![article(1, b"a")]),
            Timestamp::now()
        ));

        let mut sr = enact_summary(Tier::National, 0, VoteType::Regular, b"");
        sr.action = LawAction::Repeal;
        sr.law_id = 0;
        assert_ok!(Lib::write_law_version(2, sr, chapters_of(vec![]), 1));

        assert_eq!(Laws::<Test>::get(0).unwrap().status, LawStatus::Repealed);
    });
}

// ───────────────── 提案入口校验 ─────────────────

fn one_chapter() -> crate::pallet::ChaptersOf<Test> {
    chapters_of(vec![article(1, b"content")])
}

#[test]
fn propose_enact_by_legislator_reaches_engine_notconfigured() {
    new_test_ext().execute_with(|| {
        // 合法发起人 + 合法输入 → 通过全部校验 → 调引擎 () → NotConfigured → VoteEngineCreateFailed
        assert_noop!(
            Lib::propose_enact_law(
                RuntimeOrigin::signed(legislator()),
                Tier::Municipal,
                1001,
                municipal_houses(),
                municipal_actor_cid_number(),
                proposer_role_code(),
                municipal_executive_cid_number(),
                None,
                VoteType::Regular,
                title(b"law"),
                None,
                one_chapter(),
                100,
            ),
            Error::<Test>::VoteEngineCreateFailed
        );
    });
}

#[test]
fn municipal_education_route_reaches_engine_notconfigured() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            Lib::propose_enact_law(
                RuntimeOrigin::signed(legislator()),
                Tier::Municipal,
                1001,
                municipal_houses(),
                municipal_education_actor_cid_number(),
                proposer_role_code(),
                municipal_executive_cid_number(),
                None,
                VoteType::RegularEducation,
                title(b"education law"),
                None,
                one_chapter(),
                100,
            ),
            Error::<Test>::VoteEngineCreateFailed
        );
    });
}

#[test]
fn routing_rejects_cid_code_mismatch() {
    new_test_ext().execute_with(|| {
        // houses 需要市立法会 CID，却夹带市政府 CID；机构码必须从 CID 自身解析。
        let forged_houses = BoundedVec::try_from(vec![municipal_executive_cid_number()])
            .expect("forged houses within bound");
        assert_noop!(
            Lib::propose_enact_law(
                RuntimeOrigin::signed(legislator()),
                Tier::Municipal,
                1001,
                forged_houses,
                municipal_actor_cid_number(),
                proposer_role_code(),
                municipal_executive_cid_number(),
                None,
                VoteType::Regular,
                title(b"forged route"),
                None,
                one_chapter(),
                100,
            ),
            Error::<Test>::RoutingMismatch
        );
    });
}

#[test]
fn propose_enact_by_non_legislator_rejected() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            Lib::propose_enact_law(
                RuntimeOrigin::signed(outsider()),
                Tier::Municipal,
                1001,
                municipal_houses(),
                municipal_actor_cid_number(),
                proposer_role_code(),
                municipal_executive_cid_number(),
                None,
                VoteType::Regular,
                title(b"law"),
                None,
                one_chapter(),
                100,
            ),
            Error::<Test>::NotLegislator
        );
    });
}

#[test]
fn propose_enact_constitution_rejected() {
    // 立法入口不得新立第二部宪法(ADR-027 §6.1):tier=Constitution 直接拒。
    new_test_ext().execute_with(|| {
        assert_noop!(
            Lib::propose_enact_law(
                RuntimeOrigin::signed(legislator()),
                Tier::Constitution,
                0,
                houses(),
                actor_cid_number(),
                proposer_role_code(),
                executive_cid_number(),
                None,
                VoteType::Special,
                title(b"second constitution"),
                None,
                one_chapter(),
                100,
            ),
            Error::<Test>::CannotEnactConstitution
        );
    });
}

#[test]
fn enum_discriminants_match_node_guard() {
    // 钉死 SCALE 变体索引,防节点守卫(core/constitution.rs)硬编码常量漂移。
    use codec::Encode;
    assert_eq!(Tier::Constitution.encode(), vec![0u8]);
    assert_eq!(LawStatus::Pending.encode(), vec![0u8]);
    assert_eq!(LawStatus::Effective.encode(), vec![1u8]);
    assert_eq!(LawStatus::Repealed.encode(), vec![2u8]);
    // 特别案业务 wire 值 = 4：节点守卫据此判定核心章档位背书（第十九条）。
    assert_eq!(VoteType::Special.encode(), vec![4u8]);
    assert_eq!(
        VoteType::Special.representative_rule(),
        legislation_vote::RepresentativeVoteRule::Special
    );
}

#[test]
fn propose_enact_empty_title_and_articles_rejected() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            Lib::propose_enact_law(
                RuntimeOrigin::signed(legislator()),
                Tier::Municipal,
                1001,
                municipal_houses(),
                municipal_actor_cid_number(),
                proposer_role_code(),
                municipal_executive_cid_number(),
                None,
                VoteType::Regular,
                title(b""),
                None,
                one_chapter(),
                100,
            ),
            Error::<Test>::EmptyTitle
        );
        assert_noop!(
            Lib::propose_enact_law(
                RuntimeOrigin::signed(legislator()),
                Tier::Municipal,
                1001,
                municipal_houses(),
                municipal_actor_cid_number(),
                proposer_role_code(),
                municipal_executive_cid_number(),
                None,
                VoteType::Regular,
                title(b"law"),
                None,
                crate::pallet::ChaptersOf::<Test>::default(),
                100,
            ),
            Error::<Test>::EmptyChapters
        );
    });
}

// 预置一部宪法(article 1 与 17 固定),供修法/废法校验测试。
fn seed_constitution() {
    let law_id = 0u64;
    let version = 1u32;
    let chapters = chapters_of(vec![article(1, b"yuan-1"), article(17, b"yuan-17")]);
    LawVersions::<Test>::insert(
        law_id,
        version,
        LawVersion::<Test> {
            law_id,
            version,
            title: title(b"constitution"),
            title_en: None,
            chapters,
            content_hash: [0u8; 32],
            vote_type: VoteType::Special,
            proposal_id: 1,
            published_at: 1_000,
            effective_at: 1_000,
        },
    );
    Laws::<Test>::insert(
        law_id,
        Law {
            law_id,
            tier: Tier::Constitution,
            scope_code: 0,
            houses: houses(),
            effective_version: Some(version),
            latest_version: version,
            pending_version: None,
            status: LawStatus::Effective,
        },
    );
    let _ = LawsByScope::<Test>::try_mutate(Tier::Constitution, 0, |v| v.try_push(law_id));
    NextLawId::<Test>::put(1);
}

#[test]
fn amend_constitution_immutable_article_rejected() {
    new_test_ext().execute_with(|| {
        seed_constitution();
        // 改第 1 条(不可修改条款)→ 拒绝
        assert_noop!(
            Lib::propose_amend_law(
                RuntimeOrigin::signed(legislator()),
                0,
                actor_cid_number(),
                proposer_role_code(),
                executive_cid_number(),
                legislature_cid_number(),
                VoteType::Special,
                title(b"amended-constitution"),
                None,
                chapters_of(vec![article(1, b"CHANGED"), article(17, b"yuan-17")]),
                200,
            ),
            Error::<Test>::ImmutableArticleViolation
        );
    });
}

// 预置一部两章宪法(核心章第一章 + 一般章第二章),供第十九条章→档位强制测试。
// 核心章:第 1 条(不可修改)+ 第 5、20 条(核心非禁改);一般章:第 60、61 条。
fn seed_constitution_tiered() {
    let law_id = 0u64;
    let version = 1u32;
    let chapters = chapters_core_general(
        vec![
            article(1, b"core-1"),
            article(5, b"core-5"),
            article(20, b"core-20"),
        ],
        vec![article(60, b"gen-60"), article(61, b"gen-61")],
    );
    LawVersions::<Test>::insert(
        law_id,
        version,
        LawVersion::<Test> {
            law_id,
            version,
            title: title(b"constitution"),
            title_en: None,
            chapters,
            content_hash: [0u8; 32],
            vote_type: VoteType::Special,
            proposal_id: 1,
            published_at: 1_000,
            effective_at: 1_000,
        },
    );
    Laws::<Test>::insert(
        law_id,
        Law {
            law_id,
            tier: Tier::Constitution,
            scope_code: 0,
            houses: houses(),
            effective_version: Some(version),
            latest_version: version,
            pending_version: None,
            status: LawStatus::Effective,
        },
    );
    let _ = LawsByScope::<Test>::try_mutate(Tier::Constitution, 0, |v| v.try_push(law_id));
    NextLawId::<Test>::put(1);
}

#[test]
fn amend_core_chapter_with_major_rejected() {
    // 第十九条:改第一章核心条款必须走特别案。用重要案改核心章第 5 条 → 拒。
    new_test_ext().execute_with(|| {
        seed_constitution_tiered();
        assert_noop!(
            Lib::propose_amend_law(
                RuntimeOrigin::signed(legislator()),
                0,
                actor_cid_number(),
                proposer_role_code(),
                executive_cid_number(),
                legislature_cid_number(),
                VoteType::Major,
                title(b"amended-constitution"),
                None,
                chapters_core_general(
                    vec![
                        article(1, b"core-1"),
                        article(5, b"CHANGED"),
                        article(20, b"core-20"),
                    ],
                    vec![article(60, b"gen-60"), article(61, b"gen-61")],
                ),
                200,
            ),
            Error::<Test>::CoreClauseRequiresSpecial
        );
    });
}

#[test]
fn amend_core_chapter_with_special_passes_gate() {
    // 用特别案改核心章第 5 条 → 过章→档位闸 → 到引擎 () → VoteEngineCreateFailed。
    new_test_ext().execute_with(|| {
        seed_constitution_tiered();
        assert_noop!(
            Lib::propose_amend_law(
                RuntimeOrigin::signed(legislator()),
                0,
                actor_cid_number(),
                proposer_role_code(),
                executive_cid_number(),
                legislature_cid_number(),
                VoteType::Special,
                title(b"amended-constitution"),
                None,
                chapters_core_general(
                    vec![
                        article(1, b"core-1"),
                        article(5, b"CHANGED"),
                        article(20, b"core-20"),
                    ],
                    vec![article(60, b"gen-60"), article(61, b"gen-61")],
                ),
                200,
            ),
            Error::<Test>::VoteEngineCreateFailed
        );
    });
}

#[test]
fn amend_general_chapter_with_special_rejected() {
    // 第十九条:改第一章以外的一般条款必须走重要案。用特别案改一般章第 60 条 → 拒。
    new_test_ext().execute_with(|| {
        seed_constitution_tiered();
        assert_noop!(
            Lib::propose_amend_law(
                RuntimeOrigin::signed(legislator()),
                0,
                actor_cid_number(),
                proposer_role_code(),
                executive_cid_number(),
                legislature_cid_number(),
                VoteType::Special,
                title(b"amended-constitution"),
                None,
                chapters_core_general(
                    vec![
                        article(1, b"core-1"),
                        article(5, b"core-5"),
                        article(20, b"core-20"),
                    ],
                    vec![article(60, b"CHANGED"), article(61, b"gen-61")],
                ),
                200,
            ),
            Error::<Test>::GeneralClauseRequiresMajor
        );
    });
}

#[test]
fn amend_general_chapter_with_major_passes_gate() {
    // 用重要案改一般章第 60 条 → 过章→档位闸 → 到引擎 () → VoteEngineCreateFailed。
    new_test_ext().execute_with(|| {
        seed_constitution_tiered();
        assert_noop!(
            Lib::propose_amend_law(
                RuntimeOrigin::signed(legislator()),
                0,
                actor_cid_number(),
                proposer_role_code(),
                executive_cid_number(),
                legislature_cid_number(),
                VoteType::Major,
                title(b"amended-constitution"),
                None,
                chapters_core_general(
                    vec![
                        article(1, b"core-1"),
                        article(5, b"core-5"),
                        article(20, b"core-20"),
                    ],
                    vec![article(60, b"CHANGED"), article(61, b"gen-61")],
                ),
                200,
            ),
            Error::<Test>::VoteEngineCreateFailed
        );
    });
}

#[test]
fn amend_constitution_no_change_rejected() {
    // 全文与当前生效版本完全一致 → 空提案,拒(EmptyAmendment)。
    new_test_ext().execute_with(|| {
        seed_constitution_tiered();
        assert_noop!(
            Lib::propose_amend_law(
                RuntimeOrigin::signed(legislator()),
                0,
                actor_cid_number(),
                proposer_role_code(),
                executive_cid_number(),
                legislature_cid_number(),
                VoteType::Special,
                title(b"amended-constitution"),
                None,
                chapters_core_general(
                    vec![
                        article(1, b"core-1"),
                        article(5, b"core-5"),
                        article(20, b"core-20"),
                    ],
                    vec![article(60, b"gen-60"), article(61, b"gen-61")],
                ),
                200,
            ),
            Error::<Test>::EmptyAmendment
        );
    });
}

#[test]
fn write_core_amend_without_referendum_proof_rejected() {
    // 提交层复校验:核心章改动 + 特别案,但引擎无公投结果(mock=()→None)→ 拒(ReferendumProofMissing)。
    new_test_ext().execute_with(|| {
        seed_constitution_tiered();
        let mut summary = enact_summary(
            Tier::Constitution,
            0,
            VoteType::Special,
            b"amended-constitution",
        );
        summary.action = LawAction::Amend;
        summary.law_id = 0;
        let new_chapters = chapters_core_general(
            vec![
                article(1, b"core-1"),
                article(5, b"CHANGED"),
                article(20, b"core-20"),
            ],
            vec![article(60, b"gen-60"), article(61, b"gen-61")],
        );
        assert_noop!(
            Lib::write_law_version(9, summary, new_chapters, Timestamp::now()),
            Error::<Test>::ReferendumProofMissing
        );
    });
}

#[test]
fn write_amend_without_guard_proof_rejected() {
    // 提交层:一般章修宪(重要案,免公投),但引擎无护宪终审结果(mock=()→None)→ 拒(GuardReviewProofMissing)。
    new_test_ext().execute_with(|| {
        seed_constitution_tiered();
        let mut summary = enact_summary(
            Tier::Constitution,
            0,
            VoteType::Major,
            b"amended-constitution",
        );
        summary.action = LawAction::Amend;
        summary.law_id = 0;
        let new_chapters = chapters_core_general(
            vec![
                article(1, b"core-1"),
                article(5, b"core-5"),
                article(20, b"core-20"),
            ],
            vec![article(60, b"CHANGED"), article(61, b"gen-61")],
        );
        assert_noop!(
            Lib::write_law_version(9, summary, new_chapters, Timestamp::now()),
            Error::<Test>::GuardReviewProofMissing
        );
    });
}

#[test]
fn rejects_amend_while_pending() {
    // 待生效(Pending)期间不得再次修订(P3:保证至多一个待生效版本)。
    new_test_ext().execute_with(|| {
        let mut summary = enact_summary(Tier::Municipal, 1001, VoteType::Major, b"law");
        summary.effective_at = 2_000; // 未来生效 → 写入后 status=Pending
        assert_ok!(Lib::write_law_version(
            7,
            summary,
            one_chapter(),
            Timestamp::now()
        ));
        assert_eq!(Laws::<Test>::get(0).unwrap().status, LawStatus::Pending);
        assert_noop!(
            Lib::propose_amend_law(
                RuntimeOrigin::signed(legislator()),
                0,
                actor_cid_number(),
                proposer_role_code(),
                executive_cid_number(),
                None,
                VoteType::Major,
                title(b"amended-law"),
                None,
                one_chapter(),
                200,
            ),
            Error::<Test>::AmendmentAlreadyPending
        );
    });
}

#[test]
fn write_rejects_enact_constitution_directly() {
    // 最终写入层也拒绝新立第二部宪法,不能只依赖 propose_enact_law 入口。
    new_test_ext().execute_with(|| {
        let summary = enact_summary(Tier::Constitution, 0, VoteType::Special, b"constitution");
        assert_noop!(
            Lib::write_law_version(9, summary, one_chapter(), 1),
            Error::<Test>::CannotEnactConstitution
        );
        assert_eq!(NextLawId::<Test>::get(), 0, "拒绝前不得消耗 law_id");
    });
}

#[test]
fn write_rejects_amend_constitution_immutable_article_directly() {
    // forged/异常回调直接写入 Amend 时,仍必须执行不可修改条款复校验。
    new_test_ext().execute_with(|| {
        seed_constitution();
        let mut summary = enact_summary(
            Tier::Constitution,
            0,
            VoteType::Special,
            b"amended-constitution",
        );
        summary.action = LawAction::Amend;
        summary.law_id = 0;
        assert_noop!(
            Lib::write_law_version(
                10,
                summary,
                chapters_of(vec![article(1, b"CHANGED"), article(17, b"yuan-17")]),
                1,
            ),
            Error::<Test>::ImmutableArticleViolation
        );
    });
}

#[test]
fn write_rejects_amend_while_pending_directly() {
    // Pending 单飞规则在写入层复查,防多个待生效版本互相覆盖。
    new_test_ext().execute_with(|| {
        let mut summary = enact_summary(Tier::Municipal, 1001, VoteType::Major, b"law");
        summary.effective_at = 2_000;
        assert_ok!(Lib::write_law_version(
            7,
            summary,
            one_chapter(),
            Timestamp::now()
        ));

        let mut amend = enact_summary(Tier::Municipal, 1001, VoteType::Major, b"amended-law");
        amend.action = LawAction::Amend;
        amend.law_id = 0;
        assert_noop!(
            Lib::write_law_version(8, amend, one_chapter(), 1),
            Error::<Test>::AmendmentAlreadyPending
        );
    });
}

#[test]
fn write_rejects_repeal_constitution_directly() {
    // 最终写入层也拒绝废止宪法,不能只依赖 propose_repeal_law 入口。
    new_test_ext().execute_with(|| {
        seed_constitution();
        let mut summary = enact_summary(Tier::Constitution, 0, VoteType::Special, b"");
        summary.action = LawAction::Repeal;
        summary.law_id = 0;
        assert_noop!(
            Lib::write_law_version(11, summary, ChaptersOf::<Test>::default(), 1),
            Error::<Test>::CannotRepealConstitution
        );
        assert_eq!(Laws::<Test>::get(0).unwrap().status, LawStatus::Effective);
    });
}

#[test]
fn amend_constitution_preserving_immutable_reaches_engine() {
    new_test_ext().execute_with(|| {
        seed_constitution();
        // 第 1、17 条逐字保留,新增第 99 条 → 通过不可修改校验 → 调引擎 () → NotConfigured
        assert_noop!(
            Lib::propose_amend_law(
                RuntimeOrigin::signed(legislator()),
                0,
                actor_cid_number(),
                proposer_role_code(),
                executive_cid_number(),
                legislature_cid_number(),
                VoteType::Special,
                title(b"amended-constitution"),
                None,
                chapters_of(vec![
                    article(1, b"yuan-1"),
                    article(17, b"yuan-17"),
                    article(99, b"new")
                ]),
                200,
            ),
            Error::<Test>::VoteEngineCreateFailed
        );
    });
}

#[test]
fn amend_constitution_with_regular_vote_type_rejected() {
    new_test_ext().execute_with(|| {
        seed_constitution();
        assert_noop!(
            Lib::propose_amend_law(
                RuntimeOrigin::signed(legislator()),
                0,
                actor_cid_number(),
                proposer_role_code(),
                executive_cid_number(),
                None,
                VoteType::Regular, // 宪法修改不允许常规案
                title(b"amended-constitution"),
                None,
                chapters_of(vec![article(1, b"yuan-1"), article(17, b"yuan-17")]),
                200,
            ),
            Error::<Test>::InvalidVoteTypeForConstitution
        );
    });
}

#[test]
fn repeal_constitution_rejected() {
    new_test_ext().execute_with(|| {
        seed_constitution();
        assert_noop!(
            Lib::propose_repeal_law(
                RuntimeOrigin::signed(legislator()),
                0,
                actor_cid_number(),
                proposer_role_code(),
                executive_cid_number(),
                None,
                VoteType::Special
            ),
            Error::<Test>::CannotRepealConstitution
        );
    });
}

#[test]
fn amend_nonexistent_law_rejected() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            Lib::propose_amend_law(
                RuntimeOrigin::signed(legislator()),
                404,
                actor_cid_number(),
                proposer_role_code(),
                executive_cid_number(),
                None,
                VoteType::Regular,
                title(b"x"),
                None,
                one_chapter(),
                100,
            ),
            Error::<Test>::LawNotFound
        );
    });
}

// ───────────────── 宪法章号 / 同章节号 / 条号唯一性 ─────────────────

#[test]
fn constitution_rejects_duplicate_chapter_number() {
    new_test_ext().execute_with(|| {
        let chapters = structured_chapters(vec![
            chapter(1, vec![section(1, vec![article(1, b"a")])]),
            chapter(1, vec![section(1, vec![article(2, b"b")])]),
        ]);
        assert_noop!(
            Lib::ensure_unique_constitution_numbers(&chapters),
            Error::<Test>::DuplicateChapterNumber
        );
    });
}

#[test]
fn constitution_rejects_duplicate_section_number_in_same_chapter() {
    new_test_ext().execute_with(|| {
        let chapters = structured_chapters(vec![chapter(
            1,
            vec![
                section(3, vec![article(1, b"a")]),
                section(3, vec![article(2, b"b")]),
            ],
        )]);
        assert_noop!(
            Lib::ensure_unique_constitution_numbers(&chapters),
            Error::<Test>::DuplicateSectionNumber
        );
    });
}

#[test]
fn constitution_allows_same_section_number_in_different_chapters() {
    let chapters = structured_chapters(vec![
        chapter(1, vec![section(7, vec![article(10, b"a")])]),
        chapter(2, vec![section(7, vec![article(20, b"b")])]),
    ]);
    assert_ok!(Lib::ensure_unique_constitution_numbers(&chapters));
}

#[test]
fn constitution_rejects_duplicate_article_number_across_chapters() {
    new_test_ext().execute_with(|| {
        let chapters = structured_chapters(vec![
            chapter(1, vec![section(1, vec![article(9, b"a")])]),
            chapter(2, vec![section(1, vec![article(9, b"b")])]),
        ]);
        assert_noop!(
            Lib::ensure_unique_constitution_numbers(&chapters),
            Error::<Test>::DuplicateArticleNumber
        );
    });
}

#[test]
fn amend_constitution_rejects_duplicate_chapter_before_article_lookup() {
    new_test_ext().execute_with(|| {
        seed_constitution_tiered();
        let chapters = structured_chapters(vec![
            chapter(
                1,
                vec![section(
                    1,
                    vec![
                        article(1, b"core-1"),
                        article(5, b"CHANGED"),
                        article(20, b"core-20"),
                    ],
                )],
            ),
            chapter(
                1,
                vec![section(
                    1,
                    vec![article(60, b"gen-60"), article(61, b"gen-61")],
                )],
            ),
        ]);
        assert_noop!(
            Lib::propose_amend_law(
                RuntimeOrigin::signed(legislator()),
                0,
                actor_cid_number(),
                proposer_role_code(),
                executive_cid_number(),
                legislature_cid_number(),
                VoteType::Special,
                title(b"amended-constitution"),
                None,
                chapters,
                200,
            ),
            Error::<Test>::DuplicateChapterNumber
        );
    });
}

#[test]
fn write_constitution_rechecks_duplicate_section_before_storage() {
    new_test_ext().execute_with(|| {
        seed_constitution_tiered();
        let mut summary = enact_summary(
            Tier::Constitution,
            0,
            VoteType::Major,
            b"amended-constitution",
        );
        summary.action = LawAction::Amend;
        summary.law_id = 0;
        let chapters = structured_chapters(vec![
            chapter(
                1,
                vec![section(
                    1,
                    vec![
                        article(1, b"core-1"),
                        article(5, b"core-5"),
                        article(20, b"core-20"),
                    ],
                )],
            ),
            chapter(
                2,
                vec![
                    section(1, vec![article(60, b"CHANGED")]),
                    section(1, vec![article(61, b"gen-61")]),
                ],
            ),
        ]);
        assert_noop!(
            Lib::write_law_version(9, summary, chapters, Timestamp::now()),
            Error::<Test>::DuplicateSectionNumber
        );
        assert!(LawVersions::<Test>::get(0, 2).is_none());
    });
}

// ───────────────── 宪法 constitution.scale 解码与结构校验 ─────────────────

#[test]
fn constitution_scale_decodes_and_is_well_formed() {
    use codec::Decode;
    let bytes = include_bytes!("../constitution.scale");
    let chapters = crate::pallet::ChaptersOf::<Test>::decode(&mut &bytes[..])
        .expect("constitution.scale 解码为 ChaptersOf");
    assert_ok!(Lib::ensure_unique_constitution_numbers(&chapters));
    assert_eq!(chapters.len(), 7, "7 章");
    let articles: Vec<_> = chapters
        .iter()
        .flat_map(|c| c.sections.iter())
        .flat_map(|s| s.articles.iter())
        .collect();
    assert_eq!(articles.len(), 141, "141 条");
    // 条号连续 1..=141
    let mut nums: Vec<u32> = articles.iter().map(|a| a.number).collect();
    nums.sort();
    assert_eq!(nums, (1u32..=141).collect::<Vec<_>>(), "条号连续 1..141");
    // body 必填 + 中英双语
    for a in &articles {
        assert!(!a.body.is_empty(), "条 {} body 非空", a.number);
        assert!(a.body_en.is_some(), "条 {} 英文", a.number);
        assert!(a.title_en.is_some(), "条 {} 标题英文", a.number);
    }
    // 不可修改条款齐全(第 1/2/3/17/19/24/34/42 条)
    for n in primitives::count_const::IMMUTABLE_CONSTITUTION_ARTICLES {
        assert!(
            articles.iter().any(|a| a.number == n),
            "不可修改条款 {n} 存在"
        );
    }
}

#[test]
fn genesis_seeds_constitution_as_law_zero() {
    use sp_runtime::BuildStorage;
    let mut storage = frame_system::GenesisConfig::<Test>::default()
        .build_storage()
        .expect("system genesis");
    crate::pallet::GenesisConfig::<Test>::default()
        .assimilate_storage(&mut storage)
        .expect("legislation-yuan genesis assimilate");
    let mut ext = sp_io::TestExternalities::new(storage);
    ext.execute_with(|| {
        // 宪法被注入为 law_id=0,tier=宪法,生效中。
        let law = Laws::<Test>::get(0).expect("宪法注入为 law 0");
        assert_eq!(law.tier, Tier::Constitution);
        assert_eq!(law.status, LawStatus::Effective);
        assert_eq!(law.effective_version, Some(1));
        assert_eq!(law.latest_version, 1);
        assert_eq!(law.pending_version, None);
        // 院序列 = [国家立法院]。
        assert_eq!(law.houses.len(), 1);
        let lv = LawVersions::<Test>::get(0, 1).expect("宪法版本 1");
        assert_eq!(lv.chapters.len(), 7, "7 章");
        // Node 宪法守卫按声明序镜像到 vote_type，并把后三个 u64 作为固定尾部；字段重排必须测试红。
        assert_eq!(
            lv.encode(),
            (
                lv.law_id,
                lv.version,
                lv.title.clone(),
                lv.title_en.clone(),
                lv.chapters.clone(),
                lv.content_hash,
                lv.vote_type,
                lv.proposal_id,
                lv.published_at,
                lv.effective_at,
            )
                .encode(),
            "LawVersion SCALE 字段序必须与 node 守卫镜像一致"
        );
        let label = LawVersionLabels::<Test>::get(0, 1).expect("宪法创世版本标签");
        assert_eq!(label.title.to_vec(), "创世版".as_bytes().to_vec());
        assert_eq!(
            label.title_en.expect("创世版本英文标签").to_vec(),
            "Genesis Edition".as_bytes().to_vec()
        );
        assert!(LawVersionLabels::<Test>::get(0, 2).is_none());
        let articles: usize = lv
            .chapters
            .iter()
            .flat_map(|c| c.sections.iter())
            .map(|s| s.articles.len())
            .sum();
        assert_eq!(articles, 141, "141 条");
        assert_eq!(Lib::list_laws(Tier::Constitution, 0), vec![0]);

        // 不可修改条款 manifest 已冻结:清单 = 单源常量,逐条摘要 = 实际条文(L3 创世锚)。
        use codec::Encode as _;
        let manifest = ConstitutionImmutableManifest::<Test>::get().expect("manifest 创世写入");
        assert_eq!(
            manifest.article_numbers.to_vec(),
            primitives::count_const::IMMUTABLE_CONSTITUTION_ARTICLES.to_vec(),
            "manifest 清单 = 不可修改条款单源"
        );
        for (i, &n) in primitives::count_const::IMMUTABLE_CONSTITUTION_ARTICLES
            .iter()
            .enumerate()
        {
            let article = lv
                .chapters
                .iter()
                .flat_map(|c| c.sections.iter())
                .flat_map(|s| s.articles.iter())
                .find(|a| a.number == n)
                .expect("不可修改条款存在于创世");
            assert_eq!(
                manifest.article_hashes[i],
                sp_io::hashing::blake2_256(&article.encode()),
                "manifest 摘要应与对应条文一致"
            );
        }
    });
}

use primitives::constitution::AmendmentScope;

// ───────────── 章/节标题变更纳入第十九条档位判定 ─────────────
//
// 标题是承载条文的目录结构、本身没有条号，故不并入逐条 diff 的 changed
// （那样会因所辖条号命中禁改清单而误判违宪）。第十九条禁改保护的对象是
// **条**本身：只要禁改条内容逐字未变，改其所在章节标题只按所属章走档位。

/// 改核心章(第一章)章标题 → 特别案档位。
#[test]
fn core_chapter_title_change_requires_special() {
    let core = vec![article(1, b"core-1"), article(5, b"core-5")];
    let general = vec![article(60, b"gen-60")];
    let current = chapters_core_general(core.clone(), general.clone());
    let mut new = current.clone();
    new[0].title = title("第一章 总则(修订)".as_bytes());
    assert_eq!(
        crate::pallet::Pallet::<Test>::constitution_amendment_scope(&current, &new),
        AmendmentScope::CoreChapter
    );
}

/// 改核心章内的节标题 → 特别案档位。
#[test]
fn core_section_title_change_requires_special() {
    let current = chapters_core_general(vec![article(1, b"core-1")], vec![article(60, b"gen-60")]);
    let mut new = current.clone();
    new[0].sections[0].title = title("第一节 国家的定义".as_bytes());
    assert_eq!(
        crate::pallet::Pallet::<Test>::constitution_amendment_scope(&current, &new),
        AmendmentScope::CoreChapter
    );
}

/// 改一般章(第二章)的节标题 → 重要案档位。修复前会被误判为 NoChange 而拒收提案。
#[test]
fn general_section_title_change_requires_major() {
    let current = chapters_core_general(vec![article(1, b"core-1")], vec![article(60, b"gen-60")]);
    let mut new = current.clone();
    new[1].sections[0].title = title("第一节 联邦政府".as_bytes());
    assert_eq!(
        crate::pallet::Pallet::<Test>::constitution_amendment_scope(&current, &new),
        AmendmentScope::GeneralOnly
    );
}

/// 节标题英文变更同样计入(中英任一变更即算)。
#[test]
fn section_title_en_change_counts() {
    let current = chapters_core_general(vec![article(1, b"core-1")], vec![article(60, b"gen-60")]);
    let mut new = current.clone();
    new[1].sections[0].title_en = Some(title("Section 1 Federal Government".as_bytes()));
    assert_eq!(
        crate::pallet::Pallet::<Test>::constitution_amendment_scope(&current, &new),
        AmendmentScope::GeneralOnly
    );
}

/// 禁改条所在节的标题可以改：只要禁改条本身逐字未变，就不是违宪，按所属章走档位。
/// 第 1 条是禁改条且位于核心章，故为特别案而非 ImmutableViolation。
#[test]
fn title_change_over_immutable_article_is_not_violation() {
    let current = chapters_core_general(vec![article(1, b"core-1")], vec![article(60, b"gen-60")]);
    let mut new = current.clone();
    new[0].sections[0].title = title("第一节(改名)".as_bytes());
    let scope = crate::pallet::Pallet::<Test>::constitution_amendment_scope(&current, &new);
    assert_eq!(scope, AmendmentScope::CoreChapter, "按所属章判档");
    assert_ne!(
        scope,
        AmendmentScope::ImmutableViolation,
        "改标题不等于改禁改条"
    );
}

/// 标题与条文都未变 → 空改动，仍应拒。
#[test]
fn no_title_no_article_change_is_no_change() {
    let current = chapters_core_general(vec![article(1, b"core-1")], vec![article(60, b"gen-60")]);
    let new = current.clone();
    assert_eq!(
        crate::pallet::Pallet::<Test>::constitution_amendment_scope(&current, &new),
        AmendmentScope::NoChange
    );
}

/// 一般章标题变更 + 核心章条文变更同批提交 → 取更严的一档(特别案)。
#[test]
fn mixed_change_takes_stricter_scope() {
    let current = chapters_core_general(vec![article(5, b"core-5")], vec![article(60, b"gen-60")]);
    let mut new = current.clone();
    new[1].sections[0].title = title("第一节 联邦政府".as_bytes());
    new[0].sections[0].articles[0] = article(5, b"core-5-amended");
    assert_eq!(
        crate::pallet::Pallet::<Test>::constitution_amendment_scope(&current, &new),
        AmendmentScope::CoreChapter
    );
}
