//! 所属原语的真实单元测试，保留原断言与固定输入。
mod code {

    use crate::cid::code::*;

    #[test]
    fn country_code_uses_constitution_name() {
        assert_eq!(country_code_text(&COUNTRY_CN), Some("CN"));
        assert_eq!(COUNTRY_CN_INFO.country_full_name, "中华民族联邦共和国");
        assert_eq!(
            COUNTRY_CN_INFO.country_full_name_en,
            "Federal Republic of the Chinese Nation"
        );
        assert_eq!(COUNTRY_CN_INFO.country_short_name, "中华联邦");
        assert_eq!(COUNTRY_CN_INFO.country_short_name_en, "Chinese Federation");
    }

    #[test]
    fn province_codes_are_two_uppercase_and_unique() {
        assert_eq!(PROVINCE_CODE_INFOS.len(), 43);
        for info in PROVINCE_CODE_INFOS {
            let text = province_code_text(&info.province_code).expect("province code ascii");
            assert_eq!(text.len(), 2);
            assert!(text.chars().all(|ch| ch.is_ascii_uppercase()));
        }
        for (i, province_code) in PROVINCE_CODES.iter().enumerate() {
            for other_province_code in PROVINCE_CODES.iter().skip(i + 1) {
                assert_ne!(province_code, other_province_code, "province duplicate");
            }
        }
        assert_eq!(province_code_by_name("广东省"), Some(*b"GD"));
        assert_eq!(province_name_by_code(b"HJ"), Some("合江省"));
    }

    #[test]
    fn institution_codes_are_three_or_four_uppercase_and_unique() {
        assert_eq!(INSTITUTION_CODE_INFOS.len(), 104);
        for info in INSTITUTION_CODE_INFOS {
            let text = info.institution_code_text;
            assert!(text.len() == 3 || text.len() == 4);
            assert!(text.chars().all(|ch| ch.is_ascii_uppercase()));
            assert_eq!(institution_code_from_str(text), Some(info.institution_code));
            assert_eq!(
                institution_code_from_str(info.institution_code_label),
                Some(info.institution_code)
            );
        }
        for (i, institution_code) in ALL_CODES.iter().enumerate() {
            for other_institution_code in ALL_CODES.iter().skip(i + 1) {
                assert_ne!(
                    institution_code, other_institution_code,
                    "institution duplicate"
                );
            }
        }
    }

    #[test]
    fn classification_spot_check() {
        assert_eq!(institution_code_text(&NRC), Some("NRC"));
        assert_eq!(institution_code_label(&NRC), Some("国家储委会"));
        assert!(is_fixed_governance_code(&NRC));
        assert!(!is_registered_multisig_code(&NRC));
        assert!(is_public_legal_code(&NRC));
        assert!(is_fixed_governance_code(&FRG));
        assert!(!is_registered_multisig_code(&FRG));
        assert!(!is_institution_code(&FRG));
        assert!(is_fixed_governance_code(&NJD));
        assert!(!is_registered_multisig_code(&NJD));
        assert!(is_public_legal_code(&NJD));
        assert_eq!(
            institution_code_label(&code_bytes("FDA")),
            Some("食品药品监管局")
        );
        assert_eq!(
            institution_code_label(&code_bytes("NGB")),
            Some("国民警卫局")
        );
        assert_eq!(admin_level(&code_bytes("ARM")), Some(AdminLevel::National));
        assert_eq!(admin_level(&code_bytes("NGC")), Some(AdminLevel::National));

        assert!(is_personal_code(&PMUL));
        assert!(is_registered_multisig_code(&PMUL));
        assert!(!is_institution_code(&PMUL));

        assert!(is_institution_code(b"CGOV"));
        assert!(is_institution_code(b"SFLP"));
        assert!(is_unincorporated_code(b"UNIN"));
        assert!(is_person_code(b"CTZN"));
        assert!(!is_registered_multisig_code(b"CTZN"));
    }

    #[test]
    fn profit_policy_spot_check() {
        assert_eq!(profit_policy(b"SFGQ"), Some(ProfitPolicy::Profit));
        assert_eq!(profit_policy(b"SFGY"), Some(ProfitPolicy::NonProfit));
        assert_eq!(profit_policy(b"SFAS"), Some(ProfitPolicy::Variable));
        assert_eq!(profit_policy(b"SMTP"), Some(ProfitPolicy::Variable));
        assert_eq!(profit_policy(b"UNIN"), Some(ProfitPolicy::InheritParent));
    }

    #[test]
    fn fixed_governance_thresholds_match_constants() {
        assert_eq!(fixed_governance_pass_threshold(&NRC), Some(13));
        assert_eq!(fixed_governance_pass_threshold(&PRC), Some(6));
        assert_eq!(fixed_governance_pass_threshold(&PRB), Some(6));
        assert_eq!(fixed_governance_pass_threshold(&FRG), Some(3));
        assert_eq!(fixed_governance_pass_threshold(&NJD), Some(8));
        assert_eq!(fixed_governance_pass_threshold(&PMUL), None);
        assert_eq!(fixed_governance_pass_threshold(b"CGOV"), None);
    }

    #[test]
    fn code_bytes_pads_three_char() {
        assert_eq!(code_bytes("NRC"), *b"NRC\0");
        assert_eq!(code_bytes("CGOV"), *b"CGOV");
    }
}
mod number {
    use crate::cid::code;

    use crate::cid::generator::{generate_cid_number, GenerateCidNumberInput};
    use crate::cid::number::*;

    fn gen(institution: &str, p1: &str, province: &str, city: &str) -> String {
        let province_code = code::province_code_by_name(province)
            .and_then(|code| code::province_code_text(&code))
            .expect("test province code");
        let city_code = match city {
            "荔湾市" => "001",
            _ => "001",
        };
        generate_cid_number(GenerateCidNumberInput {
            public_key: "0xabcd",
            p1,
            province_code,
            province_name: province,
            city_code,
            city_name: city,
            year: "2026",
            institution,
        })
        .expect("cid should generate")
    }

    #[test]
    fn roundtrip_three_char_layout() {
        // 3 字符码 roundtrip。
        let code = gen("NRC", "0", "广东省", "荔湾市");
        let parts = parse_cid_number_parts(&code).expect("must parse");
        assert_eq!(parts.institution, code::NRC);
        assert_eq!(parts.institution_code_text, "NRC");
        assert!(!parts.profit);
        assert!(validate_cid_number_format(&code).is_ok());
    }

    #[test]
    fn scope_codes_are_derived_from_validated_r5() {
        let number = gen("CGOV", "0", "广东省", "荔湾市");
        assert_eq!(
            cid_scope_codes(number.as_bytes()).expect("valid cid has scope"),
            (*b"GD", *b"001")
        );
        assert!(cid_scope_codes(b"GD001-CGOVX-944805165").is_err());
    }

    #[test]
    fn person_cid_parses_but_has_no_scope() {
        // 人主体 CID(公民/居民/智能人):CN 前缀号段能生成+解析,但不产出省/市作用域(fail-closed)。
        for institution in ["CTZN", "NATP", "SMTP"] {
            let code = generate_cid_number(GenerateCidNumberInput {
                public_key: "0xabcd",
                p1: "1",
                province_code: "",
                province_name: "",
                city_code: "",
                city_name: "",
                year: "2026",
                institution,
            })
            .unwrap_or_else(|e| panic!("{institution} cid should generate: {e}"));
            assert_eq!(&code[0..2], "CN");
            let parts = parse_cid_number_parts(&code).expect("person cid must parse");
            assert!(
                cid_scope_codes(code.as_bytes()).is_err(),
                "{institution} must have no scope"
            );
            let _ = parts;
        }
    }

    #[test]
    fn roundtrip_four_char_profit() {
        // 4 字符盈利码 roundtrip。
        let code = gen("SFGQ", "1", "广东省", "荔湾市");
        let parts = parse_cid_number_parts(&code).expect("must parse");
        assert_eq!(parts.institution, *b"SFGQ");
        assert!(parts.profit);
        assert!(parts.checksum.is_ascii_digit());
    }

    #[test]
    fn roundtrip_four_char_nonprofit() {
        // 4 字符非盈利码 roundtrip。
        let code = gen("CGOV", "0", "广东省", "荔湾市");
        let parts = parse_cid_number_parts(&code).expect("must parse");
        assert_eq!(parts.institution, *b"CGOV");
        assert!(!parts.profit);
        assert!(parts.checksum.is_ascii_uppercase());
    }

    #[test]
    fn all_codes_roundtrip_generate_parse() {
        // 全部发号码都必须生成、解析一致。
        for institution_code in code::ALL_CODES {
            if institution_code == code::PMUL {
                continue;
            }
            let number = generate_cid_number(GenerateCidNumberInput {
                public_key: "0xfeed",
                // 固定策略会忽略 p1。
                p1: "1",
                province_code: "GD",
                province_name: "广东省",
                city_code: "001",
                city_name: "荔湾市",
                year: "2026",
                institution: code::institution_code_text(&institution_code)
                    .expect("institution code text"),
            })
            .unwrap_or_else(|e| {
                panic!(
                    "{} should generate: {e}",
                    code::institution_code_text(&institution_code).expect("institution code text")
                )
            });
            let parts = parse_cid_number_parts(&number).unwrap_or_else(|e| {
                panic!(
                    "{} should parse: {e}",
                    code::institution_code_text(&institution_code).expect("institution code text")
                )
            });
            assert_eq!(
                parts.institution,
                institution_code,
                "roundtrip code mismatch for {}",
                code::institution_code_text(&institution_code).expect("institution code text")
            );
            assert!(validate_cid_number_format(&number).is_ok());
        }
    }

    #[test]
    fn rejects_bad_segment_count() {
        assert!(validate_cid_number_format("GD001-CGOVX-944805165").is_err());
    }

    #[test]
    fn rejects_empty() {
        assert!(validate_cid_number_format("").is_err());
        assert!(validate_cid_number_format("   ").is_err());
    }

    #[test]
    fn rejects_lowercase() {
        let code = gen("CGOV", "0", "广东省", "荔湾市");
        let lowered = code.to_lowercase();
        assert!(validate_cid_number_format(&lowered).is_err());
    }

    #[test]
    fn rejects_legacy_format() {
        // 旧版号必须校验失败。
        assert!(validate_cid_number_format("LN001-NRC05-944805165-2026").is_err());
        assert!(validate_cid_number_format("GFR-AH001-ZF0X-898100720-2026").is_err());
    }

    #[test]
    fn rejects_tampered_checksum() {
        let code = gen("NRC", "0", "广东省", "荔湾市");
        // 篡改校验位。
        let chars: Vec<char> = code.chars().collect();
        // 第二段最后一位是校验位。
        let parts: Vec<&str> = code.split('-').collect();
        let seg2 = parts[1];
        let bad_checksum = if seg2.ends_with('0') { '1' } else { '0' };
        let tampered = format!(
            "{}-{}{}-{}-{}",
            parts[0],
            &seg2[..4],
            bad_checksum,
            parts[2],
            parts[3]
        );
        let _ = chars; // silence
        assert!(validate_cid_number_format(&tampered).is_err());
    }
}
mod official_template {

    use crate::cid::official_template::*;

    #[test]
    fn template_family_counts() {
        assert_eq!(PROVINCE_DEPARTMENT_TEMPLATES.len(), 11);
        assert_eq!(CITY_TEMPLATES.len(), 17);
        assert_eq!(TOWN_TEMPLATES.len(), 14);
    }

    #[test]
    fn name_composition_matches_reverse_verified_examples() {
        let gov = &CITY_TEMPLATES[0];
        assert_eq!(gov.cid_short_name("荔湾市"), "荔湾市政府");
        assert_eq!(gov.cid_full_name("荔湾市"), "荔湾市自治政府");
        let dept = &PROVINCE_DEPARTMENT_TEMPLATES[0];
        assert_eq!(dept.cid_short_name("广东省"), "广东省国防厅");
        let town = &TOWN_TEMPLATES[0];
        assert_eq!(town.cid_short_name("锦程镇"), "锦程镇政府");
    }
}
mod official_derive {

    use crate::cid::official_derive::*;
    use alloc::collections::BTreeSet;
    use alloc::string::ToString;

    #[test]
    fn derived_count_matches_area_and_templates() {
        assert_eq!(
            public_institution_derived_count(),
            49_297,
            "派生机构总数 = 11×43 + 17×2872"
        );
    }

    #[test]
    fn every_derived_number_is_valid_public_and_unique() {
        let expected = public_institution_derived_count();
        let mut count = 0usize;
        let mut seen = BTreeSet::<String>::new();
        for_each_public_institution(|cid, full, short| {
            count += 1;
            let parts = crate::cid::number::parse_cid_number_parts(cid)
                .unwrap_or_else(|e| panic!("派生号 {cid} 非法: {e}"));
            assert!(
                crate::cid::code::is_public_legal_code(&parts.institution),
                "派生号 {cid} 非公权家族"
            );
            assert!(!full.is_empty() && !short.is_empty(), "派生名不能为空");
            assert!(seen.insert(cid.to_string()), "派生号 {cid} 重复");
        });
        assert_eq!(count, expected, "枚举数量与推导一致");
    }

    #[test]
    fn genesis_institution_numbers_are_globally_unique() {
        // 全量创世机构号必须全局唯一:常量直铸(296 公权)+ 私权基金会 + 模板派生(49,297)
        // 三者跨集合、集合内都不得重号。派生号一旦与某常量机构撞号,会在 publicentity
        // 存储里静默覆盖;故在此显式断言,不依赖「机构码族不相交」的隐式推理。
        use crate::cid::china::china_cb::CHINA_CB;
        use crate::cid::china::china_ch::CHINA_CH;
        use crate::cid::china::china_jc::CHINA_JC;
        use crate::cid::china::china_jy::CHINA_JY;
        use crate::cid::china::china_lf::CHINA_LF;
        use crate::cid::china::china_sf::CHINA_SF;
        use crate::cid::china::china_zf::CHINA_ZF;
        use crate::cid::china::citizenchain::CITIZENCHAIN_FOUNDATION;

        let mut seen = BTreeSet::<String>::new();

        // 先放入常量机构号(7 张公权表 + 私权基金会),逐条断言常量之间不重复。
        for cid in CHINA_CB
            .iter()
            .map(|n| n.cid_number)
            .chain(CHINA_CH.iter().map(|n| n.cid_number))
            .chain(CHINA_ZF.iter().map(|n| n.cid_number))
            .chain(CHINA_JC.iter().map(|n| n.cid_number))
            .chain(CHINA_SF.iter().map(|n| n.cid_number))
            .chain(CHINA_LF.iter().map(|n| n.cid_number))
            .chain(CHINA_JY.iter().map(|n| n.cid_number))
            .chain(core::iter::once(CITIZENCHAIN_FOUNDATION.cid_number))
        {
            assert!(seen.insert(cid.to_string()), "常量创世机构号 {cid} 重复");
        }

        // 再枚举全部派生机构号,断言与常量及彼此都不重号。
        for_each_public_institution(|cid, _full, _short| {
            assert!(
                seen.insert(cid.to_string()),
                "派生创世机构号 {cid} 与常量或其它派生机构重号"
            );
        });
    }
}
mod generator {

    use crate::cid::generator::*;

    fn gen(institution: &str, p1: &str) -> String {
        generate_cid_number(GenerateCidNumberInput {
            public_key: "0xabcd",
            p1,
            province_code: "GD",
            province_name: "广东省",
            city_code: "001",
            city_name: "荔湾市",
            year: "2026",
            institution,
        })
        .expect("cid should generate")
    }

    #[test]
    fn all_person_codes_are_country_prefixed_not_province() {
        // 人主体(公民 CTZN / 居民 NATP / 智能人 SMTP)统一去地域化:R5 = CN + 号码高 3 位。
        for institution in ["CTZN", "NATP", "SMTP"] {
            let code = gen(institution, "1");
            let r5 = code.split('-').next().expect("r5 segment");
            assert_eq!(&r5[0..2], "CN", "{institution} r5 must be CN-prefixed");
            assert!(r5[2..5].chars().all(|c| c.is_ascii_digit()));
            assert_ne!(&r5[0..2], "GD");
        }
    }

    #[test]
    fn person_cid_needs_no_province() {
        // 自助占号无居住地:人主体号段只吃公钥/码/年,省市留空也能生成(区别于机构码)。
        for institution in ["CTZN", "NATP", "SMTP"] {
            let code = generate_cid_number(GenerateCidNumberInput {
                public_key: "0xabcd",
                p1: "1",
                province_code: "",
                province_name: "",
                city_code: "",
                city_name: "",
                year: "2026",
                institution,
            })
            .unwrap_or_else(|e| panic!("{institution} cid should generate without province: {e}"));
            assert_eq!(&code[0..2], "CN");
        }
    }

    #[test]
    fn citizen_cid_number_golden() {
        // 人主体 CID 号段金标(model B dev //0 公钥,CTZN,2026 年)。CN 前缀 + 12 位号段:
        // R5[2:5] 高 3 位 + N9 低 9 位共同构成 1e12/年容量。逐字节钉死,防生成规则漂移。
        let code = generate_cid_number(GenerateCidNumberInput {
            public_key: "0x2afba9278e30ccf6a6ceb3a8b6e336b70068f045c666f2e7f4f9cc5f47db8972",
            p1: "1",
            province_code: "",
            province_name: "",
            city_code: "",
            city_name: "",
            year: "2026",
            institution: "CTZN",
        })
        .expect("citizen golden cid should generate");
        assert_eq!(code, "CN951-CTZN1-539598435-2026");
    }

    #[test]
    fn public_legal_keeps_real_city_code() {
        let code = gen("CGOV", "0");
        assert_eq!(code.split('-').next(), Some("GD001"));
    }

    #[test]
    fn three_char_national_layout_shape() {
        let code = gen("NRC", "0");
        let seg2 = code.split('-').nth(1).unwrap();
        assert_eq!(seg2.len(), 5);
        assert_eq!(&seg2[0..3], "NRC");
        assert_eq!(&seg2[3..4], "0");
    }

    #[test]
    fn pmul_has_no_number() {
        let r = generate_cid_number(GenerateCidNumberInput {
            public_key: "0x1",
            p1: "0",
            province_code: "GD",
            province_name: "广东省",
            city_code: "001",
            city_name: "荔湾市",
            year: "2026",
            institution: "PMUL",
        });
        assert!(r.is_err());
    }
}
