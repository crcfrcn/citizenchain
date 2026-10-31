//! 原所属模块回归，断言与输入保持。
use super::*;
use crate::law::model::{LawChapter, LawSection};
use codec::Decode;

fn fixture_cid(code: &[u8; 4]) -> String {
    primitives::cid::generator::generate_cid_number(
        primitives::cid::generator::GenerateCidNumberInput {
            public_key: "0x1234",
            p1: "0",
            province_code: "GD",
            province_name: "广东省",
            city_code: "001",
            city_name: "测试市",
            year: "2026",
            institution: super::super::model::institution_code_text(code).as_str(),
        },
    )
    .expect("valid fixture CID")
}

fn fixture_resolver(code: &[u8; 4]) -> Option<String> {
    Some(fixture_cid(code))
}

fn enact_input(tier: u8, vote_type: u8) -> ProposeLawInput {
    ProposeLawInput {
        law_action: LawActionInput::Enact,
        proposer_role_code: "REPRESENTATIVE".to_string(),
        tier,
        vote_type,
        title: "道路交通安全法".to_string(),
        title_en: Some("Road Traffic Safety Law".to_string()),
        chapters: vec![LawChapter {
            number: 1,
            title: "总则".to_string(),
            title_en: None,
            sections: vec![LawSection {
                number: 1,
                title: "定义".to_string(),
                title_en: None,
                articles: vec![],
            }],
        }],
        effective_at: 1000,
        law_id: None,
    }
}

#[test]
fn enact_national_builds_call_with_original_actor_and_houses() {
    let input = enact_input(1, 2);
    let actor = fixture_cid(b"NRP\0");
    let call = build_propose_law_call(&input, *b"NRP\0", &actor, fixture_resolver).unwrap();
    assert_eq!(&call.call_data[..2], &[25, 0]);
    assert_eq!(call.action, 0x1900);
    let mut bytes = &call.call_data[2..];
    assert_eq!(u8::decode(&mut bytes).unwrap(), 1);
    assert_eq!(
        primitives::genesis::LawRegion::decode(&mut bytes).unwrap(),
        primitives::genesis::LawRegion::NATIONAL
    );
    let houses = Vec::<Vec<u8>>::decode(&mut bytes).unwrap();
    assert_eq!(
        houses,
        vec![
            actor.as_bytes().to_vec(),
            fixture_cid(b"NSN\0").into_bytes()
        ]
    );
    assert_eq!(Vec::<u8>::decode(&mut bytes).unwrap(), actor.as_bytes());
}

#[test]
fn bound_actor_is_reused_without_institution_code_lookup() {
    let input = enact_input(1, 2);
    let actor = fixture_cid(b"NRP\0");
    let resolver = |code: &[u8; 4]| {
        assert_ne!(code, b"NRP\0", "发起机构不得按机构码重新查询");
        fixture_resolver(code)
    };
    assert!(build_propose_law_call(&input, *b"NRP\0", &actor, resolver).is_ok());
}

#[test]
fn municipal_education_keeps_actor_separate_from_voting_house() {
    let input = enact_input(3, 1);
    let actor = fixture_cid(b"CEDU");
    let call = build_propose_law_call(&input, *b"CEDU", &actor, fixture_resolver).unwrap();
    let mut bytes = &call.call_data[2..];
    let _ = u8::decode(&mut bytes).unwrap();
    assert_eq!(
        primitives::genesis::LawRegion::decode(&mut bytes).unwrap(),
        primitives::genesis::LawRegion::from_codes(3, b"GD", b"001").unwrap()
    );
    assert_eq!(
        Vec::<Vec<u8>>::decode(&mut bytes).unwrap(),
        vec![fixture_cid(b"CLEG").into_bytes()]
    );
    assert_eq!(Vec::<u8>::decode(&mut bytes).unwrap(), actor.as_bytes());
}

#[test]
fn malformed_or_wrong_actor_institution_is_rejected() {
    let input = enact_input(1, 2);
    for actor in ["invalid-cid".to_string(), fixture_cid(b"NSN\0")] {
        assert!(matches!(
            build_propose_law_call(&input, *b"NRP\0", &actor, fixture_resolver),
            Err(LegislationError::InstitutionCidUnresolved)
        ));
    }
}

#[test]
fn wrong_resolved_route_institution_is_rejected() {
    let input = enact_input(1, 2);
    let actor = fixture_cid(b"NRP\0");
    assert!(matches!(
        build_propose_law_call(&input, *b"NRP\0", &actor, |_| Some(actor.clone())),
        Err(LegislationError::InstitutionCidUnresolved)
    ));
}

#[test]
fn amend_and_repeal_keep_law_id_and_actor_encoding() {
    let actor = fixture_cid(b"NRP\0");
    for (action, call_index) in [(LawActionInput::Amend, 1), (LawActionInput::Repeal, 2)] {
        let mut input = enact_input(1, 2);
        input.law_action = action;
        input.law_id = Some(42);
        let call = build_propose_law_call(&input, *b"NRP\0", &actor, fixture_resolver).unwrap();
        assert_eq!(&call.call_data[..2], &[25, call_index]);
        let mut bytes = &call.call_data[2..];
        assert_eq!(u64::decode(&mut bytes).unwrap(), 42);
        assert_eq!(Vec::<u8>::decode(&mut bytes).unwrap(), actor.as_bytes());
    }
}

#[test]
fn amend_without_law_id_is_rejected() {
    let mut input = enact_input(1, 2);
    input.law_action = LawActionInput::Amend;
    assert!(matches!(
        build_propose_law_call(&input, *b"NRP\0", &fixture_cid(b"NRP\0"), fixture_resolver),
        Err(LegislationError::MissingLawId)
    ));
}

#[test]
fn unresolved_institution_cid_fails_closed() {
    let input = enact_input(1, 2);
    assert!(matches!(
        build_propose_law_call(&input, *b"NRP\0", &fixture_cid(b"NRP\0"), |_| None),
        Err(LegislationError::InstitutionCidUnresolved)
    ));
}

#[test]
fn provincial_education_has_no_routing() {
    let input = enact_input(2, 1);
    assert!(matches!(
        build_propose_law_call(&input, *b"PRP\0", &fixture_cid(b"PRP\0"), fixture_resolver),
        Err(LegislationError::UnknownRouting)
    ));
}

#[test]
fn enact_province_keeps_letters_in_four_byte_slot() {
    let input = enact_input(2, 2);
    let actor = fixture_cid(b"PRP\0");
    let call = build_propose_law_call(&input, *b"PRP\0", &actor, fixture_resolver).unwrap();
    assert_eq!(&call.call_data[3..7], &[b'G', b'D', 0, 0]);
}

#[test]
fn representative_vote_call_targets_legislation_vote_pallet() {
    let call = build_representative_vote_call(42, "REPRESENTATIVE", true);
    assert_eq!(&call.call_data[..2], &[26, 1]);
}
