// 原账户派生一致性用例，说明随测试迁移。

// 内置机构主/费用账户派生对拍测试。

//! 所属原语的真实单元测试，保留原断言与固定输入。
mod derive {

    use crate::account_derive::AccountKind;
    use crate::core_const::SS58_FORMAT;

    #[test]
    fn china_ch_main_fee_accounts_match_derive_primitive() {
        for n in crate::cid::china::china_ch::CHINA_CH {
            let cid = n.cid_number.as_bytes();
            assert_eq!(
                n.main_account,
                AccountKind::InstitutionMain { cid_number: cid }.derive(SS58_FORMAT),
                "省储行 {} 主账户派生漂移",
                n.cid_full_name
            );
            assert_eq!(
                n.fee_account,
                AccountKind::InstitutionFee { cid_number: cid }.derive(SS58_FORMAT),
                "省储行 {} 费用账户派生漂移",
                n.cid_full_name
            );
        }
    }

    #[test]
    fn china_cb_main_fee_accounts_match_derive_primitive() {
        for n in crate::cid::china::china_cb::CHINA_CB {
            let cid = n.cid_number.as_bytes();
            assert_eq!(
                n.main_account,
                AccountKind::InstitutionMain { cid_number: cid }.derive(SS58_FORMAT),
                "储委会 {} 主账户派生漂移",
                n.cid_full_name
            );
            assert_eq!(
                n.fee_account,
                AccountKind::InstitutionFee { cid_number: cid }.derive(SS58_FORMAT),
                "储委会 {} 费用账户派生漂移",
                n.cid_full_name
            );
        }
    }

    #[test]
    fn china_other_institutions_main_fee_match_derive_primitive() {
        macro_rules! check_arr {
            ($arr:expr, $label:expr) => {
                for n in $arr {
                    let cid = n.cid_number.as_bytes();
                    assert_eq!(
                        n.main_account,
                        AccountKind::InstitutionMain { cid_number: cid }.derive(SS58_FORMAT),
                        "{} {} 主账户派生漂移",
                        $label,
                        n.cid_full_name
                    );
                    assert_eq!(
                        n.fee_account,
                        AccountKind::InstitutionFee { cid_number: cid }.derive(SS58_FORMAT),
                        "{} {} 费用账户派生漂移",
                        $label,
                        n.cid_full_name
                    );
                }
            };
        }
        check_arr!(crate::cid::china::china_zf::CHINA_ZF, "政府");
        check_arr!(crate::cid::china::china_lf::CHINA_LF, "立法");
        check_arr!(crate::cid::china::china_sf::CHINA_SF, "司法");
        check_arr!(crate::cid::china::china_jc::CHINA_JC, "监察");
        check_arr!(crate::cid::china::china_jy::CHINA_JY, "教育");
        check_arr!(
            core::slice::from_ref(&crate::cid::china::citizenchain::CITIZENCHAIN_FOUNDATION),
            "公民链技术"
        );
    }

    #[test]
    fn reserved_account_list_matches_builtin_institution_accounts() {
        let mut accounts: Vec<[u8; 32]> = Vec::new();
        macro_rules! push_main_fee {
            ($arr:expr) => {
                for node in $arr {
                    accounts.push(node.main_account);
                    accounts.push(node.fee_account);
                }
            };
        }

        push_main_fee!(crate::cid::china::china_cb::CHINA_CB);
        push_main_fee!(crate::cid::china::china_ch::CHINA_CH);
        push_main_fee!(crate::cid::china::china_zf::CHINA_ZF);
        push_main_fee!(crate::cid::china::china_jc::CHINA_JC);
        push_main_fee!(crate::cid::china::china_lf::CHINA_LF);
        push_main_fee!(crate::cid::china::china_sf::CHINA_SF);
        push_main_fee!(crate::cid::china::china_jy::CHINA_JY);
        push_main_fee!(core::slice::from_ref(
            &crate::cid::china::citizenchain::CITIZENCHAIN_FOUNDATION,
        ));
        accounts.extend(
            crate::cid::china::china_ch::CHINA_CH
                .iter()
                .map(|node| node.stake_account),
        );
        accounts.push(crate::cid::china::china_cb::SAFETY_FUND_ACCOUNT);
        accounts.push(crate::cid::china::china_cb::NRC_HE_ACCOUNT);
        accounts.sort();
        accounts.dedup();

        assert_eq!(
            accounts.len(),
            crate::cid::china::china_zb::CHINA_RESERVED_MAIN_ACCOUNTS.len(),
            "制度保留地址数量必须覆盖全部内置机构主/费用账户、省储行质押账户和国家储委会基金账户"
        );
        assert_eq!(
            accounts.as_slice(),
            crate::cid::china::china_zb::CHINA_RESERVED_MAIN_ACCOUNTS.as_slice(),
            "china_zb.rs 必须与内置制度账户排序去重后一致"
        );
    }

    #[test]
    fn builtin_institution_name_have_runtime_digest() {
        let digest = crate::cid::china::builtin_institution_name_digest();
        assert_ne!(digest, [0u8; 32], "内置机构名称四元组 runtime 指纹不可为空");
        // 名称常量与原件核对后，通过所属公开实现取得的独立测试金标。
        assert_eq!(
            digest,
            hex_literal::hex!("fff5304630d85d517d715b4fb67aa8ff8f72e778a9236a0f64c2a739f015748c"),
            "内置机构名称内容、顺序或指纹算法发生漂移"
        );
    }

    #[test]
    fn builtin_institution_name_values_are_present() {
        macro_rules! check_arr {
            ($arr:expr, $label:expr) => {
                for n in $arr {
                    assert!(
                        !n.cid_full_name.trim().is_empty(),
                        "{} 缺少 cid_full_name",
                        $label
                    );
                    assert!(
                        !n.cid_short_name.trim().is_empty(),
                        "{} {} 缺少 cid_short_name",
                        $label,
                        n.cid_full_name
                    );
                    assert!(
                        !n.full_name_en.trim().is_empty(),
                        "{} {} 缺少 full_name_en",
                        $label,
                        n.cid_full_name
                    );
                    assert!(
                        !n.short_name_en.trim().is_empty(),
                        "{} {} 缺少 short_name_en",
                        $label,
                        n.cid_full_name
                    );
                }
            };
        }
        check_arr!(crate::cid::china::china_zf::CHINA_ZF, "政府");
        check_arr!(crate::cid::china::china_lf::CHINA_LF, "立法");
        check_arr!(crate::cid::china::china_sf::CHINA_SF, "司法");
        check_arr!(crate::cid::china::china_jc::CHINA_JC, "监察");
        check_arr!(crate::cid::china::china_jy::CHINA_JY, "教育");
        check_arr!(crate::cid::china::china_cb::CHINA_CB, "储委会");
        check_arr!(crate::cid::china::china_ch::CHINA_CH, "储行");
        check_arr!(
            core::slice::from_ref(&crate::cid::china::citizenchain::CITIZENCHAIN_FOUNDATION),
            "公民链技术"
        );
    }
}
mod china_ch {

    use crate::cid::china::china_ch::CHINA_CH;
    use crate::genesis::GENESIS_CITIZEN_MAX;

    #[test]
    fn citizens_sum_matches_genesis_total() {
        // 省储行人口总和必须等于创世人口。
        let citizens_sum: u64 = CHINA_CH.iter().map(|n| n.citizens_number).sum();
        assert_eq!(citizens_sum, GENESIS_CITIZEN_MAX);
    }

    #[test]
    fn stake_sum_matches_population_basis() {
        // 创立发行按每人 10_000 分汇总。
        let citizens_sum: u128 = CHINA_CH.iter().map(|n| n.citizens_number as u128).sum();
        let stake_sum: u128 = CHINA_CH.iter().map(|n| n.stake_amount).sum();
        assert_eq!(stake_sum, citizens_sum * 10_000u128);
        assert!(
            CHINA_CH.iter().all(|n| n.stake_amount % 10_000 == 0),
            "每家省储行创立质押本金必须按整人口基数计算，保证逐年 BP 利息可精确复算"
        );
    }

    #[test]
    fn all_china_ch_main_accounts_are_unique() {
        // 省储行主账户必须唯一。
        let mut addrs: Vec<[u8; 32]> = CHINA_CH.iter().map(|n| n.main_account).collect();
        let total = addrs.len();
        addrs.sort();
        addrs.dedup();
        assert_eq!(addrs.len(), total, "main_account duplicate found");
    }

    #[test]
    fn all_stake_accounts_are_unique_and_separate_from_main_accounts() {
        let mut stake_accounts: Vec<[u8; 32]> = CHINA_CH.iter().map(|n| n.stake_account).collect();
        let total = stake_accounts.len();
        stake_accounts.sort();
        stake_accounts.dedup();
        assert_eq!(stake_accounts.len(), total, "stake_account duplicate found");
        assert!(CHINA_CH.iter().all(|bank| !CHINA_CH
            .iter()
            .any(|entry| entry.main_account == bank.stake_account)));
    }
}
mod area {

    use crate::cid::china::area::*;

    #[test]
    fn area_counts_match_china_sqlite_snapshot() {
        let (p, c, t) = area_counts();
        assert_eq!(p, 43, "省数");
        assert_eq!(c, 2872, "市数");
        assert_eq!(t, 39087, "镇数");
    }

    #[test]
    fn first_city_parses() {
        let mut first_city: Option<(alloc::string::String, alloc::string::String)> = None;
        for_each_area(|item| {
            if let AreaItem::City(city) = item {
                if first_city.is_none() {
                    first_city = Some((
                        alloc::string::ToString::to_string(city.province_code),
                        alloc::string::ToString::to_string(city.city_name),
                    ));
                }
            }
        });
        let (pc, name) = first_city.expect("has city");
        assert_eq!(pc.len(), 2);
        assert!(!name.is_empty());
    }
}
