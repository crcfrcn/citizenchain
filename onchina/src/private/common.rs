//! 私权机构类型单一来源。
//!
//! 这里只定义私权机构的机构码分类。
//! `ZG/TG` 只服务公民/自然人/智能人等人类主体来源分类,不用于私权机构。

use serde::{Deserialize, Serialize};

use crate::domains::private::participants::ParticipantRole;
use crate::institution::subjects::CreateInstitutionInput;

/// 私权机构业务类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub(crate) enum PrivateType {
    Sole,
    Partnership,
    Company,
    Corporation,
    Welfare,
    Association,
}

impl PrivateType {
    pub(crate) fn from_str(value: &str) -> Option<Self> {
        match value.trim() {
            "SOLE" => Some(Self::Sole),
            "PARTNERSHIP" => Some(Self::Partnership),
            "COMPANY" => Some(Self::Company),
            "CORPORATION" => Some(Self::Corporation),
            "WELFARE" => Some(Self::Welfare),
            "ASSOCIATION" => Some(Self::Association),
            _ => None,
        }
    }

    pub(crate) fn as_code(self) -> &'static str {
        match self {
            Self::Sole => "SOLE",
            Self::Partnership => "PARTNERSHIP",
            Self::Company => "COMPANY",
            Self::Corporation => "CORPORATION",
            Self::Welfare => "WELFARE",
            Self::Association => "ASSOCIATION",
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Sole => "个体经营",
            Self::Partnership => "合伙企业",
            Self::Company => "股权公司",
            Self::Corporation => "股份公司",
            Self::Welfare => "公益组织",
            Self::Association => "注册协会",
        }
    }
}

/// 合伙企业内部形态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub(crate) enum PartnershipKind {
    General,
    Limited,
}

impl PartnershipKind {
    pub(crate) fn from_str(value: &str) -> Option<Self> {
        match value.trim() {
            "GENERAL" => Some(Self::General),
            "LIMITED" => Some(Self::Limited),
            _ => None,
        }
    }

    pub(crate) fn as_code(self) -> &'static str {
        match self {
            Self::General => "GENERAL",
            Self::Limited => "LIMITED",
        }
    }
}

/// 创建私权机构时由类型规则锁定的身份字段。
#[derive(Debug, Clone, Copy)]
pub(crate) struct PrivateTypeRule {
    pub(crate) private_type: PrivateType,
    pub(crate) partnership_kind: Option<PartnershipKind>,
    pub(crate) institution_code: &'static str,
    /// `Some` 表示由类型规则固定；`None` 表示必须由本次机构实例显式提交。
    pub(crate) p1: Option<&'static str>,
}

/// 私权机构真实子模块的静态边界描述。
#[derive(Debug, Clone, Copy)]
pub(crate) struct PrivateModuleSpec {
    pub(crate) route_segment: &'static str,
    pub(crate) private_type: PrivateType,
    pub(crate) title: &'static str,
    pub(crate) description: &'static str,
    pub(crate) allowed_participant_roles: &'static [ParticipantRole],
}

/// 把请求强制锁定为某个私权类型。调用方只传业务类型,不信任前端的主体属性和机构码。
pub(crate) fn lock_input_to_rule(input: &mut CreateInstitutionInput, rule: PrivateTypeRule) {
    input.private_type = Some(rule.private_type.as_code().to_string());
    input.partnership_kind = rule.partnership_kind.map(|kind| kind.as_code().to_string());
    input.institution = rule.institution_code.to_string();
    // 注册协会 SFAS 的盈利属性由本次实例显式选择；其它私权类型继续由类型规则锁定。
    if let Some(p1) = rule.p1 {
        input.p1 = Some(p1.to_string());
    }
    // 六类目标私权机构都是独立主体;非法人个体经营/无限合伙也不挂靠所属法人。
    input.parent_cid_number = None;
}

/// 通用非合伙私权类型的规则解析。合伙企业必须显式走 partnership 模块校验。
pub(crate) fn fixed_rule(private_type: PrivateType) -> Result<PrivateTypeRule, &'static str> {
    resolve_private_type_rule(private_type.as_code(), None)
}

/// 模块边界运行期自检。开发期用 debug_assert 暴露空配置,生产期无额外返回成本。
pub(crate) fn assert_module_spec(spec: &PrivateModuleSpec) {
    debug_assert!(!spec.route_segment.is_empty());
    debug_assert_eq!(spec.private_type.label(), spec.title);
    debug_assert!(!spec.description.is_empty());
    debug_assert!(!spec.allowed_participant_roles.is_empty());
    for participant_role in spec.allowed_participant_roles {
        debug_assert!(!participant_role.label().is_empty());
    }
}

/// 按私权类型解析身份字段。调用方不得让前端自带 institution_code 覆盖本规则。
pub(crate) fn resolve_private_type_rule(
    private_type: &str,
    partnership_kind: Option<&str>,
) -> Result<PrivateTypeRule, &'static str> {
    let private_type =
        PrivateType::from_str(private_type).ok_or("private_type must be a valid private type")?;
    let rule = match private_type {
        PrivateType::Sole => PrivateTypeRule {
            private_type,
            partnership_kind: None,
            institution_code: "SFGT",
            p1: Some("1"),
        },
        PrivateType::Partnership => match partnership_kind
            .and_then(PartnershipKind::from_str)
            .ok_or("partnership_kind must be GENERAL or LIMITED")?
        {
            PartnershipKind::General => PrivateTypeRule {
                private_type,
                partnership_kind: Some(PartnershipKind::General),
                institution_code: "SFGP",
                p1: Some("1"),
            },
            PartnershipKind::Limited => PrivateTypeRule {
                private_type,
                partnership_kind: Some(PartnershipKind::Limited),
                institution_code: "SFLP",
                p1: Some("1"),
            },
        },
        PrivateType::Company => PrivateTypeRule {
            private_type,
            partnership_kind: None,
            institution_code: "SFGQ",
            p1: Some("1"),
        },
        PrivateType::Corporation => PrivateTypeRule {
            private_type,
            partnership_kind: None,
            institution_code: "SFGF",
            p1: Some("1"),
        },
        PrivateType::Welfare => PrivateTypeRule {
            private_type,
            partnership_kind: None,
            institution_code: "SFGY",
            p1: Some("0"),
        },
        PrivateType::Association => PrivateTypeRule {
            private_type,
            partnership_kind: None,
            // 注册协会(SFAS)的 p1 由实例输入；类型规则不提供默认值。
            institution_code: "SFAS",
            p1: None,
        },
    };
    Ok(rule)
}

/// 由私权机构码反推 private_type 展示码(如 `SFGY` → `WELFARE`)。
///
/// 链上 Institutions 只存机构码,不存 private_type —— 后者是按机构码确定性派生的业务分类。
/// 联邦 drill-in 把纯链上私权机构投影进本地库时,本地无既有行可继承 private_type,必须据机构码
/// 回填,否则私权列表 SQL 的 `private_type IS NOT NULL` 会把纯投影的私权机构(如创世公民链
/// 技术发展基金会 SFGY)挡在列表外。非私权机构码返回 `None`(不写 private_type)。
pub(crate) fn private_type_code_from_institution_code(
    institution_code: &str,
) -> Option<&'static str> {
    match institution_code {
        "SFGT" => Some(PrivateType::Sole.as_code()),
        "SFGP" | "SFLP" => Some(PrivateType::Partnership.as_code()),
        "SFGQ" => Some(PrivateType::Company.as_code()),
        "SFGF" => Some(PrivateType::Corporation.as_code()),
        "SFGY" => Some(PrivateType::Welfare.as_code()),
        "SFAS" => Some(PrivateType::Association.as_code()),
        _ => None,
    }
}

#[cfg(test)]
// 私权机构类型夹具必须可解析，断言式解包仅限测试模块。
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn association_keeps_instance_p1_while_fixed_types_define_p1() {
        let association = resolve_private_type_rule("ASSOCIATION", None).expect("association");
        assert_eq!(association.institution_code, "SFAS");
        assert_eq!(association.p1, None);

        let company = resolve_private_type_rule("COMPANY", None).expect("company");
        assert_eq!(company.p1, Some("1"));
        let welfare = resolve_private_type_rule("WELFARE", None).expect("welfare");
        assert_eq!(welfare.p1, Some("0"));
    }
}
