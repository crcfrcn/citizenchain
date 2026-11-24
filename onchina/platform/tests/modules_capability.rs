//! 验证各类机构工作台能力和注册局上下级权限范围。

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
fn subordinate_registry_can_browse_federal_institution() {
    let federal = capabilities_for("FRG");
    let city = capabilities_for("CREG");

    // CREG 保留正常机构浏览；具体写权仍由对应业务和岗位授权核验。
    assert!(city.view_federal_registry);
    assert!(federal.view_federal_registry);
}

#[test]
fn national_judicial_yuan_has_no_registry_business() {
    let judicial = capabilities_for("NJD");

    // NJD 可进入司法院工作台，不获得注册局业务能力。
    assert!(!judicial.can_view_citizens);
    assert!(!judicial.can_view_institutions);
    assert!(!judicial.view_city_registry);
    assert!(!judicial.view_federal_registry);
}

#[test]
fn legislative_institutions_get_role_based_legislation_capabilities() {
    // 发起机构（国家众议会）：发起 + 代表机构表决。
    let house = capabilities_for("NRP");
    assert!(house.can_view_legislation);
    assert!(house.can_propose_legislation);
    assert!(house.cast_representative_vote);

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
    assert!(!ordinary.can_business_write && !ordinary.can_view_citizens);
}


#[test]
fn institution_workspace_has_profile_without_personnel_list_capabilities() {
    for code in ["NRC", "PRC", "PRB", "NJD", "CGOV", "CEDU", "SFGY", "UNIN"] {
        let capabilities = capabilities_for(code);
        let serialized = serde_json::to_value(capabilities).unwrap();
        for removed in ["canViewOwnAdmins", "canViewFederalRegistryAdmins", "canViewCityRegistryAdmins", "canCrudCityRegistryAdmins"] {
            assert!(serialized.get(removed).is_none(), "{code}: {removed}");
        }
        let sections = crate::workspace::manifest::own_institution_sections(capabilities);
        assert!(sections[0].workspace_actions.is_empty());
        assert_eq!(sections[1].workspace_actions.len(), 1);
        assert_eq!(sections[1].workspace_actions[0].action, "view_institution_profile");
        assert!(sections[1].workspace_actions[0].enabled);
    }
}
