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

#[test]
fn wallet_upgrade_repairs_missing_state_once_without_rewriting_rosters() {
    use frame_support::traits::Hooks;
    new_test_ext().execute_with(|| {
        let code = code_bytes("SFLP");
        let cid = AdminCidNumber::try_from(wallet_fixture_cid(code, 77)).unwrap();
        let roster = InstitutionAdmins::<AdminsOf<Test>> {
            institution_code: code,
            admins: admins(3).try_into().unwrap(),
        };
        AdminAccounts::<Test>::insert(&cid, &roster);
        StorageVersion::new(0).put::<PrivateAdmins>();
        frame_system::Account::<Test>::mutate(account(0), |info| {
            info.nonce = 7;
            info.providers = 1;
        });
        let accounts = (0..3)
            .map(|n| frame_system::Account::<Test>::get(account(n)))
            .collect::<Vec<_>>();
        assert_eq!(
            PrivateAdmins::resolve_admin_account(code, &cid, &account(0)),
            None
        );
        <PrivateAdmins as Hooks<u64>>::on_runtime_upgrade();
        assert_eq!(
            StorageVersion::get::<PrivateAdmins>(),
            StorageVersion::new(1)
        );
        assert_eq!(AdminAccounts::<Test>::get(&cid), Some(roster));
        for n in 0..3 {
            let wallet = account(n);
            assert_eq!(
                AdminWallets::<Test>::get(&cid, &wallet),
                Some(InstitutionAdminWalletState {
                    wallet: Some(wallet.clone()),
                    revision: 0
                })
            );
            assert_eq!(
                PrivateAdmins::resolve_admin_account(code, &cid, &wallet),
                Some(wallet.clone())
            );
            assert_eq!(
                PrivateAdmins::resolve_role_authorization(code, &cid, &wallet),
                Some(wallet.clone())
            );
            let info = frame_system::Account::<Test>::get(&wallet);
            assert_eq!(info.nonce, accounts[n as usize].nonce);
            assert_eq!(info.providers, accounts[n as usize].providers);
            assert_eq!(info.sufficients, accounts[n as usize].sufficients + 1);
        }
        let state = sp_io::storage::root(sp_runtime::StateVersion::V1);
        <PrivateAdmins as Hooks<u64>>::on_runtime_upgrade();
        assert_eq!(sp_io::storage::root(sp_runtime::StateVersion::V1), state);
    });
}

#[test]
fn wallet_upgrade_preserves_existing_versions_references_and_empty_state() {
    use frame_support::traits::Hooks;
    for populated in [false, true] {
        new_test_ext().execute_with(|| {
            let code = code_bytes("SFLP");
            let cid = AdminCidNumber::try_from(wallet_fixture_cid(code, 78)).unwrap();
            StorageVersion::new(0).put::<PrivateAdmins>();
            if populated {
                AdminAccounts::<Test>::insert(
                    &cid,
                    InstitutionAdmins::<AdminsOf<Test>> {
                        institution_code: code,
                        admins: admins(2).try_into().unwrap(),
                    },
                );
                AdminWallets::<Test>::insert(
                    &cid,
                    account(0),
                    InstitutionAdminWalletState {
                        wallet: Some(account(0)),
                        revision: 12,
                    },
                );
                WalletRoleBindings::<Test>::insert(&cid, account(0), account(99));
                frame_system::Account::<Test>::mutate(account(0), |info| {
                    info.nonce = 9;
                    info.sufficients = 3;
                });
            }
            let original = frame_system::Account::<Test>::get(account(0));
            <PrivateAdmins as Hooks<u64>>::on_runtime_upgrade();
            assert_eq!(
                StorageVersion::get::<PrivateAdmins>(),
                StorageVersion::new(1)
            );
            assert_eq!(frame_system::Account::<Test>::get(account(0)), original);
            if populated {
                assert_eq!(
                    AdminWallets::<Test>::get(&cid, account(0))
                        .unwrap()
                        .revision,
                    12
                );
                assert_eq!(
                    WalletRoleBindings::<Test>::get(&cid, account(0)),
                    Some(account(99))
                );
                assert_eq!(
                    AdminWallets::<Test>::get(&cid, account(1))
                        .unwrap()
                        .revision,
                    0
                );
            } else {
                assert!(!AdminWallets::<Test>::iter().any(|_| true));
            }
        });
    }
}

#[test]
fn wallet_upgrade_rolls_back_incomplete_records_and_reference_overflow() {
    use frame_support::traits::Hooks;
    for overflow in [false, true] {
        new_test_ext().execute_with(|| {
            let code = code_bytes("SFLP");
            let cid = AdminCidNumber::try_from(wallet_fixture_cid(code, 79)).unwrap();
            AdminAccounts::<Test>::insert(
                &cid,
                InstitutionAdmins::<AdminsOf<Test>> {
                    institution_code: code,
                    admins: admins(2).try_into().unwrap(),
                },
            );
            StorageVersion::new(0).put::<PrivateAdmins>();
            if overflow {
                frame_system::Account::<Test>::mutate(account(1), |info| info.providers = u32::MAX);
            } else {
                WalletRoleBindings::<Test>::insert(&cid, account(1), account(1));
            }
            let original = sp_io::storage::root(sp_runtime::StateVersion::V1);
            <PrivateAdmins as Hooks<u64>>::on_runtime_upgrade();
            assert_eq!(
                StorageVersion::get::<PrivateAdmins>(),
                StorageVersion::new(0)
            );
            assert_eq!(sp_io::storage::root(sp_runtime::StateVersion::V1), original);
        });
    }
}
