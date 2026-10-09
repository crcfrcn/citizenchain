//! 任免案(人事任免职书)字段 schema —— Phase 4 预留,仅锁数据形。
//!
//! 字段规格取自任务卡权威定稿。职位码表(`office` 真源:机构码 + 职务)与升级路径
//! 字段化(`reject_count`/`escalated`,第53/55/57/64条)为**显式待定项**,随任免案链路上线时定,
//! 本轮以机构码 + 自由文本职务名承载,不引入投机字段。camelCase 出线对齐既有 DTO 契约。

// Phase 4 预留:未接线的任免案 schema,待立法控制台任免线(legislation-console-framework)上线时消费。
#![allow(dead_code)] // Phase 4 预留:整模块为任免案数据形占位,尚无生产消费方。

use serde::{Deserialize, Serialize};

use crate::domains::legislation::model::ProposalCategory;

/// 任免动作(任命 / 免职 / 替任=免旧+任新一体)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PersonnelAction {
    /// 任命(下发任职书)。
    Appoint,
    /// 免职(下发免职书)。
    Dismiss,
    /// 替任(免现任 + 任新人,一体决定)。
    Replace,
}

/// 任免职书正文(单一职位的任免决定)。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersonnelDecision {
    /// 任免动作。
    pub action: PersonnelAction,
    /// 职位所在机构码(如 NRP 国家部 / PGV 省政府;职位码表未定前 = 机构码 + 职务名)。
    pub office_institution_code: String,
    /// 职务名(部长/副部长/省长/市长…;职位码表未定前为自由文本)。
    pub office_title: String,
    /// 职位序号(同职多席时,如副部长 ≤3 席;单席为 1)。
    pub office_seat: u32,
    /// 被任免人 CID 号(实名锚)。
    pub nominee_cid_number: String,
    /// 被任免人姓名(展示)。
    pub nominee_name: String,
    /// 第几届(宪法各条「任职不得超过 2 届」)。
    pub term_index: u32,
    /// 任期年限(宪法各条,多为 5 年)。
    pub term_years: u32,
    /// 任免理由 / 依据说明。
    pub reason: String,
}

/// 发起任免案请求体。
///
/// 表决院(houses)由后端按 `tier` + `scope_code` 解析(参议会/市立法会单院),
/// 不收前端;`scope_code` 亦由会话派生覆盖,不信前端(对齐 `ProposeLawInput` 纪律)。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProposePersonnelInput {
    /// 层级(1 国家 / 2 省 / 3 市)。
    pub tier: u8,
    /// 行政区码(后端会话派生覆盖)。
    pub scope_code: u32,
    /// 表决类型(默认常规案 0;升级重要案 2 随链路上线)。
    pub vote_type: u8,
    /// 任免职书正文。
    pub decision: PersonnelDecision,
}

impl ProposePersonnelInput {
    /// 提案类型判别(单源 `ProposalCategory::Personnel`)。
    pub fn category() -> ProposalCategory {
        ProposalCategory::Personnel
    }
}

#[cfg(test)]
// 人事提案模型夹具必须满足固定校验，断言式解包仅限测试。
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    fn sample_decision() -> PersonnelDecision {
        PersonnelDecision {
            action: PersonnelAction::Appoint,
            office_institution_code: "NRP".to_string(),
            office_title: "部长".to_string(),
            office_seat: 1,
            nominee_cid_number: "LN001-NRC0G-944805165-2026".to_string(),
            nominee_name: "张三".to_string(),
            term_index: 1,
            term_years: 5,
            reason: "依宪法第55条第1款提名任命".to_string(),
        }
    }

    /// 任免职书序列化为 camelCase + action 为 snake_case 枚举文案(锁定 CitizenApp 契约)。
    #[test]
    fn personnel_decision_serializes_camel_case() {
        let json = serde_json::to_string(&sample_decision()).expect("serialize decision");
        assert!(json.contains("\"action\":\"appoint\""));
        assert!(json.contains("\"officeInstitutionCode\":\"NRP\""));
        assert!(json.contains("\"nomineeCidNumber\":"));
        assert!(json.contains("\"termYears\":5"));
        // 不得出现 snake_case 字段名。
        assert!(!json.contains("office_institution_code"));
    }

    /// ProposePersonnelInput 从 camelCase JSON 反序列化,并携正确提案类型判别。
    #[test]
    fn propose_personnel_input_roundtrips_and_categorizes() {
        let input = ProposePersonnelInput {
            tier: 1,
            scope_code: 0,
            vote_type: 0,
            decision: sample_decision(),
        };
        let json = serde_json::to_value(&input.decision).expect("value");
        let back: PersonnelDecision =
            serde_json::from_value(json).expect("deserialize decision back");
        assert_eq!(back.action, PersonnelAction::Appoint);
        assert_eq!(back.office_seat, 1);
        assert_eq!(
            ProposePersonnelInput::category(),
            ProposalCategory::Personnel
        );
        assert_eq!(ProposePersonnelInput::category().as_u8(), 1);
    }
}
