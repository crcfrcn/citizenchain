//! 验证全局提案主键、年度展示号和反向索引的独立性。

use super::*;
// 这些 storage 在 engingcore 主 crate;dual_id 测试通过 super::* 拿到 engingcore::pallet 的 re-import。

/// 走 `_with_data` 路径触发 `register_proposal_data` 与反向索引写入。
fn create_institution_proposal_with_data_via_engine(
    who: AccountId32,
    institution_code: InstitutionCode,
    actor_cid_number: CidNumber,
    module_tag: &[u8],
) -> u128 {
    <InternalVote as InternalVoteEngine<AccountId32>>::create_institution_proposal_with_data(
        who,
        institution_code,
        actor_cid_number.to_vec(),
        None,
        subject_cids_for(&actor_cid_number),
        internal_vote_plan_with_owner(&actor_cid_number, module_tag, b"payload"),
        b"payload".to_vec(),
    )
    .expect("internal proposal with data should be created")
}

/// 主键全局单调:首个 = 0,逐次 +1。
#[test]
fn proposal_id_is_globally_monotonic_starting_from_zero() {
    new_test_ext().execute_with(|| {
        let id0 = create_internal_proposal_via_engine(nrc_admin(0), NRC, nrc_cid());
        let id1 = create_internal_proposal_via_engine(nrc_admin(0), NRC, nrc_cid());
        let id2 = create_internal_proposal_via_engine(nrc_admin(0), NRC, nrc_cid());
        assert_eq!(id0, 0);
        assert_eq!(id1, 1);
        assert_eq!(id2, 2);
        assert_eq!(NextProposalId::<Test>::get(), 3);
    });
}

/// 跨年:主键单调累加,展示号 `seq_in_year` 重置为 0。
#[test]
fn display_meta_seq_in_year_resets_across_year_boundary() {
    new_test_ext().execute_with(|| {
        // 2026 年内两条
        let id0 = create_internal_proposal_via_engine(nrc_admin(0), NRC, nrc_cid());
        let id1 = create_internal_proposal_via_engine(nrc_admin(0), NRC, nrc_cid());
        assert_eq!(id0, 0);
        assert_eq!(id1, 1);
        let d0 = ProposalDisplayId::<Test>::get(id0).unwrap();
        let d1 = ProposalDisplayId::<Test>::get(id1).unwrap();
        assert_eq!(d0.year, 2026);
        assert_eq!(d0.seq_in_year, 0);
        assert_eq!(d1.year, 2026);
        assert_eq!(d1.seq_in_year, 1);

        // 跨到 2027
        set_test_now_secs(1_830_297_599);
        let id2 = create_internal_proposal_via_engine(nrc_admin(0), NRC, nrc_cid());
        assert_eq!(id2, 2); // 主键继续 +1
        let d2 = ProposalDisplayId::<Test>::get(id2).unwrap();
        assert_eq!(d2.year, 2027);
        assert_eq!(d2.seq_in_year, 0); // 年内序号重置
    });
}

/// 跨年累加器解 cap:无论多少都不返回 `YearCounterOverflow`(u32 上限 42.9 亿)。
///
/// 这里直接灌 100 万 + 1 条到 `YearProposalCounter`,模拟"千万级/年"目标的边界。
#[test]
fn year_proposal_counter_no_longer_capped_at_one_million() {
    new_test_ext().execute_with(|| {
        // 先创建一条让 CurrentProposalYear 进入 2026 分支(否则 stored_year=0 触发重置)
        let _ = create_internal_proposal_via_engine(nrc_admin(0), NRC, nrc_cid());
        // 强制把 YearProposalCounter 设为历史上限值，确认当前代码不会再拒绝。
        YearProposalCounter::<Test>::put(1_000_000u64);
        // 当前实现仍能成功创建；历史实现会在此处返回 ProposalIdOverflow。
        let id = create_internal_proposal_via_engine(nrc_admin(0), NRC, nrc_cid());
        let display = ProposalDisplayId::<Test>::get(id).unwrap();
        assert_eq!(display.seq_in_year, 1_000_000);
        assert_eq!(YearProposalCounter::<Test>::get(), 1_000_001);
    });
}

/// 反向索引:`register_proposal_data` 后 4 张索引各有一条。
#[test]
fn reverse_indexes_populated_after_register_proposal_data() {
    new_test_ext().execute_with(|| {
        let id = create_institution_proposal_with_data_via_engine(
            nrc_admin(0),
            NRC,
            nrc_cid(),
            b"test-tag",
        );

        // ProposalsByCode
        assert!(ProposalsByCode::<Test>::contains_key(NRC, id));
        // ProposalsByCid
        assert!(ProposalsByCid::<Test>::contains_key(nrc_cid(), id));
        // ProposalsByYear
        let display = ProposalDisplayId::<Test>::get(id).unwrap();
        assert!(ProposalsByYear::<Test>::contains_key(display.year, id));
        // ProposalsByOwner — 用注册时传入的 module_tag
        let owner = engingcore::pallet::ProposalOwner::<Test>::get(id).expect("owner present");
        assert!(ProposalsByOwner::<Test>::contains_key(owner, id));
    });
}

/// 清理路径(FinalCleanup)同步删除 4 张反向索引 + ProposalDisplayId。
#[test]
fn final_cleanup_removes_indexes_and_display_id() {
    new_test_ext().execute_with(|| {
        let id = create_institution_proposal_with_data_via_engine(
            nrc_admin(0),
            NRC,
            nrc_cid(),
            b"cleanup-tag",
        );
        let display = ProposalDisplayId::<Test>::get(id).unwrap();
        let owner = engingcore::pallet::ProposalOwner::<Test>::get(id).expect("owner present");

        // 走完整 FinalCleanup 路径
        VotingEngine::cleanup_proposal_indexes(id);
        engingcore::pallet::Proposals::<Test>::remove(id);
        engingcore::pallet::ProposalOwner::<Test>::remove(id);

        assert!(!ProposalDisplayId::<Test>::contains_key(id));
        assert!(!ProposalsByCode::<Test>::contains_key(NRC, id));
        assert!(!ProposalsByCid::<Test>::contains_key(nrc_cid(), id));
        assert!(!ProposalsByYear::<Test>::contains_key(display.year, id));
        assert!(!ProposalsByOwner::<Test>::contains_key(owner, id));
    });
}


#[test]
fn proposal_id_and_annual_sequence_accept_values_beyond_the_old_widths() {
    new_test_ext().execute_with(|| {
        NextProposalId::<Test>::put(u128::from(u64::MAX) + 1);
        CurrentProposalYear::<Test>::put(2026u32);
        YearProposalCounter::<Test>::put(u64::from(u32::MAX) + 1);
        let id = engingcore::Pallet::<Test>::allocate_proposal_id().unwrap();
        assert_eq!(id, u128::from(u64::MAX) + 1);
        let display = ProposalDisplayId::<Test>::get(id).unwrap();
        assert_eq!((display.year, display.seq_in_year), (2026u32, u64::from(u32::MAX) + 1));
        assert_eq!(engingcore::Pallet::<Test>::unix_seconds_to_year(
            (1970..65536).map(|year| u64::from(engingcore::Pallet::<Test>::days_in_year(year)) * 86400).sum()
        ).unwrap(), 65536u32);
    });
}

#[test]
fn proposal_counter_overflow_never_wraps_or_consumes_an_id() {
    new_test_ext().execute_with(|| {
        NextProposalId::<Test>::put(u128::MAX);
        assert_noop!(engingcore::Pallet::<Test>::allocate_proposal_id(), engingcore::Error::<Test>::ProposalIdOverflow);
        assert_eq!(NextProposalId::<Test>::get(), u128::MAX);
        NextProposalId::<Test>::put(123u128);
        CurrentProposalYear::<Test>::put(2026u32);
        YearProposalCounter::<Test>::put(u64::MAX);
        assert_noop!(engingcore::Pallet::<Test>::allocate_proposal_id(), engingcore::Error::<Test>::YearCounterOverflow);
        assert_eq!(NextProposalId::<Test>::get(), 123u128);
        assert!(!ProposalDisplayId::<Test>::contains_key(123u128));
    });
}
