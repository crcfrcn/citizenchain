//! 原所属模块回归，断言与输入保持。
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
        locality_violation(ParentLocalityRule::SameCity, "广东", "广州", "广东", "广州").is_none()
    );
    assert!(
        locality_violation(ParentLocalityRule::SameCity, "广东", "广州", "广东", "深圳").is_some()
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
