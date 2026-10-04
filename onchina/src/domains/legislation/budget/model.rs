//! 预算案(政府收支预算)字段 schema —— Phase 4 预留,仅锁数据形。
//!
//! 四级收支科目 类>款>项>目;**仅「目」(叶子)携金额**,类/款/项 金额由子项汇总
//! (展示/服务层计算,不冗余存储,避免重复计数)。金额单位**分**(`u128`),序列化为**字符串**
//! (国家级预算约 10^16 分,超 JS `Number` 安全整数 2^53,必须以 string 承载防精度丢失)。
//! `code` 编码规则(国标 vs 自定义)待定,当前自由文本。camelCase 出线对齐既有 DTO 契约。

// Phase 4 预留:未接线的预算案 schema,待立法控制台预算线(legislation-console-framework)上线时消费。
#![allow(dead_code)] // Phase 4 预留:整模块为预算案数据形占位,尚无生产消费方。

use serde::{Deserialize, Serialize};

use crate::domains::legislation::model::ProposalCategory;

/// `u128`(分)与 JSON 字符串互转的 serde 适配器。
///
/// 预算金额可超 2^53,JSON 数字在 JS 侧会丢精度,故金额一律以字符串出线/入线。
mod fen_string {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(value: &u128, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&value.to_string())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<u128, D::Error> {
        let raw = String::deserialize(deserializer)?;
        raw.parse::<u128>().map_err(serde::de::Error::custom)
    }
}

/// 目(收支科目最末层,唯一携金额的叶子)。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BudgetSubitem {
    /// 科目码(编码规则待定,当前自由文本)。
    pub code: String,
    /// 科目名。
    pub name: String,
    /// 收入(分,字符串承载 u128)。
    #[serde(with = "fen_string")]
    pub revenue: u128,
    /// 支出(分,字符串承载 u128)。
    #[serde(with = "fen_string")]
    pub expenditure: u128,
}

/// 项(目录 + 目列表;自身不携金额,金额 = 子目汇总)。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BudgetItem {
    pub code: String,
    pub name: String,
    pub subitems: Vec<BudgetSubitem>,
}

/// 款(目录 + 项列表)。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BudgetSection {
    pub code: String,
    pub name: String,
    pub items: Vec<BudgetItem>,
}

/// 类(目录 + 款列表;收支科目顶层)。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BudgetClass {
    pub code: String,
    pub name: String,
    pub sections: Vec<BudgetSection>,
}

/// 预算总案(某政府某会计年度的完整收支预算)。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BudgetPlan {
    /// 预算主体机构码(提交预算的政府 CID 机构码)。
    pub budget_entity_code: String,
    /// 会计年度(公历年,如 2027)。
    pub fiscal_year: u16,
    /// 类>款>项>目 收支科目。
    pub categories: Vec<BudgetClass>,
    /// 收入合计(分,= 全部「目」revenue 之和)。
    #[serde(with = "fen_string")]
    pub total_revenue: u128,
    /// 支出合计(分,= 全部「目」expenditure 之和)。
    #[serde(with = "fen_string")]
    pub total_expenditure: u128,
}

/// 发起预算案请求体。
///
/// 表决院(houses)由后端按 `tier` + `scope_code` 解析(立法机关单院),不收前端;
/// `scope_code` 由会话派生覆盖(对齐 `ProposeLawInput` 纪律)。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProposeBudgetInput {
    /// 层级(1 国家 / 2 省 / 3 市)。
    pub tier: u8,
    /// 行政区码(后端会话派生覆盖)。
    pub scope_code: u32,
    /// 表决类型(预算案为常规案 0)。
    pub vote_type: u8,
    /// 预算总案。
    pub plan: BudgetPlan,
}

impl ProposeBudgetInput {
    /// 提案类型判别(单源 `ProposalCategory::Budget`)。
    pub fn category() -> ProposalCategory {
        ProposalCategory::Budget
    }
}

#[cfg(test)]
// 预算模型夹具异常必须立即失败，断言式解包不进入生产路径。
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    fn sample_plan() -> BudgetPlan {
        BudgetPlan {
            budget_entity_code: "NGV".to_string(),
            fiscal_year: 2027,
            categories: vec![BudgetClass {
                code: "1".to_string(),
                name: "一般公共服务".to_string(),
                sections: vec![BudgetSection {
                    code: "01".to_string(),
                    name: "人大事务".to_string(),
                    items: vec![BudgetItem {
                        code: "0101".to_string(),
                        name: "行政运行".to_string(),
                        subitems: vec![BudgetSubitem {
                            code: "010101".to_string(),
                            name: "基本支出".to_string(),
                            revenue: 0,
                            // 100 万亿元 = 10^16 分,超 JS 安全整数 2^53(≈9.007×10^15),
                            // 裸数字会在 JS 侧丢精度,必须字符串承载——用超阈值取样才真验证。
                            expenditure: 10_000_000_000_000_000,
                        }],
                    }],
                }],
            }],
            total_revenue: 0,
            total_expenditure: 10_000_000_000_000_000,
        }
    }

    /// 预算金额以**字符串**出线(防 JS Number 精度丢失),结构为 camelCase。
    #[test]
    fn budget_amounts_serialize_as_strings() {
        let json = serde_json::to_string(&sample_plan()).expect("serialize plan");
        // 金额必须是带引号的字符串,而非裸数字(取样值超 2^53,裸数字会丢精度)。
        assert!(json.contains("\"totalExpenditure\":\"10000000000000000\""));
        assert!(json.contains("\"expenditure\":\"10000000000000000\""));
        assert!(json.contains("\"totalRevenue\":\"0\""));
        assert!(json.contains("\"budgetEntityCode\":\"NGV\""));
        assert!(json.contains("\"fiscalYear\":2027"));
        // 不得把金额写成裸数字。
        assert!(!json.contains(":10000000000000000"));
    }

    /// 金额字符串可无损反序列化回 u128(round-trip 保精度)。
    #[test]
    fn budget_amounts_roundtrip_without_precision_loss() {
        let json = serde_json::to_string(&sample_plan()).expect("serialize");
        let back: BudgetPlan = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.total_expenditure, 10_000_000_000_000_000u128);
        assert_eq!(
            back.categories[0].sections[0].items[0].subitems[0].expenditure,
            10_000_000_000_000_000u128
        );
        assert_eq!(ProposeBudgetInput::category(), ProposalCategory::Budget);
        assert_eq!(ProposeBudgetInput::category().as_u8(), 2);
    }
}
