//! 原所属模块回归，断言与输入保持。
use super::*;

#[test]
fn proposal_request_deserializes_camel_case() {
    let json = r#"{
            "lawAction":"enact","tier":1,"voteType":2,
            "proposerRoleCode":"REPRESENTATIVE",
            "title":"道路交通安全法","titleEn":"Road Traffic Safety Law",
            "chapters":[{"number":1,"title":"总则","sections":[]}],
            "effectiveAt":1000,"lawId":null
        }"#;
    let input: ProposeLawInput = serde_json::from_str(json).expect("parse");
    assert_eq!(input.law_action, LawActionInput::Enact);
    assert_eq!(input.tier, 1);
    assert_eq!(input.chapters.len(), 1);
    assert_eq!(input.chapters[0].title, "总则");
}

#[test]
fn region_view_serializes_original_letters_without_numeric_province() {
    let region = primitives::genesis::LawRegion::from_codes(3, b"GD", b"001").expect("city region");
    assert_eq!(
        serde_json::to_value(LawRegionView::from(region)).expect("serialize"),
        serde_json::json!({"provinceCode":"GD", "cityNumber":1})
    );
    assert_eq!(
        serde_json::to_value(LawRegionView::from(
            primitives::genesis::LawRegion::NATIONAL
        ))
        .expect("serialize"),
        serde_json::json!({"provinceCode":"", "cityNumber":0})
    );
}

#[test]
fn chapters_convert_to_encoder_args_preserving_text() {
    let chapters = vec![LawChapter {
        number: 1,
        title: "总则".to_string(),
        title_en: Some("General".to_string()),
        sections: vec![LawSection {
            number: 1,
            title: "定义".to_string(),
            title_en: None,
            articles: vec![LawArticle {
                number: 1,
                title: "第一条".to_string(),
                title_en: None,
                body: "正文".to_string(),
                body_en: None,
                clauses: vec![LawClause {
                    number: 1,
                    text: "第一款".to_string(),
                    text_en: None,
                }],
            }],
        }],
    }];
    let args = to_chapter_args(&chapters);
    assert_eq!(args.len(), 1);
    assert_eq!(args[0].title, "总则".as_bytes());
    assert_eq!(args[0].title_en.as_deref(), Some("General".as_bytes()));
    assert_eq!(
        args[0].sections[0].articles[0].clauses[0].text,
        "第一款".as_bytes()
    );
    assert!(args[0].sections[0].title_en.is_none());
}

#[test]
fn chapter_args_round_trip_back_to_display_chapters() {
    // to_chapter_args → to_law_chapters 往返:展示文本与层级结构无损还原。
    let original = vec![LawChapter {
        number: 1,
        title: "总则".to_string(),
        title_en: Some("General".to_string()),
        sections: vec![LawSection {
            number: 1,
            title: "定义".to_string(),
            title_en: None,
            articles: vec![LawArticle {
                number: 1,
                title: "第一条".to_string(),
                title_en: None,
                body: "正文".to_string(),
                body_en: None,
                clauses: vec![LawClause {
                    number: 1,
                    text: "第一款".to_string(),
                    text_en: None,
                }],
            }],
        }],
    }];
    let restored = to_law_chapters(&to_chapter_args(&original));
    assert_eq!(restored[0].title, "总则");
    assert_eq!(restored[0].title_en.as_deref(), Some("General"));
    assert_eq!(restored[0].sections[0].articles[0].body, "正文");
    assert_eq!(
        restored[0].sections[0].articles[0].clauses[0].text,
        "第一款"
    );
}

#[test]
fn institution_code_text_trims_trailing_nul() {
    assert_eq!(institution_code_text(b"NRP\0"), "NRP");
    assert_eq!(institution_code_text(b"CLEG"), "CLEG");
    let house = house_ref(b"LN001-NSN0G-000000001-2026");
    assert_eq!(house.cid_number, "LN001-NSN0G-000000001-2026");
}
