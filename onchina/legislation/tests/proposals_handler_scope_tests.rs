//! 原所属模块回归，断言与输入保持。
use super::*;

fn cid(institution: &str, province: &str, city: &str) -> String {
    primitives::cid::generator::generate_cid_number(
        primitives::cid::generator::GenerateCidNumberInput {
            public_key: "0x1234",
            p1: "0",
            province_code: province,
            province_name: "测试省",
            city_code: city,
            city_name: "测试市",
            year: "2026",
            institution,
        },
    )
    .unwrap()
}

fn context(level: &str, institution: &str, province: &str, city: &str) -> AdminAuthContext {
    AdminAuthContext {
        account_id: format!("0x{}", "11".repeat(32)),
        institution_cid_number: cid(institution, province, city),
        institution_code: institution.into(),
        admin_level: Some(level.into()),
        family_name: String::new(),
        given_name: String::new(),
        scope_province_name: Some("广东省".into()),
        scope_city_name: None,
        scope_town_name: None,
        cid_short_name: None,
    }
}

fn city_context() -> AdminAuthContext {
    let mut ctx = context("CITY", "CSLF", "GD", "001");
    ctx.scope_city_name = Some(
        onchina::codes::china::provinces()
            .iter()
            .find(|province| province.province_code == "GD")
            .unwrap()
            .cities
            .iter()
            .find(|city| city.city_code == "001")
            .unwrap()
            .city_name
            .to_string(),
    );
    ctx
}

fn input(action: LawActionInput, tier: u8) -> ProposeLawInput {
    ProposeLawInput {
        law_action: action,
        proposer_role_code: "REPRESENTATIVE".into(),
        tier,
        vote_type: 2,
        title: "法律".into(),
        title_en: None,
        chapters: Vec::new(),
        effective_at: 1000,
        law_id: Some(7),
    }
}

fn law(tier: u8, institution: &str, province: &str, city: &str) -> chain_read::OnChainLaw {
    chain_read::OnChainLaw {
        law_id: 7,
        tier,
        region: primitives::genesis::LawRegion::from_codes(
            tier,
            province.as_bytes(),
            city.as_bytes(),
        )
        .unwrap(),
        houses: vec![cid(institution, province, city).into_bytes()],
        effective_version: Some(1),
        latest_version: 1,
        pending_version: None,
        status: 1,
    }
}

#[test]
fn province_scope_keeps_verified_session_scope_and_original_code() {
    let ctx = context("PROVINCE", "PRP", "GD", "002");
    let scope = law_scope(&ctx, 2).unwrap();
    assert_eq!(scope.province_code, "GD");
    assert!(scope.city_code.is_empty());
    assert!(law_scope(&ctx, 3).is_err());
}

#[test]
fn fixed_province_letters_are_preserved_without_numeric_conversion() {
    for province in primitives::cid::code::PROVINCE_CODE_INFOS {
        let code = primitives::cid::code::province_code_text(&province.province_code).unwrap();
        assert_eq!(code.len(), 2);
        assert!(code.bytes().all(|byte| byte.is_ascii_uppercase()));
        let mut ctx = context("PROVINCE", "PRP", code, "001");
        ctx.scope_province_name = Some(province.province_name.into());
        let mut request = input(LawActionInput::Enact, 2);
        request.law_id = None;
        let scope = proposal_scope(&ctx, &request, None).unwrap();
        let region =
            primitives::genesis::LawRegion::from_cid(2, ctx.institution_cid_number.as_bytes())
                .unwrap();
        assert_eq!(region.province_code.as_slice(), code.as_bytes());
        assert_eq!(region.city_number, 0);
        assert_eq!(scope.province_code, code);
        assert!(scope.city_code.is_empty());
    }
}

#[test]
fn new_law_rejects_cid_region_different_from_session() {
    let mut ctx = context("PROVINCE", "PRP", "GZ", "001");
    let request = input(LawActionInput::Enact, 2);
    assert!(proposal_scope(&ctx, &request, None).is_err());
    ctx.institution_cid_number = "invalid-cid".into();
    assert!(proposal_scope(&ctx, &request, None).is_err());
}

#[test]
fn missing_session_scope_and_wrong_tier_are_rejected() {
    let mut ctx = context("PROVINCE", "PRP", "GD", "001");
    ctx.scope_province_name = None;
    assert!(law_scope(&ctx, 2).is_err());
    ctx.scope_province_name = Some("广东省".into());
    ctx.admin_level = Some("CITY".into());
    assert!(law_scope(&ctx, 3).is_err());
    assert!(law_scope(&ctx, 2).is_err());
}

#[test]
fn amend_and_repeal_validate_target_id_and_original_cid_scope() {
    let ctx = context("PROVINCE", "PRP", "GD", "001");
    let target = law(2, "PRP", "GD", "002");
    for action in [LawActionInput::Amend, LawActionInput::Repeal] {
        let request = input(action, 2);
        let scope = proposal_scope(&ctx, &request, Some(&target)).unwrap();
        assert_eq!(scope.province_code, "GD");
        assert_eq!(request.law_id, Some(7));
    }
}

#[test]
fn target_law_requires_same_id_tier_province_and_city() {
    let ctx = city_context();
    for action in [LawActionInput::Amend, LawActionInput::Repeal] {
        let request = input(action, 3);
        let same_city = law(3, "CLEG", "GD", "001");
        assert!(proposal_scope(&ctx, &request, Some(&same_city)).is_ok());
        let mut target = law(3, "CLEG", "GZ", "001");
        assert!(proposal_scope(&ctx, &request, Some(&target)).is_err());
        target = law(3, "CLEG", "GD", "002");
        assert!(proposal_scope(&ctx, &request, Some(&target)).is_err());
        target = law(2, "PRP", "GD", "001");
        assert!(proposal_scope(&ctx, &request, Some(&target)).is_err());
        target = law(3, "CLEG", "GD", "001");
        target.law_id = 8;
        assert!(proposal_scope(&ctx, &request, Some(&target)).is_err());
    }
}

#[test]
fn amend_and_repeal_require_existing_target_and_id() {
    let ctx = context("PROVINCE", "PRP", "GD", "001");
    for action in [LawActionInput::Amend, LawActionInput::Repeal] {
        let mut request = input(action, 2);
        assert!(proposal_scope(&ctx, &request, None).is_err());
        request.law_id = None;
        assert_eq!(
            proposal_scope(&ctx, &request, None).unwrap_err(),
            service::LegislationError::MissingLawId.code()
        );
    }
}

#[test]
fn new_law_derives_original_cid_region_and_keeps_cn() {
    let national = context("NATIONAL", "NRP", "ZS", "001");
    let mut request = input(LawActionInput::Enact, 1);
    request.law_id = None;
    let scope = proposal_scope(&national, &request, None).unwrap();
    assert_eq!(
        primitives::genesis::LawRegion::from_cid(1, national.institution_cid_number.as_bytes())
            .unwrap(),
        primitives::genesis::LawRegion::NATIONAL
    );
    assert!(scope.province_code.is_empty() && scope.city_code.is_empty());
    assert_eq!(primitives::cid::code::COUNTRY_CN, *b"CN");
    assert!(law_scope(&national, 0).is_ok());
    assert!(law_scope(&national, 2).is_err());
    let city = city_context();
    let mut request = input(LawActionInput::Enact, 3);
    request.law_id = None;
    let scope = proposal_scope(&city, &request, None).unwrap();
    assert_eq!(
        primitives::genesis::LawRegion::from_cid(3, city.institution_cid_number.as_bytes())
            .unwrap()
            .city_number,
        1
    );
    assert_eq!(scope.province_code, "GD");
    assert_eq!(scope.city_code, "001");
}
