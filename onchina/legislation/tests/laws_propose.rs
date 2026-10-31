//! 原所属模块回归，断言与输入保持。
use super::*;
use legislation::{Tier, VoteType};

/// 本编码器的 `tier`/`vote_type` 单字节序号必须与链端真实枚举 `.encode()` 逐字节一致。
#[test]
fn tier_and_vote_type_bytes_match_runtime_enums() {
    for (byte, real) in [
        (0u8, Tier::Constitution),
        (1, Tier::National),
        (2, Tier::Provincial),
        (3, Tier::Municipal),
    ] {
        assert_eq!(vec![byte], real.encode(), "Tier 序号漂移");
    }
    for (byte, real) in [
        (0u8, VoteType::Regular),
        (1, VoteType::RegularEducation),
        (2, VoteType::Major),
        (3, VoteType::MajorEducation),
        (4, VoteType::Special),
    ] {
        assert_eq!(vec![byte], real.encode(), "VoteType 序号漂移");
    }
}

fn cid(suffix: &str) -> Vec<u8> {
    format!("LN001-{suffix}0G-000000001-2026").into_bytes()
}

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

#[test]
fn regional_call_keeps_literal_province_and_existing_house_offset() {
    let houses = vec![cid("NRP")];
    for (tier, region, raw) in [
        (
            2,
            LawRegion::from_codes(2, b"GD", b"001").unwrap(),
            [b'G', b'D', 0, 0],
        ),
        (
            3,
            LawRegion::from_codes(3, b"GD", b"001").unwrap(),
            [b'G', b'D', 1, 0],
        ),
        (
            3,
            LawRegion::from_codes(3, b"GZ", b"001").unwrap(),
            [b'G', b'Z', 1, 0],
        ),
    ] {
        let call = encode_propose_enact_law(
            tier,
            region,
            &houses,
            &cid("NRP"),
            b"REPRESENTATIVE",
            &cid("PRS"),
            None,
            2,
            b"law",
            None,
            &[],
            0,
        );
        assert_eq!(&call.call_data[3..7], &raw);
        assert_eq!(
            Vec::<Vec<u8>>::decode(&mut &call.call_data[7..]).unwrap(),
            houses
        );
    }
}

/// 完整 `propose_enact_law` 编码:用链端真实 `Tier`/`VoteType` + codec 逐参数拼 golden,逐字节比对。
#[test]
fn enact_law_call_matches_codec_golden_and_prefix() {
    let houses = vec![cid("NRP"), cid("NSN")];
    let actor_cid_number = cid("NRP");
    let proposer_role_code = b"REPRESENTATIVE";
    let executive_cid_number = cid("PRS");
    let legislature_cid_number = cid("NLG");
    let chapters = sample_chapters();
    let title = "道路交通安全法".as_bytes();
    let title_en = "Road Traffic Safety Law".as_bytes();

    let chain = encode_propose_enact_law(
        1,                   // tier=National
        LawRegion::NATIONAL, // 全国仍为四零字节
        &houses,
        &actor_cid_number,
        proposer_role_code,
        &executive_cid_number,
        Some(&legislature_cid_number),
        2, // vote_type=Major
        title,
        Some(title_en),
        &chapters,
        1000,
    );

    // 前缀 [25,0] + QR 动作码 0x1900 = (25<<8)|0。
    assert_eq!(&chain.call_data[..2], &[25, 0]);
    assert_eq!(chain.action, 0x1900);

    let mut golden = Vec::new();
    golden.extend(Tier::National.encode());
    golden.extend(0u32.encode());
    golden.extend(houses.encode());
    golden.extend(actor_cid_number.encode());
    golden.extend(proposer_role_code.to_vec().encode());
    golden.extend(executive_cid_number.encode());
    golden.extend(Some(legislature_cid_number).encode());
    golden.extend(VoteType::Major.encode());
    golden.extend(title.to_vec().encode());
    golden.extend(Some(title_en.to_vec()).encode());
    golden.extend(chapters.encode());
    golden.extend(1000u64.encode());

    assert_eq!(
        &chain.call_data[2..],
        &golden[..],
        "enact call SCALE 与链端类型漂移"
    );
}

/// 修法/废法前缀、`law_id`(u64 小端)与废法尾部(无 chapters,末字节=vote_type)。
#[test]
fn amend_and_repeal_prefix_law_id_and_repeal_tail() {
    let actor_cid_number = cid("PRP");
    let proposer_role_code = b"REPRESENTATIVE";
    let executive_cid_number = cid("PGV");
    let legislature_cid_number = cid("PLG");

    let amend = encode_propose_amend_law(
        7,
        &actor_cid_number,
        proposer_role_code,
        &executive_cid_number,
        Some(&legislature_cid_number),
        0,
        b"x",
        None,
        &sample_chapters(),
        50,
    );
    assert_eq!(&amend.call_data[..2], &[25, 1]);
    assert_eq!(amend.action, 0x1901);
    assert_eq!(&amend.call_data[2..10], &7u64.to_le_bytes());

    let repeal = encode_propose_repeal_law(
        7,
        &actor_cid_number,
        proposer_role_code,
        &executive_cid_number,
        None,
        4,
    );
    assert_eq!(&repeal.call_data[..2], &[25, 2]);
    assert_eq!(&repeal.call_data[2..10], &7u64.to_le_bytes());
    // 废法尾 = actor CID + 发起岗位码 + executive CID + legislature(None) + vote_type。
    let mut golden_tail = Vec::new();
    golden_tail.extend(actor_cid_number.encode());
    golden_tail.extend(proposer_role_code.to_vec().encode());
    golden_tail.extend(executive_cid_number.encode());
    golden_tail.extend(Option::<Vec<u8>>::None.encode());
    golden_tail.push(4u8);
    assert_eq!(&repeal.call_data[10..], &golden_tail[..]);
}
