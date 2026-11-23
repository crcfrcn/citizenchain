//! 私权生命周期入口和读取接口共享同一CID名册，替换后旧人员立即失权。
use super::*;

#[test]
fn private_lifecycle_replaces_roster_and_normalizes_names_once() {
    new_test_ext().execute_with(|| {
        let cid = b"GD001-SFLP0-923456789-2026".to_vec();
        let code = code_bytes("SFLP");
        assert_ok!(PrivateAdmins::update_institution_wallets(cid.clone(),code,admins(2),vec![],vec![]));
        let mut next = admins(1);
        next[0].account_id = account(9);
        next[0].family_name = Default::default();
        next[0].given_name = Default::default();
        assert_ok!(PrivateAdmins::update_institution_wallets(cid.clone(),code,next,vec![(account(0),0),(account(1),0)],vec![]));
        assert_eq!(
            PrivateAdmins::institution_admins(code, &cid),
            Some(vec![account(9)])
        );
        assert!(!PrivateAdmins::is_institution_admin(
            code,
            &cid,
            &account(0)
        ));
        assert_eq!(
            PrivateAdmins::resolve_admin_account(code, &cid, &account(9)),
            Some(account(9))
        );
        let stored = AdminAccounts::<Test>::get(AdminCidNumber::try_from(cid).unwrap()).unwrap();
        assert_eq!(stored.admins[0].family_name.as_slice(), "管理".as_bytes());
        assert_eq!(stored.admins[0].given_name.as_slice(), "员".as_bytes());
    });
}

#[test]
fn actual_wallet_roundtrip_preserves_each_address_history_and_role_reference() {
    new_test_ext().execute_with(|| {
        let code = code_bytes("SFLP");
        let cid = wallet_fixture_cid(code, 6);
        let wallets = wallet_roster_fixture(code, &cid, 2);
        let old = wallets[0].clone();
        let new = account(200);
        let first = wallet_change(&old, old.clone(), new.clone(), 0);
        assert_ok!(PrivateAdmins::change_institution_wallet(
            code,
            cid.clone(),
            first.clone()
        ));
        assert_noop!(
            PrivateAdmins::change_institution_wallet(code, cid.clone(), first),
            Error::<Test>::WalletAdminNotFound
        );
        assert_ok!(PrivateAdmins::change_institution_wallet(
            code,
            cid.clone(),
            wallet_change(&new, new.clone(), old.clone(), 0)
        ));
        assert_eq!(
            PrivateAdmins::institution_wallet_state(code, &cid, &old),
            Some(InstitutionAdminWalletState {
                wallet: Some(old.clone()),
                revision: 2
            })
        );
        assert_eq!(
            PrivateAdmins::institution_wallet_state(code, &cid, &new),
            Some(InstitutionAdminWalletState {
                wallet: None,
                revision: 1
            })
        );
        assert_eq!(
            PrivateAdmins::resolve_role_authorization(code, &cid, &old),
            Some(old.clone())
        );
        assert_eq!(
            PrivateAdmins::resolve_role_authorization(code, &cid, &new),
            None
        );
        assert_noop!(
            PrivateAdmins::change_institution_wallet(
                code,
                cid.clone(),
                wallet_change(&old, old.clone(), account(201), 0)),
            Error::<Test>::WalletStaleRevision
        );
        let key = AdminCidNumber::try_from(cid.clone()).unwrap();
        let original = AdminAccounts::<Test>::get(&key).unwrap();
        let remaining = vec![original.admins[1].clone()];
        assert_ok!(PrivateAdmins::do_set_institution_admins(
            cid.clone(),
            code,
            AdminAccountKind::PrivateInstitution,
            remaining
        ));
        assert_eq!(AdminWallets::<Test>::get(&key, &old).unwrap().revision, 3);
        assert!(!WalletRoleBindings::<Test>::contains_key(&key, &old));
        assert_ok!(PrivateAdmins::do_set_institution_admins(
            cid.clone(),
            code,
            AdminAccountKind::PrivateInstitution,
            original.admins.to_vec()
        ));
        assert_eq!(AdminWallets::<Test>::get(&key, &old).unwrap().revision, 4);
        assert_eq!(
            AdminWallets::<Test>::get(&key, &wallets[1])
                .unwrap()
                .revision,
            0
        );
    });
}

#[test]
fn wallet_change_does_not_write_existing_voter_snapshot() {
    use engingcore::types::{AuthorizationSubject, CidNumber, RoleCode, RoleSubject};
    new_test_ext().execute_with(|| {
        let code = code_bytes("SFLP");
        let cid = wallet_fixture_cid(code, 7);
        let wallets = wallet_roster_fixture(code, &cid, 1);
        let subject = AuthorizationSubject::Institution(RoleSubject {
            cid_number: CidNumber::try_from(cid.clone()).unwrap(),
            role_code: RoleCode::try_from(b"FIXTURE_ROLE".to_vec()).unwrap(),
        });
        let voters: BoundedVec<AccountId32, <Test as engingcore::Config>::MaxAdminsPerInstitution> =
            wallets.clone().try_into().unwrap();
        engingcore::VoterSnapshot::<Test>::insert(42u128, &subject, &voters);
        let before = sp_io::storage::root(sp_runtime::StateVersion::V1);
        assert_ok!(PrivateAdmins::change_institution_wallet(
            code,
            cid.clone(),
            wallet_change(&wallets[0], wallets[0].clone(), account(202), 0)
        ));
        assert_ne!(sp_io::storage::root(sp_runtime::StateVersion::V1), before);
        assert_eq!(
            engingcore::VoterSnapshot::<Test>::get(42, &subject),
            Some(voters)
        );
    });
}

#[test]
fn full_roster_removal_keeps_retired_address_history_on_recreation() {
    new_test_ext().execute_with(|| {
        let code = code_bytes("SFLP");
        let cid = wallet_fixture_cid(code, 10);
        let wallets = wallet_roster_fixture(code, &cid, 1);
        let key = AdminCidNumber::try_from(cid.clone()).unwrap();
        AdminAccounts::<Test>::remove(&key);
        assert_noop!(
            PrivateAdmins::retire_institution_wallet(code, cid.clone(), wallets[0].clone(), 1),
            Error::<Test>::WalletStaleRevision
        );
        assert_ok!(PrivateAdmins::retire_institution_wallet(
            code,
            cid.clone(),
            wallets[0].clone(),
            0
        ));
        assert_eq!(
            AdminWallets::<Test>::get(&key, &wallets[0]),
            Some(InstitutionAdminWalletState {
                wallet: None,
                revision: 1
            })
        );
        wallet_roster_fixture(code, &cid, 1);
        assert_eq!(
            PrivateAdmins::current_institution_wallet(code, &cid, &wallets[0]),
            Some(wallets[0].clone())
        );
        assert_eq!(
            AdminWallets::<Test>::get(&key, &wallets[0])
                .unwrap()
                .revision,
            2
        );
    });
}
