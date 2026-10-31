//! 原所属模块回归，断言与输入保持。
use super::super::chain_propose::{ArticleArg, ClauseArg, SectionArg};
use super::*;
use codec::Encode;
use legislation::{LawStatus, Tier, VoteType};

fn sample_chapters() -> Vec<ChapterArg> {
    vec![ChapterArg {
        number: 1,
        title: "总则".as_bytes().to_vec(),
        title_en: Some("General".as_bytes().to_vec()),
        sections: vec![SectionArg {
            number: 1,
            title: "定义".as_bytes().to_vec(),
            title_en: None,
            articles: vec![ArticleArg {
                number: 1,
                title: "第一条".as_bytes().to_vec(),
                title_en: None,
                body: "正文".as_bytes().to_vec(),
                body_en: None,
                clauses: vec![ClauseArg {
                    number: 1,
                    text: "第一款".as_bytes().to_vec(),
                    text_en: None,
                }],
            }],
        }],
    }]
}

/// 范围夹具复用既有 CID 生成规则，只用于法律地域读取回归。
fn scoped_law(tier: u8, province: &str, city: &str) -> OnChainLaw {
    let cid = primitives::cid::generator::generate_cid_number(
        primitives::cid::generator::GenerateCidNumberInput {
            public_key: "0x1234",
            p1: "0",
            province_code: province,
            province_name: "测试省",
            city_code: city,
            city_name: "测试市",
            year: "2026",
            institution: if tier == 2 { "PRP" } else { "CLEG" },
        },
    )
    .unwrap();
    OnChainLaw {
        law_id: 1,
        tier,
        region: LawRegion::from_codes(tier, province.as_bytes(), city.as_bytes()).unwrap(),
        houses: vec![cid.into_bytes()],
        effective_version: Some(1),
        latest_version: 1,
        pending_version: None,
        status: 1,
    }
}

#[test]
fn province_and_local_city_numbers_remain_distinct_and_unmodified() {
    let scope = LawScope {
        tier: 3,
        province_code: "GD".into(),
        city_code: "001".into(),
    };
    assert!(law_matches_scope(&scoped_law(3, "GD", "001"), &scope));
    assert!(!law_matches_scope(&scoped_law(3, "GZ", "001"), &scope));
    assert!(!law_matches_scope(&scoped_law(3, "GD", "002"), &scope));
    assert_eq!(scope.province_code, "GD");
    assert_eq!(scope.city_code, "001");
    let mut malformed = scoped_law(3, "GD", "001");
    malformed.houses = vec![b"invalid-cid".to_vec()];
    assert!(!law_matches_scope(&malformed, &scope));
    let province = LawScope {
        tier: 2,
        province_code: "GD".into(),
        city_code: String::new(),
    };
    let mut law = scoped_law(2, "GD", "002");
    assert!(law_matches_scope(&law, &province));
    law.houses.clear();
    assert!(!law_matches_scope(&law, &province));
}

#[test]
fn all_saved_house_cids_must_match_the_requested_region() {
    let scope = LawScope {
        tier: 3,
        province_code: "GD".into(),
        city_code: "001".into(),
    };
    let mut law = scoped_law(3, "GD", "001");
    law.houses.extend(scoped_law(3, "GZ", "001").houses);
    assert!(!law_matches_scope(&law, &scope));
}

#[test]
fn constitution_scope_keeps_national_legislature_ownership() {
    let nlg = primitives::cid::china::china_lf::CHINA_LF[0]
        .cid_number
        .as_bytes()
        .to_vec();
    let law = OnChainLaw {
        law_id: 0,
        tier: 0,
        region: LawRegion::NATIONAL,
        houses: vec![nlg.clone()],
        effective_version: Some(1),
        latest_version: 1,
        pending_version: None,
        status: 1,
    };
    let scope = LawScope {
        tier: 0,
        province_code: String::new(),
        city_code: String::new(),
    };
    assert!(law_matches_scope(&law, &scope));
    assert_eq!(law.houses, vec![nlg]);
    assert_eq!(primitives::cid::code::COUNTRY_CN, *b"CN");
}

#[test]
fn stored_region_must_agree_with_original_house_cids() {
    let scope = LawScope {
        tier: 3,
        province_code: "GD".into(),
        city_code: "001".into(),
    };
    let mut law = scoped_law(3, "GD", "001");
    law.region = LawRegion::from_codes(3, b"GZ", b"001").unwrap();
    assert!(!law_matches_scope(&law, &scope));
}

/// 用链端现有 `Tier`/`LawStatus` 编码 golden，镜像解码字段保持一致。
#[test]
fn law_decodes_from_runtime_encoded_bytes() {
    let houses = vec![
        b"LN001-NRP0G-000000001-2026".to_vec(),
        b"LN001-NSN0G-000000001-2026".to_vec(),
    ];
    let mut golden = Vec::new();
    golden.extend(7u64.encode());
    golden.extend(Tier::National.encode());
    golden.extend(0u32.encode()); // 合法全国地域固定四零字节
    golden.extend(houses.encode());
    golden.extend(Some(3u32).encode());
    golden.extend(3u32.encode());
    golden.extend(Option::<u32>::None.encode());
    golden.extend(LawStatus::Effective.encode());

    let law = decode_law(&golden).expect("decode Law");
    assert_eq!(law.law_id, 7);
    assert_eq!(law.tier, 1); // National
    assert_eq!(law.region, LawRegion::NATIONAL);
    assert_eq!(law.houses.len(), 2);
    assert_eq!(law.houses[0], b"LN001-NRP0G-000000001-2026");
    assert_eq!(law.effective_version, Some(3));
    assert_eq!(law.latest_version, 3);
    assert_eq!(law.pending_version, None);
    assert_eq!(law.status, 1); // Effective
    let mut trailing = golden.clone();
    trailing.push(0);
    assert!(decode_law(&trailing).is_err());
    let mut invalid = golden.clone();
    invalid[8] = 2; // 省级不能使用全国零地域。
    assert!(decode_law(&invalid).is_err());
    for (province, city) in [(b"GD", b"001"), (b"GZ", b"001")] {
        let region = LawRegion::from_codes(3, province, city).unwrap();
        let mut regional = golden.clone();
        regional[8] = 3;
        regional[9..13].copy_from_slice(&region.encode());
        let law = decode_law(&regional).expect("regional law");
        assert_eq!(law.region, region);
        assert_eq!(law.latest_version, 3);
    }
}

/// LawVersion 镜像解码 + 组装 LawView(章节字节→String、houses→CID、hash→0x hex)。
#[test]
fn law_version_decodes_and_builds_view() {
    let chapters = sample_chapters();
    let mut golden = Vec::new();
    golden.extend(7u64.encode());
    golden.extend(2u32.encode());
    golden.extend("道路交通安全法".as_bytes().to_vec().encode());
    golden.extend(Some("Road".as_bytes().to_vec()).encode());
    golden.extend(chapters.encode());
    golden.extend([9u8; 32].encode());
    golden.extend(VoteType::Major.encode());
    golden.extend(11u64.encode());
    golden.extend(500u64.encode());
    golden.extend(1000u64.encode());

    let version = decode_law_version(&golden).expect("decode LawVersion");
    assert_eq!(version.version, 2);
    assert_eq!(version.vote_type, 2); // Major
    assert_eq!(version.chapters.len(), 1);
    assert_eq!(version.title, "道路交通安全法".as_bytes());

    let label = decode_law_version_label(&{
        let mut bytes = Vec::new();
        "创世版".as_bytes().to_vec().encode_to(&mut bytes);
        Some("Genesis Edition".as_bytes().to_vec()).encode_to(&mut bytes);
        bytes
    })
    .expect("decode LawVersionLabel");
    assert_eq!(label.title, "创世版".as_bytes());

    let law = OnChainLaw {
        law_id: 7,
        tier: 1,
        region: LawRegion::NATIONAL,
        houses: vec![
            b"LN001-NRP0G-000000001-2026".to_vec(),
            b"LN001-NSN0G-000000001-2026".to_vec(),
        ],
        effective_version: Some(2),
        latest_version: 2,
        pending_version: None,
        status: 1,
    };
    let view = build_law_view(&law, &version, Some(&label), &[1]);
    assert_eq!(view.effective_version, Some(2));
    assert_eq!(view.latest_version, 2);
    assert_eq!(view.pending_version, None);
    assert_eq!(view.version_title.as_deref(), Some("创世版"));
    assert_eq!(view.version_title_en.as_deref(), Some("Genesis Edition"));
    assert_eq!(view.title, "道路交通安全法");
    assert_eq!(view.houses.len(), 2);
    assert_eq!(view.houses[0].cid_number, "LN001-NRP0G-000000001-2026");
    assert!(view.immutable_article_numbers.is_empty());
    assert!(view.content_hash.starts_with("0x"));
    assert_eq!(view.chapters[0].title, "总则");
    assert_eq!(
        view.chapters[0].sections[0].articles[0].clauses[0].text,
        "第一款"
    );

    let constitution_law = OnChainLaw { tier: 0, ..law };
    let constitution_view = build_law_view(&constitution_law, &version, None, &[1, 2]);
    assert_eq!(constitution_view.immutable_article_numbers, vec![1, 2]);
    assert!(constitution_view.version_title.is_none());
}
