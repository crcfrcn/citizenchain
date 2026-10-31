//! 原所属模块回归，断言与输入保持。
use super::*;

#[test]
fn national_sees_every_province() {
    let s = VisibleScope::national();
    assert!(s.includes_province("安徽省"));
    assert!(s.includes_province("广东省"));
    assert!(s.includes_city("合肥市"));
    assert!(s.includes_town("任意镇"));
}

#[test]
fn province_limited_to_own_province_all_cities() {
    let s = VisibleScope::province("安徽省".to_string());
    assert!(s.includes_province("安徽省"));
    assert!(!s.includes_province("广东省"));
    assert!(s.includes_city("合肥市"));
    assert!(s.includes_town("任意镇"));
    assert!(s.skip_province_list);
    assert!(!s.skip_city_list);
}

#[test]
fn city_limited_to_own_city_all_towns() {
    let s = VisibleScope::city("安徽省".to_string(), "合肥市".to_string());
    assert!(s.includes_province("安徽省"));
    assert!(s.includes_city("合肥市"));
    assert!(!s.includes_city("芜湖市"));
    assert!(s.includes_town("任意镇"));
    assert!(s.skip_city_list);
    assert!(!s.skip_town_list);
}

#[test]
fn town_limited_to_own_town() {
    let s = VisibleScope::town(
        "安徽省".to_string(),
        "合肥市".to_string(),
        "三十岗镇".to_string(),
    );
    assert!(s.includes_province("安徽省"));
    assert!(s.includes_city("合肥市"));
    assert!(s.includes_town("三十岗镇"));
    assert!(!s.includes_town("董铺镇"));
    // 无镇维度记录(town 空,如手动创建机构)对镇级管理员仍可见,不被误锁。
    assert!(s.includes_town(""));
    assert!(s.skip_town_list);
}

#[test]
fn empty_scope_matches_nothing() {
    let s = VisibleScope::empty();
    assert!(!s.includes_province("安徽省"));
    assert!(!s.can_write);
}

fn ctx(
    institution_code: &str,
    admin_level: Option<&str>,
    province: Option<&str>,
    city: Option<&str>,
    town: Option<&str>,
) -> AdminAuthContext {
    AdminAuthContext {
        account_id: "0x1111111111111111111111111111111111111111111111111111111111111111"
            .to_string(),
        institution_cid_number: "LN001-FRG0G-000000001-2026".to_string(),
        institution_code: institution_code.to_string(),
        admin_level: admin_level.map(str::to_string),
        family_name: "管理".to_string(),
        given_name: "员".to_string(),
        scope_province_name: province.map(str::to_string),
        scope_city_name: city.map(str::to_string),
        scope_town_name: town.map(str::to_string),
        cid_short_name: None,
    }
}

#[test]
fn frg_is_province_scoped_despite_national_level() {
    // 安全关键:FRG 码 admin_level 为 NATIONAL,但必须特判为省级,绝不给全国范围。
    let s = get_visible_scope(&ctx("FRG", Some("NATIONAL"), Some("安徽省"), None, None));
    assert!(!s.nationwide);
    assert!(s.includes_province("安徽省"));
    assert!(!s.includes_province("广东省"));
}

#[test]
fn frg_without_province_fails_closed() {
    let s = get_visible_scope(&ctx("FRG", Some("NATIONAL"), None, None, None));
    assert!(!s.includes_province("安徽省"));
}

#[test]
fn ordinary_institutions_are_isolated_by_cid_even_in_same_city() {
    for code in ["MFA", "PGV", "CGOV", "TGOV", "SFLP", "UNIN"] {
        let admin = ctx(code, Some("CITY"), Some("安徽省"), Some("合肥市"), None);
        let scope = get_visible_scope(&admin);
        assert!(!scope.can_write);
        assert!(!scope.includes_province("安徽省"));
        assert!(scope.includes_institution(&admin.institution_cid_number, "", "", ""));
        assert!(!scope.includes_institution("another-institution", "安徽省", "合肥市", ""));
    }
    assert!(!VisibleScope::own("").includes_institution("", "", "", ""));
}
