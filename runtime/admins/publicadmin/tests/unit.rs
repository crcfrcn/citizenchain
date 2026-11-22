//! 验证公权管理员在创世和运行阶段的资料要求及换绑授权。

use super::*;

#[test]
fn public_admins_allow_temporarily_empty_identity_and_name_fields() {
    new_test_ext().execute_with(|| {
        let mut input = admins(3);
        input[0].family_name = Default::default();
        input[0].given_name = Default::default();
        assert_ok!(PublicAdmins::do_set_institution_admins(
            b"GD001-CGOV0-923456789-2026".to_vec(),
            code_bytes("CGOV"),
            AdminAccountKind::PublicInstitution,
            input,
        ));
        let cid =
            AdminCidNumber::try_from(b"GD001-CGOV0-923456789-2026".to_vec()).expect("cid fits");
        let stored = AdminAccounts::<Test>::get(cid).expect("admins exist");
        assert!(stored.admins[0].cid_number.is_empty());
        assert!(stored.admins[0].family_name.is_empty());
        assert!(stored.admins[0].given_name.is_empty());
    });
}

#[test]
fn operation_phase_requires_complete_public_admin_fields() {
    new_test_ext().execute_with(|| {
        set_operation_phase(true);
        // 运行期:空 cid/姓名 → 拒。
        assert_noop!(
            PublicAdmins::do_set_institution_admins(
                b"GD001-CGOV0-923456789-2026".to_vec(),
                code_bytes("CGOV"),
                AdminAccountKind::PublicInstitution,
                admins(1),
            ),
            Error::<Test>::IncompleteAdminFields
        );
        // 运行期:四要素齐全 + identity 绑定匹配 → Ok。
        let mut complete = admins(1);
        complete[0].account_id = account(9);
        complete[0].cid_number = b"CN220-CTZN2-198805200-2026"
            .to_vec()
            .try_into()
            .expect("cid fits");
        complete[0].family_name = "张".as_bytes().to_vec().try_into().expect("family fits");
        complete[0].given_name = "三".as_bytes().to_vec().try_into().expect("given fits");
        assert_ok!(PublicAdmins::do_set_institution_admins(
            b"GD001-CGOV0-923456789-2026".to_vec(),
            code_bytes("CGOV"),
            AdminAccountKind::PublicInstitution,
            complete,
        ));
    });
}

#[test]
fn initialized_wallet_does_not_follow_personal_cid_account_after_rebind() {
    new_test_ext().execute_with(|| {
        set_operation_phase(true);
        let institution_cid_number = b"GD001-CGOV0-623456789-2026".to_vec();
        let citizen_cid_number = b"CN220-CTZN2-198805200-2026";
        let mut complete = admins(1);
        complete[0].account_id = account(9);
        complete[0].cid_number = citizen_cid_number
            .to_vec()
            .try_into()
            .expect("citizen cid fits");
        complete[0].family_name = "张".as_bytes().to_vec().try_into().expect("family fits");
        complete[0].given_name = "三".as_bytes().to_vec().try_into().expect("given fits");
        assert_ok!(PublicAdmins::do_set_institution_admins(
            institution_cid_number.clone(),
            code_bytes("CGOV"),
            AdminAccountKind::PublicInstitution,
            complete,
        ));

        // 换绑前，当前账户解析为名册中的规范 account_id。
        assert_eq!(
            PublicAdmins::resolve_admin_account(
                code_bytes("CGOV"),
                &institution_cid_number,
                &account(9),
            ),
            Some(account(9))
        );

        // 个人CID换绑不改变已初始化机构钱包。
        set_current_account_id_seed(10);
        assert_eq!(
            PublicAdmins::resolve_admin_account(
                code_bytes("CGOV"),
                &institution_cid_number,
                &account(10),
            ),
            None
        );
        assert_eq!(
            PublicAdmins::resolve_admin_account(
                code_bytes("CGOV"),
                &institution_cid_number,
                &account(9),
            ),
            Some(account(9))
        );
    });
}

#[test]
fn public_admin_cid_must_exist_but_institution_wallet_is_independent() {
    new_test_ext().execute_with(|| {
        let mut valid = admins(1);
        valid[0].account_id = account(9);
        valid[0].cid_number = b"CN220-CTZN2-198805200-2026"
            .to_vec()
            .try_into()
            .expect("citizen cid fits");
        assert_ok!(PublicAdmins::do_set_institution_admins(
            b"GD001-CGOV0-823456789-2026".to_vec(),
            code_bytes("CGOV"),
            AdminAccountKind::PublicInstitution,
            valid.clone(),
        ));

        valid[0].cid_number = wallet_fixture_cid(code_bytes("CTZN"), 7).try_into().unwrap();
        assert_noop!(
            PublicAdmins::do_set_institution_admins(
                b"GD001-CGOV0-723456789-2026".to_vec(),
                code_bytes("CGOV"),
                AdminAccountKind::PublicInstitution,
                valid,
            ),
            Error::<Test>::CitizenIdentityMismatch
        );
    });
}

#[test]
fn public_admins_accept_public_codes_and_reject_private_codes() {
    new_test_ext().execute_with(|| {
        let public_cid = b"GD001-CGOV0-123456789-2026".to_vec();
        assert_ok!(PublicAdmins::do_set_institution_admins(
            public_cid.clone(),
            code_bytes("CGOV"),
            AdminAccountKind::PublicInstitution,
            admins(3),
        ));
        let public_key: AdminCidNumber = public_cid.try_into().expect("cid fits");
        let stored = AdminAccounts::<Test>::get(public_key).expect("public admins exist");
        assert_eq!(stored.institution_code, code_bytes("CGOV"));
        assert_eq!(stored.admins.len(), 3);

        assert_ok!(PublicAdmins::do_set_institution_admins(
            b"GD001-UNIN0-223456789-2026".to_vec(),
            code_bytes("UNIN"),
            AdminAccountKind::PublicInstitution,
            admins(2),
        ));

        assert_noop!(
            PublicAdmins::do_set_institution_admins(
                b"GD001-SFLP0-323456789-2026".to_vec(),
                code_bytes("SFLP"),
                AdminAccountKind::PublicInstitution,
                admins(3),
            ),
            Error::<Test>::InvalidAdminAccountKind
        );
    });
}

#[test]
fn public_admins_activate_and_query_active_admins() {
    new_test_ext().execute_with(|| {
        let cid = b"GD001-CGOV0-423456789-2026".to_vec();
        assert_ok!(PublicAdmins::do_set_institution_admins(
            cid.clone(),
            code_bytes("CGOV"),
            AdminAccountKind::PublicInstitution,
            admins(3),
        ));

        assert!(PublicAdmins::is_institution_admin(
            code_bytes("CGOV"),
            &cid,
            &account(0)
        ));
        assert_eq!(
            PublicAdmins::institution_admins_len(code_bytes("CGOV"), &cid),
            Some(3)
        );
    });
}

#[test]
fn public_admins_fix_size_only_for_full_genesis_identity() {
    new_test_ext().execute_with(|| {
        let fixed = primitives::governance_skeleton::fixed_institutions()
            .into_iter()
            .find(|institution| institution.code == code_bytes("NRC"))
            .expect("NRC genesis identity");
        assert_noop!(
            PublicAdmins::do_set_institution_admins(
                fixed.cid_number.as_bytes().to_vec(),
                code_bytes("NRC"),
                AdminAccountKind::PublicInstitution,
                admins(3),
            ),
            Error::<Test>::InvalidAdminsLen
        );

        assert_ok!(PublicAdmins::do_set_institution_admins(
            fixed.cid_number.as_bytes().to_vec(),
            code_bytes("NRC"),
            AdminAccountKind::PublicInstitution,
            admins(NRC_ADMIN_COUNT as u8),
        ));

        // 机构码相同但不是创世 CID 时，不得扩大固定人数保护范围。
        assert_ok!(PublicAdmins::do_set_institution_admins(
            b"LN001-NRC0G-123456789-2026".to_vec(),
            code_bytes("NRC"),
            AdminAccountKind::PublicInstitution,
            admins(3),
        ));
    });
}

#[test]
fn permanent_member_body_protection_is_scoped_to_genesis_cid() {
    new_test_ext().execute_with(|| {
        let spec = primitives::institution_constraints::member_composition_specs()[0];
        assert_noop!(
            PublicAdmins::do_set_institution_admins(
                spec.institution.cid_number.as_bytes().to_vec(),
                spec.institution.code,
                AdminAccountKind::PublicInstitution,
                indexed_admins(spec.min_members - 1),
            ),
            Error::<Test>::InvalidAdminsLen
        );
        assert_ok!(PublicAdmins::do_set_institution_admins(
            spec.institution.cid_number.as_bytes().to_vec(),
            spec.institution.code,
            AdminAccountKind::PublicInstitution,
            indexed_admins(spec.min_members),
        ));
        // 固定创世身份只保护完整 CID；不得把保护扩大到同机构码的其它机构。
        assert_ok!(PublicAdmins::do_set_institution_admins(
            b"LN001-NLG0G-123456789-2026".to_vec(),
            spec.institution.code,
            AdminAccountKind::PublicInstitution,
            admins(3),
        ));
    });
}

#[test]
fn member_body_admins_are_registered_without_owning_institution_threshold() {
    new_test_ext().execute_with(|| {
        let spec = primitives::institution_constraints::member_composition_specs()[0];
        let members = indexed_admins(spec.min_members);
        assert_ok!(PublicAdmins::do_set_institution_admins(
            spec.institution.cid_number.as_bytes().to_vec(),
            spec.institution.code,
            AdminAccountKind::PublicInstitution,
            members.clone(),
        ));
        assert_eq!(
            AdminAccounts::<Test>::get(
                AdminCidNumber::try_from(spec.institution.cid_number.as_bytes().to_vec())
                    .expect("cid fits")
            )
            .expect("independently registered admins exist")
            .admins
            .to_vec(),
            members
        );
    });
}

#[test]
fn fixed_governance_admin_update_only_replaces_people_records() {
    new_test_ext().execute_with(|| {
        let fixed = primitives::governance_skeleton::fixed_institutions()
            .into_iter()
            .find(|institution| institution.code == code_bytes("NRC"))
            .expect("NRC genesis identity");
        let code = fixed.code;
        let cid_number: AdminCidNumber = fixed
            .cid_number
            .as_bytes()
            .to_vec()
            .try_into()
            .expect("cid fits");
        assert_ok!(PublicAdmins::do_set_institution_admins(
            cid_number.to_vec(),
            code,
            AdminAccountKind::PublicInstitution,
            admins(NRC_ADMIN_COUNT as u8),
        ));

        let replacement = (40..40 + NRC_ADMIN_COUNT as u8)
            .map(|seed| Admin {
                account_id: account(seed),
                cid_number: Default::default(),
                family_name: Default::default(),
                given_name: Default::default(),
            })
            .collect::<Vec<_>>();
        assert_ok!(PublicAdmins::do_set_institution_admins(
            cid_number.to_vec(),
            code,
            AdminAccountKind::PublicInstitution,
            replacement.clone(),
        ));

        assert_eq!(
            AdminAccounts::<Test>::get(cid_number.clone())
                .expect("fixed admin account remains")
                .admins
                .to_vec(),
            replacement
        );
    });
}

#[test]
fn institution_wallet_foundation_covers_existing_institution_code_catalog() {
    use codec::Encode;
    new_test_ext().execute_with(|| {
        let mut covered = 0;
        for info in primitives::cid::code::INSTITUTION_CODE_INFOS {
            let code = info.institution_code;
            if !can_store_public_admin_code(&code)
                || primitives::cid::code::is_fixed_governance_code(&code)
            {
                continue;
            }
            let cid = wallet_fixture_cid(code, 1);
            let anchors = wallet_roster_fixture(code, &cid, 2);
            let key = AdminCidNumber::try_from(cid.clone()).unwrap();
            let before = AdminAccounts::<Test>::get(&key).unwrap().encode();
            assert_eq!(
                PublicAdmins::current_institution_wallet(code, &cid, &anchors[0]),
                None
            );
            initialize_wallet_roster(code, &cid, &anchors);
            assert_ok!(PublicAdmins::change_institution_wallet(
                code,
                cid.clone(),
                wallet_change(&anchors[0], anchors[0].clone(), account(200), 0)
            ));
            assert_eq!(
                PublicAdmins::current_institution_wallet(code, &cid, &anchors[0]),
                Some(account(200))
            );
            assert_eq!(
                PublicAdmins::institution_wallet_state(code, &cid, &anchors[0])
                    .unwrap()
                    .revision,
                1
            );
            assert_eq!(AdminAccounts::<Test>::get(&key).unwrap().encode(), before);
            assert_eq!(
                PublicAdmins::institution_admins(code, &cid),
                Some(anchors.clone())
            );
            // 原授权入口仍使用原来的规则；新的基础钱包尚未替换它。
            assert_eq!(
                PublicAdmins::resolve_admin_account(code, &cid, &anchors[0]),
                Some(anchors[0].clone())
            );
            assert_eq!(
                PublicAdmins::resolve_admin_account(code, &cid, &account(200)),
                None
            );
            covered += 1;
        }
        assert!(covered > 5);
    });
}

#[test]
fn institution_wallet_foundation_is_scoped_to_institution_and_preserves_old_indices() {
    use codec::Encode;
    new_test_ext().execute_with(|| {
        let code = code_bytes("CGOV");
        let first = wallet_fixture_cid(code, 1);
        let second = wallet_fixture_cid(code, 2);
        let anchors = wallet_roster_fixture(code, &first, 1);
        wallet_roster_fixture(code, &second, 1);
        initialize_wallet_roster(code, &first, &anchors);
        initialize_wallet_roster(code, &second, &anchors);
        assert_ok!(PublicAdmins::change_institution_wallet(
            code,
            first.clone(),
            wallet_change(&anchors[0], anchors[0].clone(), account(201), 0)
        ));
        assert_eq!(
            PublicAdmins::current_institution_wallet(code, &second, &anchors[0]),
            Some(anchors[0].clone())
        );
        assert_eq!(
            PublicAdmins::institution_wallet_state(code, &second, &anchors[0])
                .unwrap()
                .revision,
            0
        );
        assert_eq!(
            PublicAdmins::institution_wallet_state(code_bytes("MFA"), &first, &anchors[0]),
            None
        );
        assert_eq!(Error::<Test>::InvalidInstitution.encode()[0], 0);
        let key = AdminCidNumber::try_from(first).unwrap();
        assert_eq!(
            Event::<Test>::InstitutionAdminsSet {
                cid_number: key,
                institution_code: code,
                admins_len: 1,
                created: true
            }
            .encode()[0],
            0
        );
    });
}

#[test]
fn institution_wallet_foundation_preserves_all_fixed_governance_rosters() {
    use codec::Encode;
    new_test_ext().execute_with(|| {
        let mut codes = BTreeSet::new();
        for institution in primitives::governance_skeleton::fixed_institutions() {
            let cid = institution.cid_number.as_bytes().to_vec();
            let anchors = wallet_roster_fixture(institution.code, &cid, institution.expected_len);
            let key = AdminCidNumber::try_from(cid.clone()).unwrap();
            let before = AdminAccounts::<Test>::get(&key).unwrap().encode();
            initialize_wallet_roster(institution.code, &cid, &anchors);
            assert_ok!(PublicAdmins::change_institution_wallet(
                institution.code,
                cid.clone(),
                wallet_change(&anchors[0], anchors[0].clone(), account(250), 0)
            ));
            assert_eq!(
                PublicAdmins::institution_admins(institution.code, &cid),
                Some(anchors)
            );
            assert_eq!(
                PublicAdmins::institution_admins_len(institution.code, &cid),
                Some(institution.expected_len)
            );
            assert_eq!(AdminAccounts::<Test>::get(&key).unwrap().encode(), before);
            codes.insert(institution.code);
        }
        for text in ["NRC", "PRC", "PRB", "NJD", "FRG"] {
            assert!(codes.contains(&code_bytes(text)));
        }
    });
}

#[test]
fn legacy_authorization_is_frozen_atomically_on_first_use() {
    use codec::Encode;
    new_test_ext().execute_with(|| {
        let code = code_bytes("CGOV");
        let cid = wallet_fixture_cid(code, 8);
        let key = AdminCidNumber::try_from(cid.clone()).unwrap();
        let mut record = admins(1).remove(0);
        record.account_id = account(9);
        record.cid_number = b"CN220-CTZN2-198805200-2026".to_vec().try_into().unwrap();
        AdminAccounts::<Test>::insert(&key, InstitutionAdmins {institution_code: code, admins: vec![record].try_into().unwrap()});
        let before = AdminAccounts::<Test>::get(&key).unwrap().encode();
        set_operation_phase(true);
        set_current_account_id_seed(10);
        assert_eq!(PublicAdmins::resolve_admin_account(code, &cid, &account(10)), Some(account(9)));
        assert_ok!(PublicAdmins::ensure_institution_wallets(code, cid.clone()));
        assert!(PublicAdmins::wallets_initialized(&key));
        set_current_account_id_seed(11);
        assert_eq!(PublicAdmins::resolve_admin_account(code, &cid, &account(10)), Some(account(9)));
        assert_eq!(PublicAdmins::resolve_admin_account(code, &cid, &account(11)), None);
        assert_ok!(PublicAdmins::change_institution_wallet(code, cid.clone(), wallet_change(&account(9),account(10),account(77),0)));
        assert_eq!(PublicAdmins::resolve_admin_account(code, &cid, &account(77)), Some(account(9)));
        assert_eq!(PublicAdmins::resolve_admin_account(code, &cid, &account(10)), None);
        assert_eq!(AdminAccounts::<Test>::get(&key).unwrap().encode(), before);
    });
}


#[test]
fn activated_wallet_reverse_index_is_scoped_atomic_and_never_grants_duplicate_ownership() {
    new_test_ext().execute_with(|| {
        let code=code_bytes("CGOV");let cid=wallet_fixture_cid(code,66);
        let anchors=wallet_roster_fixture(code,&cid,2);let key=AdminCidNumber::try_from(cid.clone()).unwrap();
        assert_ok!(PublicAdmins::ensure_institution_wallets(code,cid.clone()));
        assert_eq!(WalletOwners::<Test>::get(&key,&anchors[0]),Some(anchors[0].clone()));
        assert_noop!(PublicAdmins::change_institution_wallet(code,cid.clone(),wallet_change(&anchors[0],anchors[0].clone(),anchors[1].clone(),0)),Error::<Test>::WalletDuplicate);
        assert_ok!(PublicAdmins::change_institution_wallet(code,cid.clone(),wallet_change(&anchors[0],anchors[0].clone(),account(201),0)));
        assert_eq!(WalletOwners::<Test>::get(&key,&anchors[0]),None);
        assert_eq!(WalletOwners::<Test>::get(&key,account(201)),Some(anchors[0].clone()));
        assert_eq!(PublicAdmins::resolve_admin_account(code,&cid,&account(201)),Some(anchors[0].clone()));
        assert_noop!(PublicAdmins::change_institution_wallet(code,cid.clone(),wallet_change(&anchors[1],anchors[1].clone(),account(201),0)),Error::<Test>::WalletDuplicate);
        assert_eq!(PublicAdmins::resolve_admin_account(code,&cid,&anchors[1]),Some(anchors[1].clone()));
    });
}
