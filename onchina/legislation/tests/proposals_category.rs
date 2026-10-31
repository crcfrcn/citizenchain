//! 原所属模块回归，断言与输入保持。
use super::*;

#[test]
fn review_house_has_no_proposal_candidates() {
    // 参议会只表决不发起:有角色但无候选。
    assert_eq!(legislation_role("NSN"), Some(LegislationRole::ReviewHouse));
    assert_eq!(legislation_role("PSN"), Some(LegislationRole::ReviewHouse));
    assert!(proposable_candidates("NSN").is_empty());
    assert!(proposable_candidates("PSN").is_empty());
}

#[test]
fn education_committee_proposes_only_education_vote_types() {
    let national = proposable_candidates("NED");
    assert_eq!(national.len(), 1);
    assert_eq!(national[0].category, ProposalCategory::Law);
    assert_eq!(national[0].tier, 1);
    assert_eq!(national[0].vote_types, vec![1, 3]); // 常规教育 / 重要教育
    assert_eq!(proposable_candidates("CEDU")[0].vote_types, vec![1, 3]);
}

#[test]
fn houses_propose_non_education_at_their_tier() {
    assert_eq!(proposable_candidates("NRP")[0].tier, 1);
    assert_eq!(proposable_candidates("PRP")[0].tier, 2);
    for code in ["CLEG", "CSLF"] {
        let candidates = proposable_candidates(code);
        assert_eq!(candidates[0].tier, 3);
        assert_eq!(candidates[0].vote_types, vec![0, 2, 4]); // 常规 / 重要 / 特别
    }
}

#[test]
fn non_legislative_institution_has_no_role_or_candidates() {
    // 注册局/政府机构本轮无立法角色;政府任免/预算案候选待链端 kind 上线接入(schema 已 Phase 4 锁定)。
    assert!(legislation_role("FRG").is_none());
    assert!(legislation_role("CGOV").is_none());
    assert!(proposable_candidates("CGOV").is_empty());
}
