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
fn institution_wallet_registration_and_change_cover_code_catalog() {
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
            let wallets = wallet_roster_fixture(code, &cid, 2);
            let key = AdminCidNumber::try_from(cid.clone()).unwrap();
            let before = AdminAccounts::<Test>::get(&key).unwrap();
            assert_eq!(
                PrivateAdmins::resolve_admin_account(code, &cid, &wallets[0]),
                Some(wallets[0].clone())
            );
            assert_ok!(PrivateAdmins::change_institution_wallet(
                code,
                cid.clone(),
                wallet_change(&wallets[0], wallets[0].clone(), account(200), 0)
            ));
            let after = AdminAccounts::<Test>::get(&key).unwrap();
            assert_eq!(after.admins.len(), before.admins.len());
            let mut expected = before;
            expected.admins[0].account_id = account(200);
            assert_eq!(after, expected);
            assert_eq!(
                PrivateAdmins::resolve_admin_account(code, &cid, &wallets[0]),
                None
            );
            assert_eq!(
                PrivateAdmins::resolve_admin_account(code, &cid, &account(200)),
                Some(account(200))
            );
            assert_eq!(
                PrivateAdmins::resolve_role_authorization(code, &cid, &account(200)),
                Some(wallets[0].clone())
            );
            assert_eq!(
                PrivateAdmins::institution_wallet_state(code, &cid, &wallets[0]),
                Some(InstitutionAdminWalletState {
                    wallet: None,
                    revision: 1
                })
            );
            assert_eq!(
                PrivateAdmins::institution_wallet_state(code, &cid, &account(200)),
                Some(InstitutionAdminWalletState {
                    wallet: Some(account(200)),
                    revision: 0
                })
            );
            covered += 1;
        }
        assert!(covered > 5);
    });
}

#[test]
fn institution_wallet_versions_and_bindings_are_scoped_to_full_cid() {
    use codec::Encode;
    new_test_ext().execute_with(|| {
        let code = code_bytes("SFLP");
        let first = wallet_fixture_cid(code, 1);
        let second = wallet_fixture_cid(code, 2);
        let wallets = wallet_roster_fixture(code, &first, 1);
        wallet_roster_fixture(code, &second, 1);
        assert_ok!(PrivateAdmins::change_institution_wallet(
            code,
            first.clone(),
            wallet_change(&wallets[0], wallets[0].clone(), account(201), 0)
        ));
        assert_eq!(
            PrivateAdmins::current_institution_wallet(code, &second, &wallets[0]),
            Some(wallets[0].clone())
        );
        assert_eq!(
            PrivateAdmins::institution_wallet_state(code, &second, &wallets[0])
                .unwrap()
                .revision,
            0
        );
        assert_eq!(
            PrivateAdmins::resolve_admin_account(code, &second, &account(201)),
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
