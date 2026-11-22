//! 验证私权名册姓名规范化、机构分类、激活及查询。

use super::*;

#[test]
fn private_admins_normalize_missing_person_name_before_storage() {
    new_test_ext().execute_with(|| {
        let mut input = admins(2);
        input[0].family_name = Default::default();
        input[0].given_name = Default::default();
        assert_ok!(PrivateAdmins::do_set_institution_admins(
            b"GD001-SFLP0-923456789-2026".to_vec(),
            code_bytes("SFLP"),
            AdminAccountKind::PrivateInstitution,
            input,
        ));
        let cid =
            AdminCidNumber::try_from(b"GD001-SFLP0-923456789-2026".to_vec()).expect("cid fits");
        let stored = AdminAccounts::<Test>::get(cid).expect("admins exist");
        assert_eq!(stored.admins[0].family_name.as_slice(), "管理".as_bytes());
        assert_eq!(stored.admins[0].given_name.as_slice(), "员".as_bytes());
    });
}

#[test]
fn private_admins_accept_private_codes_and_private_owned_unincorporated_codes() {
    new_test_ext().execute_with(|| {
        let private_cid = b"GD001-SFLP0-123456789-2026".to_vec();
        assert_ok!(PrivateAdmins::do_set_institution_admins(
            private_cid.clone(),
            code_bytes("SFLP"),
            AdminAccountKind::PrivateInstitution,
            admins(3),
        ));
        let private_key: AdminCidNumber = private_cid.try_into().expect("cid fits");
        let stored = AdminAccounts::<Test>::get(private_key).expect("private admins exist");
        assert_eq!(stored.institution_code, code_bytes("SFLP"));
        assert_eq!(stored.admins.len(), 3);

        assert_ok!(PrivateAdmins::do_set_institution_admins(
            b"GD001-UNIN0-223456789-2026".to_vec(),
            code_bytes("UNIN"),
            AdminAccountKind::PrivateInstitution,
            admins(2),
        ));
    });
}

#[test]
fn private_admins_activate_and_query_active_admins() {
    new_test_ext().execute_with(|| {
        let cid = b"GD001-JSCH0-323456789-2026".to_vec();
        assert_ok!(PrivateAdmins::do_set_institution_admins(
            cid.clone(),
            code_bytes("JSCH"),
            AdminAccountKind::PrivateInstitution,
            admins(3),
        ));

        assert!(PrivateAdmins::is_institution_admin(
            code_bytes("JSCH"),
            &cid,
            &account(0)
        ));
        assert_eq!(
            PrivateAdmins::institution_admins_len(code_bytes("JSCH"), &cid),
            Some(3)
        );
    });
}

#[test]
fn private_admins_reject_public_codes() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            PrivateAdmins::do_set_institution_admins(
                b"LN001-PRS00-423456789-2026".to_vec(),
                code_bytes("PRS"),
                AdminAccountKind::PrivateInstitution,
                admins(3),
            ),
            Error::<Test>::InvalidAdminAccountKind
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
            if !can_store_private_admin_code(&code)
                || primitives::cid::code::is_fixed_governance_code(&code)
            {
                continue;
            }
            let cid = wallet_fixture_cid(code, 1);
            let anchors = wallet_roster_fixture(code, &cid, 2);
            let key = AdminCidNumber::try_from(cid.clone()).unwrap();
            let before = AdminAccounts::<Test>::get(&key).unwrap().encode();
            assert_eq!(
                PrivateAdmins::current_institution_wallet(code, &cid, &anchors[0]),
                None
            );
            initialize_wallet_roster(code, &cid, &anchors);
            assert_ok!(PrivateAdmins::change_institution_wallet(
                code,
                cid.clone(),
                wallet_change(&anchors[0], anchors[0].clone(), account(200), 0)
            ));
            assert_eq!(
                PrivateAdmins::current_institution_wallet(code, &cid, &anchors[0]),
                Some(account(200))
            );
            assert_eq!(
                PrivateAdmins::institution_wallet_state(code, &cid, &anchors[0])
                    .unwrap()
                    .revision,
                1
            );
            assert_eq!(AdminAccounts::<Test>::get(&key).unwrap().encode(), before);
            assert_eq!(
                PrivateAdmins::institution_admins(code, &cid),
                Some(anchors.clone())
            );
            // 原授权入口仍使用原来的规则；新的基础钱包尚未替换它。
            assert_eq!(
                PrivateAdmins::resolve_admin_account(code, &cid, &anchors[0]),
                Some(anchors[0].clone())
            );
            assert_eq!(
                PrivateAdmins::resolve_admin_account(code, &cid, &account(200)),
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
        let code = code_bytes("SFLP");
        let first = wallet_fixture_cid(code, 1);
        let second = wallet_fixture_cid(code, 2);
        let anchors = wallet_roster_fixture(code, &first, 1);
        wallet_roster_fixture(code, &second, 1);
        initialize_wallet_roster(code, &first, &anchors);
        initialize_wallet_roster(code, &second, &anchors);
        assert_ok!(PrivateAdmins::change_institution_wallet(
            code,
            first.clone(),
            wallet_change(&anchors[0], anchors[0].clone(), account(201), 0)
        ));
        assert_eq!(
            PrivateAdmins::current_institution_wallet(code, &second, &anchors[0]),
            Some(anchors[0].clone())
        );
        assert_eq!(
            PrivateAdmins::institution_wallet_state(code, &second, &anchors[0])
                .unwrap()
                .revision,
            0
        );
        assert_eq!(
            PrivateAdmins::institution_wallet_state(code_bytes("SFGQ"), &first, &anchors[0]),
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
fn activated_wallet_reverse_index_is_scoped_atomic_and_never_grants_duplicate_ownership() {
    new_test_ext().execute_with(|| {
        let code=code_bytes("SFGY");let cid=wallet_fixture_cid(code,66);
        let anchors=wallet_roster_fixture(code,&cid,2);let key=AdminCidNumber::try_from(cid.clone()).unwrap();
        assert_ok!(PrivateAdmins::ensure_institution_wallets(code,cid.clone()));
        assert_eq!(WalletOwners::<Test>::get(&key,&anchors[0]),Some(anchors[0].clone()));
        assert_noop!(PrivateAdmins::change_institution_wallet(code,cid.clone(),wallet_change(&anchors[0],anchors[0].clone(),anchors[1].clone(),0)),Error::<Test>::WalletDuplicate);
        assert_ok!(PrivateAdmins::change_institution_wallet(code,cid.clone(),wallet_change(&anchors[0],anchors[0].clone(),account(201),0)));
        assert_eq!(WalletOwners::<Test>::get(&key,&anchors[0]),None);
        assert_eq!(WalletOwners::<Test>::get(&key,account(201)),Some(anchors[0].clone()));
        assert_eq!(PrivateAdmins::resolve_admin_account(code,&cid,&account(201)),Some(anchors[0].clone()));
        assert_noop!(PrivateAdmins::change_institution_wallet(code,cid.clone(),wallet_change(&anchors[1],anchors[1].clone(),account(201),0)),Error::<Test>::WalletDuplicate);
        assert_eq!(PrivateAdmins::resolve_admin_account(code,&cid,&anchors[1]),Some(anchors[1].clone()));
    });
}
