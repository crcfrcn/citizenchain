//! 原所属模块回归，断言与输入保持。
use super::*;

const ACCOUNT_ID_1: &str = "0x1111111111111111111111111111111111111111111111111111111111111111";
const ACCOUNT_ID_2: &str = "0x2222222222222222222222222222222222222222222222222222222222222222";

#[test]
fn citizen_cid_is_country_prefixed() {
    // 公民(CTZN)去地域化:R5 = CN 国家码 + 号码高 3 位,不再是省码。
    let code = generate_cid_number(GenerateCidInput {
        seed: ACCOUNT_ID_1,
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
        seed: ACCOUNT_ID_2,
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
        seed: ACCOUNT_ID_1,
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
        seed: ACCOUNT_ID_1,
        p1: "0",
        province_name: "广东省",
        city_name: "荔湾市",
        institution: "PMUL",
    });
    assert!(r.is_err());
}

#[test]
fn cid_generator_uses_a_checked_utc_u32_year() {
    let before = u32::try_from(Utc::now().year()).unwrap();
    let cid = generate_cid_number(GenerateCidInput {
        seed: ACCOUNT_ID_1, p1: "1", province_name: "广东省",
        city_name: "荔湾市", institution: "CTZN",
    }).unwrap();
    let after = u32::try_from(Utc::now().year()).unwrap();
    let parts = primitives::cid::number::parse_cid_number_parts(&cid).unwrap();
    assert!((before..=after).contains(&parts.year));
    assert_eq!(cid.rsplit('-').next().unwrap(), format!("{:04}", parts.year));
    assert!(cid.len() <= 32);
}

#[test]
fn cid_adapter_accepts_opaque_archive_seeds_and_preserves_canonical_account_seeds() {
    for seed in [ACCOUNT_ID_1, "citizen-local-0x0123456789abcdef", "citizen-local-0x0123456789abcdef|n1"] {
        let cid = generate_cid_number(GenerateCidInput {
            seed, p1: "1", province_name: "广东省", city_name: "荔湾市", institution: "CTZN",
        }).unwrap();
        let year = primitives::cid::number::parse_cid_number_parts(&cid).unwrap().year;
        let expected = primitives::cid::generator::generate_cid_number(
            primitives::cid::generator::GenerateCidNumberInput {
                public_key: seed, p1: "1", province_code: "GD", province_name: "广东省",
                city_code: "000", city_name: "荔湾市", year, institution: "CTZN",
            },
        ).unwrap();
        assert_eq!(cid, expected);
    }
    for seed in ["", "  "] {
        assert!(generate_cid_number(GenerateCidInput {
            seed, p1: "1", province_name: "广东省", city_name: "荔湾市", institution: "CTZN",
        }).is_err());
    }
    assert!(crate::crypto::pubkey::normalize_account_id("citizen-local-0x0123456789abcdef").is_none());
}
