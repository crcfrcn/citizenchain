//! 所属原语的真实单元测试，保留原断言与固定输入。
mod governance_skeleton {
    #![allow(clippy::expect_used)]
    use crate::cid::code::*;
    use crate::count_const::*;
    use crate::institution_constraints::ROLE_CODE_LEGAL_REPRESENTATIVE;

    // 测试代码沿用 expect() 断言(工作区 expect_used=warn 面向生产码;测试内 expect 是惯用法)。

    use crate::governance_skeleton::*;

    #[test]
    fn fixed_institutions_cover_fixed_codes_with_expected_counts() {
        let list = fixed_institutions();

        assert_eq!(list.len(), 89, "受保护创世治理机构必须精确为 89 个");
        assert_eq!(list.iter().filter(|item| item.code == NRC).count(), 1);
        assert_eq!(list.iter().filter(|item| item.code == PRC).count(), 43);
        assert_eq!(list.iter().filter(|item| item.code == PRB).count(), 43);
        assert_eq!(list.iter().filter(|item| item.code == NJD).count(), 1);
        assert_eq!(list.iter().filter(|item| item.code == FRG).count(), 1);

        let mut cids = list.iter().map(|item| item.cid_number).collect::<Vec<_>>();
        cids.sort_unstable();
        cids.dedup();
        assert_eq!(cids.len(), list.len(), "保护清单 CID 不得重复");
        let mut accounts = list
            .iter()
            .map(|item| item.main_account)
            .collect::<Vec<_>>();
        accounts.sort_unstable();
        accounts.dedup();
        assert_eq!(accounts.len(), list.len(), "保护清单主账户不得重复");

        let nrc = list.iter().find(|f| f.code == NRC).expect("NRC 必须在册");
        assert_eq!(nrc.expected_len, NRC_ADMIN_COUNT);

        let njd = list.iter().find(|f| f.code == NJD).expect("NJD 必须在册");
        assert_eq!(njd.expected_len, NJD_ADMIN_COUNT);
        let court = fixed_role_specs(NJD)
            .into_iter()
            .find(|role| role.role_code == ROLE_CODE_CONSTITUTION_GUARD)
            .expect("NJD 必须有护宪岗位");
        assert_eq!(court.role_name, ROLE_CONSTITUTION_GUARD);
        assert_eq!(court.seats, NJD_CONSTITUTION_GUARD_SEATS);

        assert!(list
            .iter()
            .any(|f| f.code == PRC && f.expected_len == PRC_ADMIN_COUNT));
        assert!(list
            .iter()
            .any(|f| f.code == PRB && f.expected_len == PRB_ADMIN_COUNT));
        let frg = list
            .iter()
            .find(|f| f.code == FRG)
            .expect("FRG 必须作为受保护创世机构在册");
        assert_eq!(
            frg.expected_len,
            PROVINCE_CODE_INFOS.len() as u32 * FRG_PROVINCE_GROUP_ADMIN_COUNT
        );
    }

    #[test]
    fn fixed_identity_requires_code_and_cid_to_match() {
        let institution = fixed_institutions()[0];
        assert!(
            fixed_institution_by_identity(institution.code, institution.cid_number.as_bytes(),)
                .is_some()
        );
        assert!(fixed_institution_by_identity(institution.code, b"not-genesis-cid",).is_none());
        assert!(
            fixed_institution_by_identity(*b"CGOV", institution.cid_number.as_bytes(),).is_none()
        );
    }

    #[test]
    fn legal_representative_has_one_role_and_zero_or_one_assignment() {
        let institution = fixed_institutions()[0];
        assert_eq!(
            fixed_role_assignment_bounds_by_identity(
                institution.code,
                institution.cid_number.as_bytes(),
                ROLE_CODE_LEGAL_REPRESENTATIVE,
            ),
            Some((0, 1))
        );
        assert_eq!(
            fixed_role_assignment_bounds_by_identity(
                institution.code,
                institution.cid_number.as_bytes(),
                fixed_role_specs(institution.code)[0].role_code,
            ),
            Some((institution.expected_len, institution.expected_len))
        );
    }

    #[test]
    fn njd_guard_seats_is_seven() {
        // 4/7 终审的「7」;与创世 role-by-index 0..=6 一致。
        assert_eq!(NJD_CONSTITUTION_GUARD_SEATS, 7);
    }

    #[test]
    fn frg_groups_cover_all_provinces_with_five_seats() {
        let groups = frg_province_groups();
        assert_eq!(groups.len(), PROVINCE_CODE_INFOS.len());
        assert!(groups
            .iter()
            .all(|(_, n)| *n == FRG_PROVINCE_GROUP_ADMIN_COUNT));
    }

    #[test]
    fn role_literal_is_stable() {
        assert_eq!(ROLE_CONSTITUTION_GUARD, "护宪大法官".as_bytes());
    }

    #[test]
    fn fixed_role_seat_lookup_covers_judicial_and_federal_registry_roles() {
        assert_eq!(fixed_role_seats(NJD, ROLE_CODE_CONSTITUTION_GUARD), Some(7));
        let first_province = PROVINCE_CODE_INFOS[0];
        assert_eq!(
            fixed_role_seats(
                FRG,
                &province_commissioner_role_code(first_province.province_code)
            ),
            Some(FRG_PROVINCE_GROUP_ADMIN_COUNT)
        );
        assert_eq!(fixed_role_seats(NJD, b"UNKNOWN"), None);
    }
}
mod institution_constraints {
    use crate::account_derive::InstitutionProtocolAccountKind;
    use crate::cid::code::*;

    use crate::institution_constraints::*;

    #[test]
    fn singleton_identities_and_codes_are_unique() {
        let list = singleton_institutions();
        assert_eq!(list.len(), 6);
        for (index, item) in list.iter().enumerate() {
            assert!(is_permanent_singleton_code(&item.code));
            assert!(!list[..index].iter().any(|other| other.code == item.code
                || other.cid_number == item.cid_number
                || other.main_account == item.main_account));
        }
    }

    #[test]
    fn member_composition_ranges_match_institutional_design() {
        let specs = member_composition_specs();
        assert_eq!(specs.len(), 3);
        assert_eq!((specs[0].min_members, specs[0].max_members), (105, 155));
        assert_eq!((specs[1].min_members, specs[1].max_members), (305, 355));
        assert_eq!((specs[2].min_members, specs[2].max_members), (105, 155));
        assert_eq!(national_legislature_components()[0].code, NSN);
        assert_eq!(national_legislature_components()[1].code, NRP);
    }

    #[test]
    fn required_protocol_accounts_follow_institution_design() {
        let nrc = crate::cid::china::china_cb::CHINA_CB
            .iter()
            .find(|item| institution_code_from_cid_number(item.cid_number) == Some(NRC))
            .expect("NRC genesis institution");
        assert_eq!(
            required_protocol_account_kinds(NRC, nrc.cid_number.as_bytes(), None),
            Some(NRC_PROTOCOL_ACCOUNT_KINDS)
        );

        let prb = crate::cid::china::china_ch::CHINA_CH
            .iter()
            .find(|item| institution_code_from_cid_number(item.cid_number) == Some(PRB))
            .expect("PRB genesis institution");
        assert_eq!(
            required_protocol_account_kinds(PRB, prb.cid_number.as_bytes(), None),
            Some(PRB_PROTOCOL_ACCOUNT_KINDS)
        );

        assert_eq!(
            required_protocol_account_kinds(NRC, prb.cid_number.as_bytes(), None),
            None
        );
    }

    /// 清算账户资格唯二：SFGF 本身、以及父级为 SFGF 的 UNIN；其余一律无清算账户。
    #[test]
    fn clearing_account_only_for_joint_stock_and_its_unincorporated_branch() {
        const SFGF_CID: &[u8] = b"GD001-SFGF0-123456789-2026";
        const UNIN_CID: &[u8] = b"GD001-UNIN0-123456789-2026";
        const SFLP_CID: &[u8] = b"GD001-SFLP0-123456789-2026";
        const SFGT_CID: &[u8] = b"GD001-SFGT0-123456789-2026";
        const SFGP_CID: &[u8] = b"GD001-SFGP0-123456789-2026";

        let has_clearing = |kinds: Option<&'static [InstitutionProtocolAccountKind]>| {
            kinds
                .expect("CID 与机构码一致")
                .contains(&InstitutionProtocolAccountKind::Clearing)
        };

        // ① 股份公司本身。
        assert!(has_clearing(required_protocol_account_kinds(
            SFGF, SFGF_CID, None
        )));
        // ② 父级为 SFGF 的非法人分支机构。
        assert!(has_clearing(required_protocol_account_kinds(
            UNIN,
            UNIN_CID,
            Some(SFGF_CID)
        )));

        // 父级不是股份公司的 UNIN：无清算账户。
        assert!(!has_clearing(required_protocol_account_kinds(
            UNIN,
            UNIN_CID,
            Some(SFLP_CID)
        )));
        // 无父级的 UNIN：无清算账户。
        assert!(!has_clearing(required_protocol_account_kinds(
            UNIN, UNIN_CID, None
        )));
        // 个体经营 / 无限合伙是独立经营主体、无父级概念：无清算账户。
        assert!(!has_clearing(required_protocol_account_kinds(
            *b"SFGT", SFGT_CID, None
        )));
        assert!(!has_clearing(required_protocol_account_kinds(
            *b"SFGP", SFGP_CID, None
        )));
        // 普通私法人：无清算账户。
        assert!(!has_clearing(required_protocol_account_kinds(
            *b"SFLP", SFLP_CID, None
        )));
    }
}
mod institution_asset {

    use crate::institution_asset::*;

    #[test]
    fn default_asset_allows_all_actions() {
        let account = [7u8; 32];
        for action in [
            InstitutionAssetAction::InstitutionCreateFunding,
            InstitutionAssetAction::MultisigTransferExecute,
            InstitutionAssetAction::MultisigCloseExecute,
            InstitutionAssetAction::OffchainFeeSweepExecute,
            InstitutionAssetAction::NrcSafetyFundTransfer,
            InstitutionAssetAction::L3DepositIn,
            InstitutionAssetAction::L3WithdrawOut,
            InstitutionAssetAction::L2ClearingDebit,
        ] {
            assert!(<() as InstitutionAsset<[u8; 32]>>::can_spend(
                &account, action
            ));
        }
    }
}
