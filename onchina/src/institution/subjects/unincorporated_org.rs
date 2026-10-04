//! 非法人机构能力。
//!
//! 非法人不是法人资格,但不等于全部必须挂靠。个体经营(SFGT)和无限合伙(SFGP)
//! 是独立非法人;非法人组织(UNIN,含学校分校/公权下属/公司分支)必须从属于一个法人主体。
//! 公权机构和私权机构都可能拥有从属非法人机构,所以能力放在 `subjects/unincorporated_org`。
//!
//! 本模块是非法人挂靠规则的单一权威源,创建(create_institution)、改挂
//! (update_institution)和所属法人搜索(search_parent_institutions 的 SQL 预过滤)
//! 三处必须与这里同源,缺一处就有绕过口:
//! - 父级属性:仅私法人(S)/公法人(G)可作所属法人;UNIN 通用从属,挂任意 S/G 父级;
//! - 地域规则:见 [`parent_locality_rule`](父级是学校/大学 → 分校同市);
//! - 盈利属性继承:见 [`inherited_p1`]。

use crate::cid::{code, AdminLevel};

pub(crate) fn requires_parent(institution_code: &str) -> bool {
    // 个体经营(SFGT)/无限合伙(SFGP)是独立非法人;只有非法人组织(UNIN)必须挂靠法人父级。
    institution_code == "UNIN"
}

pub(crate) fn can_attach_to_parent(parent_institution_code: &str) -> bool {
    // 父级是公法人或私法人才可作所属法人。
    code::institution_code_from_str(parent_institution_code)
        .is_some_and(|c| code::is_public_legal_code(&c) || code::is_private_legal_code(&c))
}

pub(crate) fn parent_subject_requirement_message() -> &'static str {
    "所属法人必须是私法人(S)或公法人(G)"
}

/// 非法人落位地域规则(由所属法人的行政层级决定)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ParentLocalityRule {
    /// 全国不限(私法人父级、国家级公权机构父级)
    Nationwide,
    /// 同省(省级公权机构父级)
    SameProvince,
    /// 同市(市/镇级公权机构、手动公权机构、法人教育机构父级)
    SameCity,
}

/// 判定父级是否学校/大学(公立 GUN/私立 SUN/教会 JUN 大学 + 公立 GSCH/私立 SFSC/教会 JSCH 学校)。
/// 教育委员会(NED/CEDU)是公权委员会,不是学校,不算分校所属法人。
pub(crate) fn parent_is_education_school(parent_institution_code: &str) -> bool {
    matches!(
        parent_institution_code,
        "GUN" | "SUN" | "JUN" | "GSCH" | "SFSC" | "JSCH"
    )
}

/// 所属法人的地域规则:
/// - 私法人(S)父级 → 全国不限(唯一允许跨省市;S+JY 学校例外,分校与本部同市);
/// - 法人教育机构父级 → 同市;
/// - 公法人(G)父级按机构码行政层级:市级/镇级/无层级(含手动公权机构)→ 同市,
///   省级 → 同省,国家级 → 全国。
pub(crate) fn parent_locality_rule(parent_institution_code: &str) -> ParentLocalityRule {
    if parent_is_education_school(parent_institution_code) {
        return ParentLocalityRule::SameCity;
    }
    if code::institution_code_from_str(parent_institution_code)
        .is_some_and(|c| code::is_private_legal_code(&c))
    {
        return ParentLocalityRule::Nationwide;
    }
    // 公法人(G)按机构码行政层级判级
    match code::institution_code_from_str(parent_institution_code)
        .and_then(|c| code::admin_level(&c))
    {
        Some(AdminLevel::National) => ParentLocalityRule::Nationwide,
        Some(AdminLevel::Province) => ParentLocalityRule::SameProvince,
        // 市级、镇级及一切无层级公权机构 → 同市
        Some(AdminLevel::City) | Some(AdminLevel::Town) | None => ParentLocalityRule::SameCity,
    }
}

/// 校验非法人落位省市是否符合所属法人的地域规则,违规时返回提示文案。
pub(crate) fn locality_violation(
    rule: ParentLocalityRule,
    parent_province: &str,
    parent_city: &str,
    f_province: &str,
    f_city: &str,
) -> Option<&'static str> {
    match rule {
        ParentLocalityRule::Nationwide => None,
        ParentLocalityRule::SameProvince => {
            if parent_province == f_province {
                None
            } else {
                Some("省级公权机构所属的非法人只能落位本省")
            }
        }
        ParentLocalityRule::SameCity => {
            if parent_province == f_province && parent_city == f_city {
                None
            } else {
                Some("该所属法人的非法人只能落位同一市")
            }
        }
    }
}

/// 非法人(UNIN)是通用从属机构,可挂任意法人父级(S/G):挂学校即分校、挂政府即下属、
/// 挂公司即分支。父级合法性由 [`can_attach_to_parent`] 校验,机构码不做交叉约束。
pub(crate) fn code_consistency_violation(
    _f_institution_code: &str,
    _parent_institution_code: &str,
) -> Option<&'static str> {
    None
}

/// 非法人盈利属性附属于所属法人:公法人父级恒非盈利(0),私法人父级继承其 p1。
pub(crate) fn inherited_p1(parent_institution_code: &str, parent_p1: &str) -> String {
    if code::institution_code_from_str(parent_institution_code)
        .is_some_and(|c| code::is_public_legal_code(&c))
    {
        "0".to_string()
    } else {
        parent_p1.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn education_school_parent_requires_same_city() {
        // 公立学校(GSCH)和私立学校(SFSC)父级都锁同市(分校与本部同市)
        assert_eq!(parent_locality_rule("GSCH"), ParentLocalityRule::SameCity);
        assert_eq!(parent_locality_rule("SFSC"), ParentLocalityRule::SameCity);
    }

    #[test]
    fn private_parent_is_nationwide() {
        assert_eq!(parent_locality_rule("SFGQ"), ParentLocalityRule::Nationwide);
    }

    #[test]
    fn gov_parent_locality_follows_institution_code_level() {
        // 市级机构码 → 同市
        assert_eq!(parent_locality_rule("CGOV"), ParentLocalityRule::SameCity);
        // 镇级机构码 → 同市
        assert_eq!(parent_locality_rule("TGOV"), ParentLocalityRule::SameCity);
        // 监管本体(公民教育委员会 CEDU)是市级公权机构,不是学校
        assert_eq!(parent_locality_rule("CEDU"), ParentLocalityRule::SameCity);
        // 省级机构码 → 同省
        assert_eq!(
            parent_locality_rule("PGV"),
            ParentLocalityRule::SameProvince
        );
        // 国家级机构码 → 全国
        for code in ["NLG", "MFA", "FRG"] {
            assert_eq!(parent_locality_rule(code), ParentLocalityRule::Nationwide);
        }
    }

    #[test]
    fn locality_violation_checks_province_and_city() {
        assert!(locality_violation(
            ParentLocalityRule::Nationwide,
            "广东",
            "广州",
            "安徽",
            "合肥"
        )
        .is_none());
        assert!(locality_violation(
            ParentLocalityRule::SameProvince,
            "广东",
            "广州",
            "广东",
            "深圳"
        )
        .is_none());
        assert!(locality_violation(
            ParentLocalityRule::SameProvince,
            "广东",
            "广州",
            "安徽",
            "合肥"
        )
        .is_some());
        assert!(
            locality_violation(ParentLocalityRule::SameCity, "广东", "广州", "广东", "广州")
                .is_none()
        );
        assert!(
            locality_violation(ParentLocalityRule::SameCity, "广东", "广州", "广东", "深圳")
                .is_some()
        );
    }

    #[test]
    fn unincorporated_org_attaches_to_any_legal_parent() {
        // UNIN 通用从属:挂学校/政府/公司都不报机构码错(父级合法性另由 can_attach_to_parent 校验)
        assert!(code_consistency_violation("UNIN", "GSCH").is_none());
        assert!(code_consistency_violation("UNIN", "CGOV").is_none());
        assert!(code_consistency_violation("UNIN", "SFGQ").is_none());
    }

    #[test]
    fn schools_recognized_committees_are_not() {
        // 学校/大学码 → 学校;教育委员会(CEDU)不是学校
        assert!(parent_is_education_school("GSCH"));
        assert!(parent_is_education_school("SFSC"));
        assert!(parent_is_education_school("GUN"));
        assert!(!parent_is_education_school("CEDU"));
    }

    #[test]
    fn p1_inherits_from_parent() {
        // 公法人父级(PGV)恒非盈利;私法人父级(SFGQ)继承其 p1。
        assert_eq!(inherited_p1("PGV", "0"), "0");
        assert_eq!(inherited_p1("PGV", "1"), "0"); // 公法人恒非盈利,容错
        assert_eq!(inherited_p1("SFGQ", "1"), "1");
        assert_eq!(inherited_p1("SFGQ", "0"), "0");
    }
}
