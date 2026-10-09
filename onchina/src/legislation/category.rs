//! 提案类型维度:本节点绑定机构码 → 立法角色 + 可发起的提案候选。
//!
//! OnChina 每节点单机构绑定,故「本节点能发起什么」由绑定机构码唯一决定。
//! 本文件是立法机构 → 角色/候选的**声明式单源**:能力位下发(`platform/capability.rs`)与
//! 候选解析共用此处,避免两处各写一份机构分类(全仓单源)。
//!
//! 宪法依据(机构码 = `primitives::cid::code` 真源):
//! - 众议会(NRP/PRP)：起草发起除教育外法案，并参加代表机构表决；无终审权。
//! - 国家教委会(NED):起草发起教育类法案,本会先内部表决(宪法第75条第2款)。
//! - 市教委会(CEDU)/市自治会(CSLF):向市立法会提案,自身不表决(宪法第46条)。
//! - 市立法会委员(CLEG):提案 + 单院表决(宪法第46/110条)。
//! - 参议会(NSN/PSN):只审议/终审,无发起权(宪法第45/100/106条)。
//!
//! 提案主体与表决院(houses)的解耦、合法性裁决全在链端 `legislation-yuan::ensure_routing`,
//! 本文件只声明候选,不做链上裁决。

use super::model::ProposalCategory;

/// 立法角色（决定能否发起、能否参加代表机构表决）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LegislationRole {
    /// 发起机构：发起法律案并参加代表机构表决。
    ProposerHouse,
    /// 复议/终审机构：只参加代表机构表决，无发起权。
    ReviewHouse,
    /// 仅提案不表决：向表决机构提案，自身不参加代表机构表决。
    ProposerOnly,
}

/// 机构码文本 → 立法角色;非立法机构返回 `None`。
///
/// 机构码为各省/市共用文本码(N* 国家 / P* 省 / C* 市),实例按行政区区分,
/// 与立法角色无关,故按文本码分类即可。
pub(crate) fn legislation_role(institution_code: &str) -> Option<LegislationRole> {
    match institution_code {
        "NRP" | "PRP" | "CLEG" | "NED" => Some(LegislationRole::ProposerHouse),
        "NSN" | "PSN" => Some(LegislationRole::ReviewHouse),
        "CEDU" | "CSLF" => Some(LegislationRole::ProposerOnly),
        _ => None,
    }
}

/// 一条可发起候选:提案类型 + 层级 + 该类型下本机构可选的表决类型集合。
///
/// 由发起菜单候选 API(`handler::list_proposable`)消费;当前 `category` 恒 `Law`。
/// 任免案/预算案 schema 已于 Phase 4 锁定（`personnel`/`budget` 子域），但候选发起必须等
/// 对应业务模块和代表机构表决接线完成；无链上提交路径前不列候选，避免半桩入口。
pub(crate) struct ProposableCandidate {
    /// 提案类型(本轮仅 `Law`)。
    pub(crate) category: ProposalCategory,
    /// 层级(对齐链 `Tier::as_u8`:1 国家 / 2 省 / 3 市)。
    pub(crate) tier: u8,
    /// 可选表决类型(对齐链 `VoteType::as_u8`:0 常规 / 1 常规教育 / 2 重要 / 3 重要教育 / 4 特别)。
    pub(crate) vote_types: Vec<u8>,
}

/// 本节点机构码 → 可发起候选清单。
///
/// 参议会（NSN/PSN）无发起权返回空；政府任免/预算案候选待对应业务模块接入后增加，
/// schema 已于 Phase 4 锁定，但无提交路径前不列候选。
/// 最终合法性以链端 `ensure_routing` 为准。Phase 0 落地并单测;Phase 1 起由发起菜单候选 API 消费。
pub(crate) fn proposable_candidates(institution_code: &str) -> Vec<ProposableCandidate> {
    // 非教育表决类型:常规/重要/特别(众议会、市立法会、市自治会)。
    const NON_EDUCATION: [u8; 3] = [0, 2, 4];
    // 教育表决类型:常规教育/重要教育(教委会专属)。
    const EDUCATION: [u8; 2] = [1, 3];
    match institution_code {
        "NRP" => vec![law(1, &NON_EDUCATION)], // 国家众议会:国家级非教育法案
        "NED" => vec![law(1, &EDUCATION)],     // 国家教委会:国家级教育法案
        "PRP" => vec![law(2, &NON_EDUCATION)], // 省众议会:省级非教育法案
        "CLEG" | "CSLF" => vec![law(3, &NON_EDUCATION)], // 市立法会委员/市自治会委员:市级非教育
        "CEDU" => vec![law(3, &EDUCATION)],    // 市教委会:市级教育法案
        _ => Vec::new(),
    }
}

/// 构造一条法律案候选。
fn law(tier: u8, vote_types: &[u8]) -> ProposableCandidate {
    ProposableCandidate {
        category: ProposalCategory::Law,
        tier,
        vote_types: vote_types.to_vec(),
    }
}

#[cfg(test)]
mod tests {
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
}
