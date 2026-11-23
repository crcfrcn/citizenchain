//! 通过所属公开生命周期trait写入，查询必须立即反映完整名册替换。
use super::*;

#[test]
fn public_lifecycle_and_queries_share_one_roster() {
    new_test_ext().execute_with(|| {
        let cid = b"GD001-CGOV0-923456789-2026".to_vec();
        let code = code_bytes("CGOV");
        assert_ok!(PublicAdmins::update_institution_wallets(cid.clone(),code,admins(2),vec![],vec![]));
        assert_eq!(PublicAdmins::institution_admins_len(code, &cid), Some(2));
        assert!(PublicAdmins::is_institution_admin(code, &cid, &account(0)));
        let mut next = admins(1);
        next[0].account_id = account(7);
        assert_ok!(PublicAdmins::update_institution_wallets(cid.clone(),code,next,vec![(account(0),0),(account(1),0)],vec![]));
        assert_eq!(
            PublicAdmins::institution_admins(code, &cid),
            Some(vec![account(7)])
        );
        assert!(!PublicAdmins::is_institution_admin(code, &cid, &account(0)));
        assert_eq!(
            PublicAdmins::resolve_admin_account(code, &cid, &account(7)),
            Some(account(7))
        );
        assert_eq!(
            PublicAdmins::institution_admins(code_bytes("SFLP"), &cid),
            None
        );
    });
}

#[test]
fn actual_wallet_roundtrip_preserves_each_address_history_and_role_reference() {
    new_test_ext().execute_with(|| {
        let code = code_bytes("CGOV");
        let cid = wallet_fixture_cid(code, 6);
        let wallets = wallet_roster_fixture(code, &cid, 2);
        let old = wallets[0].clone();
        let new = account(200);
        let first = wallet_change(&old, old.clone(), new.clone(), 0);
        assert_ok!(PublicAdmins::change_institution_wallet(
            code,
            cid.clone(),
            first.clone()
        ));
        assert_noop!(
            PublicAdmins::change_institution_wallet(code, cid.clone(), first),
            Error::<Test>::WalletAdminNotFound
        );
        assert_ok!(PublicAdmins::change_institution_wallet(
            code,
            cid.clone(),
            wallet_change(&new, new.clone(), old.clone(), 0)
        ));
        assert_eq!(
            PublicAdmins::institution_wallet_state(code, &cid, &old),
            Some(InstitutionAdminWalletState {
                wallet: Some(old.clone()),
                revision: 2
            })
        );
        assert_eq!(
            PublicAdmins::institution_wallet_state(code, &cid, &new),
            Some(InstitutionAdminWalletState {
                wallet: None,
                revision: 1
            })
        );
        assert_eq!(
            PublicAdmins::resolve_role_authorization(code, &cid, &old),
            Some(old.clone())
        );
        assert_eq!(
            PublicAdmins::resolve_role_authorization(code, &cid, &new),
            None
        );
        assert_noop!(
            PublicAdmins::change_institution_wallet(
                code,
                cid.clone(),
                wallet_change(&old, old.clone(), account(201), 0)),
            Error::<Test>::WalletStaleRevision
        );
        let key = AdminCidNumber::try_from(cid.clone()).unwrap();
        let original = AdminAccounts::<Test>::get(&key).unwrap();
        let remaining = vec![original.admins[1].clone()];
        assert_ok!(PublicAdmins::do_set_institution_admins(
            cid.clone(),
            code,
            AdminAccountKind::PublicInstitution,
            remaining
        ));
        assert_eq!(AdminWallets::<Test>::get(&key, &old).unwrap().revision, 3);
        assert!(!WalletRoleBindings::<Test>::contains_key(&key, &old));
        assert_ok!(PublicAdmins::do_set_institution_admins(
            cid.clone(),
            code,
            AdminAccountKind::PublicInstitution,
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
        let code = code_bytes("CGOV");
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
        assert_ok!(PublicAdmins::change_institution_wallet(
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
        let code = code_bytes("CGOV");
        let cid = wallet_fixture_cid(code, 10);
        let wallets = wallet_roster_fixture(code, &cid, 1);
        let key = AdminCidNumber::try_from(cid.clone()).unwrap();
        AdminAccounts::<Test>::remove(&key);
        assert_noop!(
            PublicAdmins::retire_institution_wallet(code, cid.clone(), wallets[0].clone(), 1),
            Error::<Test>::WalletStaleRevision
        );
        assert_ok!(PublicAdmins::retire_institution_wallet(
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
            PublicAdmins::current_institution_wallet(code, &cid, &wallets[0]),
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
