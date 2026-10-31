//! 原所属模块回归，断言与输入保持。
fn capabilities_for(code: &str) -> super::CapabilitySet {
    super::capabilities_for(&crate::modules::test_support::fixture_catalog(), code)
}

#[test]
fn tier1_registry_keeps_subordinate_registry_business_superset() {
    let federal = capabilities_for("FRG");
    let city = capabilities_for("CREG");

    // FRG 是 CREG 的省级上游,注册局工作台业务能力必须覆盖 CREG。
    assert!(federal.can_view_citizens && city.can_view_citizens);
    assert!(federal.can_view_institutions && city.can_view_institutions);
    assert!(federal.can_view_private && city.can_view_private);
    assert!(federal.can_view_education && city.can_view_education);
    assert!(federal.can_manage_institutions && city.can_manage_institutions);
    assert!(federal.can_register_institutions && city.can_register_institutions);
    assert!(federal.can_business_write && city.can_business_write);
    assert!(federal.view_city_registry && city.view_city_registry);
}

#[test]
fn subordinate_registry_can_read_federal_registry_without_registry_crud_flags() {
    let federal = capabilities_for("FRG");
    let city = capabilities_for("CREG");

    // CREG 必须能进入联邦注册局入口只读本省管理员;注册局维护类写权只留给 FRG。
    assert!(city.view_federal_registry);
    assert!(city.view_federal_admins);
    assert!(!city.manage_city_admins);
    assert!(federal.view_federal_registry);
    assert!(federal.view_federal_admins);
    assert!(federal.manage_city_admins);
}

#[test]
fn national_judicial_yuan_can_view_own_admins_only() {
    let judicial = capabilities_for("NJD");

    // NJD 可进入司法院工作台,但本期只给本机构管理员只读页,不获得注册局业务能力。
    assert!(judicial.view_own_admins);
    assert!(!judicial.can_view_citizens);
    assert!(!judicial.can_view_institutions);
    assert!(!judicial.view_city_registry);
    assert!(!judicial.view_federal_registry);
}

#[test]
fn legislative_institutions_get_role_based_legislation_capabilities() {
    // 发起机构（国家众议会）：发起 + 代表机构表决；同时保留本机构管理员只读位。
    let house = capabilities_for("NRP");
    assert!(house.can_view_legislation);
    assert!(house.can_propose_legislation);
    assert!(house.cast_representative_vote);
    assert!(house.view_own_admins);

    // 参议会:只表决,无发起权(权力分离硬约束)。
    let senate = capabilities_for("NSN");
    assert!(senate.cast_representative_vote);
    assert!(!senate.can_propose_legislation);

    // 市教委会：只提案，不参加代表机构表决（由市立法会表决）。
    let city_education = capabilities_for("CEDU");
    assert!(city_education.can_propose_legislation);
    assert!(!city_education.cast_representative_vote);

    // 本轮任免/预算/签署位均未接入。
    assert!(!house.can_propose_personnel);
    assert!(!house.can_propose_budget);
    assert!(!house.can_sign_legislation);
}

#[test]
fn ownerless_cases_exercise_real_base_capabilities() {
    let empty = crate::modules::ModuleCatalog::new(None);
    let fixture = crate::modules::test_support::fixture_catalog();
    let golden: serde_json::Value =
        serde_json::json!({"capabilityCases": super::super::test_support::fixture_cases()});
    for case in golden["capabilityCases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|case| case["owner"].is_null())
    {
        let code = case["code"].as_str().unwrap();
        // 无所属模块的夹具不得遮住真实base_capabilities分支。
        assert!(
            fixture.capabilities(code).is_none(),
            "{code} must not be injected by fixture"
        );
        assert!(fixture.workspace(code).is_none());
        let expected = serde_json::to_value(super::EMPTY).unwrap();
        assert_eq!(
            serde_json::to_value(super::capabilities_for(&empty, code)).unwrap(),
            expected,
            "{code}"
        );
        assert_eq!(
            serde_json::to_value(super::capabilities_for(&fixture, code)).unwrap(),
            expected,
            "{code}"
        );
    }
    let ordinary = super::capabilities_for(&empty, "SFGQ");
    assert!(ordinary.view_own_admins);
    assert!(!ordinary.can_business_write && !ordinary.can_view_citizens);
}
