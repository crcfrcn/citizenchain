//! 定义四级收支科目和叶级金额的预算案数据结构。

#![allow(dead_code)] // Phase 4 预留:整模块为预算案数据形占位,尚无生产消费方。

use serde::{Deserialize, Serialize};

use crate::model::ProposalCategory;

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
#[serde(deny_unknown_fields)]
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
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct BudgetItem {
    pub code: String,
    pub name: String,
    pub subitems: Vec<BudgetSubitem>,
}

/// 款(目录 + 项列表)。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct BudgetSection {
    pub code: String,
    pub name: String,
    pub items: Vec<BudgetItem>,
}

/// 类(目录 + 款列表;收支科目顶层)。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct BudgetClass {
    pub code: String,
    pub name: String,
    pub sections: Vec<BudgetSection>,
}

/// 预算总案(某政府某会计年度的完整收支预算)。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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
/// 表决院(houses)由后端按层级与原机构 CID 解析(立法机关单院),不收前端;
/// 地域由后端原机构 CID 派生，不接收前端数字占位值。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct ProposeBudgetInput {
    /// 层级(1 国家 / 2 省 / 3 市)。
    pub tier: u8,
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
#[allow(clippy::expect_used, clippy::unwrap_used)]
#[path = "../tests/budget_model.rs"]
mod tests;
