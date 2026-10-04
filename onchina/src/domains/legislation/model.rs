//! 立法与表决域数据类型(ADR-027 / 卡 20260630-onchina-legislation-console-framework)。
//!
//! 本文件放 onchina 侧的提案分类维度与通用枚举,字段/取值与链端
//! `legislation-yuan::types`(Tier / VoteType)、`legislation-vote`(STAGE_LEG_*)逐字段对齐
//! (全仓字段同名)。提案统一信封读模型(HouseRef / LegProposalState)随 Phase 1 链读落地,
//! 避免无消费方的悬空结构。

use serde::{Deserialize, Serialize};

/// 提案类型(可扩展维度)。
///
/// onchina 侧的提案分类维度,决定走哪条提案数据模板与提交链路。
/// - `Law` 映射链端 `votingengine::PROPOSAL_KIND_LEGISLATION`(本轮实现);
/// - `Personnel` / `Budget` 为未来链端新增提案种类预留,本轮只定义结构、不接链。
// Phase 0 落地并单测;Phase 1 起由 `category::proposable_candidates` 与 `law/chain_propose` 消费。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ProposalCategory {
    /// 法律案:章>节>条>款;5 种表决;立法机关发起(本轮实现)。
    Law,
    /// 任免案:任免职书;常规案(默认)/重要案(升级);政府发起(预留)。
    Personnel,
    /// 预算案:类>款>项>目;常规案;政府发起(预留)。
    Budget,
}

#[allow(dead_code)] // as_u8 预留:Phase 1+ 提交层映射链端 PROPOSAL_KIND_* 时消费。
impl ProposalCategory {
    /// onchina 侧提案类型序号(0 法律 / 1 任免 / 2 预算)。
    ///
    /// 这是 onchina 维度序号,**非**链端 `PROPOSAL_KIND_*`;链端提案种类的映射
    /// 在提交层(Phase 1+)处理,法律案对应 `PROPOSAL_KIND_LEGISLATION`。
    pub(crate) fn as_u8(&self) -> u8 {
        match self {
            ProposalCategory::Law => 0,
            ProposalCategory::Personnel => 1,
            ProposalCategory::Budget => 2,
        }
    }
}

#[cfg(test)]
// 固定日期夹具解包失败即代表模型边界回归，断言式解包仅限测试。
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn proposal_category_serializes_to_snake_case() {
        // 前端 ProposalCategory 联合类型为 'law' | 'personnel' | 'budget',必须逐字一致。
        assert_eq!(
            serde_json::to_string(&ProposalCategory::Law).unwrap(),
            "\"law\""
        );
        assert_eq!(
            serde_json::to_string(&ProposalCategory::Personnel).unwrap(),
            "\"personnel\""
        );
        assert_eq!(
            serde_json::to_string(&ProposalCategory::Budget).unwrap(),
            "\"budget\""
        );
    }

    #[test]
    fn proposal_category_index_matches_doc() {
        assert_eq!(ProposalCategory::Law.as_u8(), 0);
        assert_eq!(ProposalCategory::Personnel.as_u8(), 1);
        assert_eq!(ProposalCategory::Budget.as_u8(), 2);
    }
}
