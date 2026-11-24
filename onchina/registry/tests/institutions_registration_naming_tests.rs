//! 验证机构登记范围预检使用机构辖区且拒绝退役名单字段。

use super::*;

#[test]
fn scope_precheck_keeps_location_and_rejects_retired_admin_fields() {
    let ctx = AdminAuthContext {
        binding_revision: "1".into(),
        person_cid: onchina::testing::person_cid(),
        role_code: "REGISTRAR".into(),
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
    let input = serde_json::json!({"provinceName":"贵州省","cityName":"绥阳市"});
    assert!(precheck_institution_create_scope(&ctx, &input).is_ok());
    let outside = serde_json::json!({"provinceName":"广东省","cityName":"绥阳市"});
    assert!(precheck_institution_create_scope(&ctx, &outside).is_err());
    let retired = serde_json::json!({"provinceName":"贵州省","cityName":"绥阳市","admins":[{"accountId":a},{"accountId":b}]});
    assert!(precheck_institution_create_scope(&ctx, &retired).is_err());
    let legacy = serde_json::json!({"provinceName":"贵州省", "cityName":"绥阳市",
            "admins":[{"account_id":a},{"account_id":b}]});
    assert!(precheck_institution_create_scope(&ctx, &legacy).is_err());
    let duplicate = serde_json::json!({"provinceName":"贵州省", "cityName":"绥阳市",
            "admins":[{"accountId":a},{"accountId":a}]});
    assert!(precheck_institution_create_scope(&ctx, &duplicate).is_err());
}
