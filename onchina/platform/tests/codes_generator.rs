//! 原所属模块回归，断言与输入保持。
use super::*;

const ACCOUNT_ID_1: &str = "0x1111111111111111111111111111111111111111111111111111111111111111";
const ACCOUNT_ID_2: &str = "0x2222222222222222222222222222222222222222222222222222222222222222";

#[test]
fn citizen_cid_is_country_prefixed() {
    // 公民(CTZN)去地域化:R5 = CN 国家码 + 号码高 3 位,不再是省码。
    let code = generate_cid_number(GenerateCidInput {
        account_id: ACCOUNT_ID_1,
        p1: "1",
        province_name: "广东省",
        city_name: "荔湾市",
        institution: "CTZN",
    })
    .expect("citizen cid should generate");
    let r5 = code.split('-').next().expect("r5 segment");
    assert_eq!(&r5[0..2], "CN");
    assert!(r5[2..5].chars().all(|c| c.is_ascii_digit()));
}

#[test]
fn public_legal_keeps_real_city_code() {
    let code = generate_cid_number(GenerateCidInput {
        account_id: ACCOUNT_ID_2,
        p1: "0",
        province_name: "广东省",
        city_name: "荔湾市",
        institution: "CGOV",
    })
    .expect("public legal cid should generate");
    assert_eq!(code.split('-').next(), Some("GD001"));
}

#[test]
fn three_char_national_layout_shape() {
    let code = generate_cid_number(GenerateCidInput {
        account_id: ACCOUNT_ID_1,
        p1: "0",
        province_name: "广东省",
        city_name: "荔湾市",
        institution: "NRC",
    })
    .expect("nrc cid should generate");
    let seg2 = code.split('-').nth(1).unwrap();
    assert_eq!(seg2.len(), 5);
    assert_eq!(&seg2[0..3], "NRC");
    assert_eq!(&seg2[3..4], "0");
}

#[test]
fn pmul_has_no_number() {
    let r = generate_cid_number(GenerateCidInput {
        account_id: ACCOUNT_ID_1,
        p1: "0",
        province_name: "广东省",
        city_name: "荔湾市",
        institution: "PMUL",
    });
    assert!(r.is_err());
}
