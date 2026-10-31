//! 原所属模块回归，断言与输入保持。
use super::*;

#[test]
fn scope_precheck_reads_canonical_admin_account_keys() {
    let ctx = AdminAuthContext {
        account_id: format!("0x{}", "11".repeat(32)),
        institution_cid_number: "test-institution".into(),
        institution_code: "CREG".into(),
        admin_level: None,
        family_name: "测".into(),
        given_name: "试".into(),
        scope_province_name: Some("贵州省".into()),
        scope_city_name: Some("绥阳市".into()),
        scope_town_name: None,
        cid_short_name: None,
    };
    let a = format!("0x{}", "22".repeat(32));
    let b = format!("0x{}", "33".repeat(32));
    let input = serde_json::json!({"provinceName":"贵州省", "cityName":"绥阳市",
            "admins":[{"accountId":a},{"accountId":b}]});
    assert!(precheck_institution_create_scope(&ctx, &input).is_ok());
    let legacy = serde_json::json!({"provinceName":"贵州省", "cityName":"绥阳市",
            "admins":[{"account_id":a},{"account_id":b}]});
    assert!(precheck_institution_create_scope(&ctx, &legacy).is_err());
    let duplicate = serde_json::json!({"provinceName":"贵州省", "cityName":"绥阳市",
            "admins":[{"accountId":a},{"accountId":a}]});
    assert!(precheck_institution_create_scope(&ctx, &duplicate).is_err());
}
