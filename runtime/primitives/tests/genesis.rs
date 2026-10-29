//! 所属原语的真实单元测试，保留原断言与固定输入。
mod genesis {

    use crate::genesis::*;

    #[test]
    fn law_region_keeps_letters_city_numbers_and_frozen_national_bytes() {
        use codec::{Decode, Encode};
        assert_eq!(LawRegion::NATIONAL.encode(), 0u32.encode());
        assert_eq!(
            LawRegion::from_codes(2, b"GD", b"001").unwrap().encode(),
            [b'G', b'D', 0, 0]
        );
        let city = LawRegion::from_codes(3, b"GD", b"001").unwrap();
        assert_eq!(city.encode(), [b'G', b'D', 1, 0]);
        assert_ne!(city, LawRegion::from_codes(3, b"GZ", b"001").unwrap());
        assert_eq!(LawRegion::decode(&mut &city.encode()[..]).unwrap(), city);
        assert_eq!(
            LawRegion::from_codes(3, b"GD", b"999").unwrap().city_number,
            999
        );
        // 不新增“市号必须大于零”的编号规则；合法性仍由原 CID/字典入口负责。
        assert_eq!(
            LawRegion::from_codes(3, b"GD", b"000").unwrap().city_number,
            0
        );
        for (tier, province, city) in [
            (4, &b"GD"[..], &b"001"[..]),
            (2, b"gd", b""),
            (2, b"G", b""),
            (3, b"GD", b"1"),
            (3, b"GD", b"0a1"),
        ] {
            assert!(LawRegion::from_codes(tier, province, city).is_err());
        }
        assert!(!LawRegion::NATIONAL.matches_tier(2));
        assert!(!(LawRegion {
            province_code: *b"GD",
            city_number: 1000
        })
        .matches_tier(3));
        assert!(LawRegion::from_cid(2, b"invalid").is_err());
        assert_eq!(crate::cid::code::COUNTRY_CN, *b"CN");
    }

    #[test]
    fn genesis_issuance_matches_population() {
        // 创世发行 = 人口 × 10_000 分。
        assert_eq!(GENESIS_ISSUANCE, GENESIS_CITIZEN_MAX as u128 * 10_000u128);
    }

    #[test]
    fn he_fund_issuance_matches_whitepaper() {
        // 两和基金发行 = 195,818,501,966.00 元 × 100 分/元。
        assert_eq!(HE_FUND_ISSUANCE, 195_818_501_966u128 * 100);
    }

    #[test]
    fn genesis_law_version_label_is_constitution_genesis() {
        let label = &GENESIS_LAW_VERSION_LABELS[0];
        assert_eq!(label.law_id, 0);
        assert_eq!(label.version, 1);
        assert_eq!(label.title, "创世版");
        assert_eq!(label.title_en, "Genesis Edition");
    }
}
