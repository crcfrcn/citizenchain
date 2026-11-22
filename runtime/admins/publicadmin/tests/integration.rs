//! 通过所属公开生命周期trait写入，查询必须立即反映完整名册替换。
use super::*;

#[test]
fn public_lifecycle_and_queries_share_one_roster() {
    new_test_ext().execute_with(|| {
        let cid = b"GD001-CGOV0-923456789-2026".to_vec();
        let code = code_bytes("CGOV");
        assert_ok!(<PublicAdmins as InstitutionAdminLifecycle<
            AccountId32,
            Admin<AccountId32>,
        >>::set_institution_admins(
            b"fixture",
            cid.clone(),
            code,
            AdminAccountKind::PublicInstitution,
            admins(2)
        ));
        assert_eq!(PublicAdmins::institution_admins_len(code, &cid), Some(2));
        assert!(PublicAdmins::is_institution_admin(code, &cid, &account(0)));
        let mut next = admins(1);
        next[0].account_id = account(7);
        assert_ok!(<PublicAdmins as InstitutionAdminLifecycle<
            AccountId32,
            Admin<AccountId32>,
        >>::set_institution_admins(
            b"fixture",
            cid.clone(),
            code,
            AdminAccountKind::PublicInstitution,
            next
        ));
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
fn institution_wallet_retirement_readdition_and_replay_keep_one_monotonic_history() {
    new_test_ext().execute_with(|| {
        let code = code_bytes("CGOV");
        let cid = wallet_fixture_cid(code, 6);
        let anchors = wallet_roster_fixture(code, &cid, 2);
        initialize_wallet_roster(code, &cid, &anchors);
        let old_request = wallet_change(&anchors[0], anchors[0].clone(), account(200), 0);
        assert_ok!(PublicAdmins::change_institution_wallet(
            code,
            cid.clone(),
            old_request.clone()
        ));
        assert_noop!(
            PublicAdmins::change_institution_wallet(code, cid.clone(), old_request.clone()),
            Error::<Test>::WalletStaleRevision
        );
        let key = AdminCidNumber::try_from(cid.clone()).unwrap();
        let original = AdminAccounts::<Test>::get(&key).unwrap();
        // 原名册生命周期第1步尚未接入钱包清理，独立查询也必须排除离任人员。
        let mut remaining = original.clone();
        remaining.admins.remove(0);
        AdminAccounts::<Test>::insert(&key, remaining);
        assert_eq!(
            PublicAdmins::current_institution_wallet(code, &cid, &anchors[0]),
            None
        );
        assert_noop!(
            PublicAdmins::change_institution_wallet(
                code,
                cid.clone(),
                wallet_change(&anchors[0], account(200), account(201), 1)
            ),
            Error::<Test>::WalletAdminNotFound
        );
        assert_noop!(
            PublicAdmins::retire_institution_wallet(code, cid.clone(), anchors[0].clone(), 0),
            Error::<Test>::WalletStaleRevision
        );
        assert_ok!(PublicAdmins::retire_institution_wallet(
            code,
            cid.clone(),
            anchors[0].clone(),
            1
        ));
        assert_eq!(
            PublicAdmins::institution_wallet_state(code, &cid, &anchors[0]),
            Some(InstitutionAdminWalletState {
                wallet: None,
                revision: 2
            })
        );
        assert_noop!(
            PublicAdmins::retire_institution_wallet(code, cid.clone(), anchors[0].clone(), 2),
            Error::<Test>::WalletInactive
        );
        AdminAccounts::<Test>::insert(&key, original.clone());
        assert_eq!(
            PublicAdmins::current_institution_wallet(code, &cid, &anchors[0]),
            None
        );
        assert_noop!(
            PublicAdmins::change_institution_wallet(code, cid.clone(), old_request.clone()),
            Error::<Test>::WalletInactive
        );
        assert_noop!(
            PublicAdmins::initialize_institution_wallet(
                code,
                cid.clone(),
                anchors[0].clone(),
                account(201),
                None
            ),
            Error::<Test>::WalletStaleRevision
        );
        assert_ok!(PublicAdmins::initialize_institution_wallet(
            code,
            cid.clone(),
            anchors[0].clone(),
            account(201),
            Some(2)
        ));
        assert_eq!(
            PublicAdmins::institution_wallet_state(code, &cid, &anchors[0])
                .unwrap()
                .revision,
            3
        );
        assert_noop!(
            PublicAdmins::change_institution_wallet(code, cid.clone(), old_request),
            Error::<Test>::WalletStaleRevision
        );
        assert_eq!(AdminAccounts::<Test>::get(&key), Some(original));
        System::assert_last_event(
            Event::<Test>::InstitutionAdminWalletInitialized {
                cid_number: key,
                admin_account: anchors[0].clone(),
                wallet: account(201),
                revision: 3,
            }
            .into(),
        );
    });
}

#[test]
fn institution_wallet_updates_leave_existing_vote_snapshot_and_roster_encoding_intact() {
    use codec::Encode;
    use engingcore::types::{AuthorizationSubject, CidNumber, RoleCode, RoleSubject};
    new_test_ext().execute_with(|| {
        let code = code_bytes("CGOV");
        let cid = wallet_fixture_cid(code, 7);
        let anchors = wallet_roster_fixture(code, &cid, 1);
        let key = AdminCidNumber::try_from(cid.clone()).unwrap();
        let before = AdminAccounts::<Test>::get(&key).unwrap().encode();
        let subject = AuthorizationSubject::Institution(RoleSubject {
            cid_number: CidNumber::try_from(cid.clone()).unwrap(),
            role_code: RoleCode::try_from(b"FIXTURE_ROLE".to_vec()).unwrap(),
        });
        let voters: BoundedVec<AccountId32, <Test as engingcore::Config>::MaxAdminsPerInstitution> =
            anchors.clone().try_into().unwrap();
        engingcore::VoterSnapshot::<Test>::insert(42u128, &subject, &voters);
        initialize_wallet_roster(code, &cid, &anchors);
        assert_ok!(PublicAdmins::change_institution_wallet(
            code,
            cid.clone(),
            wallet_change(&anchors[0], anchors[0].clone(), account(202), 0)
        ));
        assert_eq!(
            engingcore::VoterSnapshot::<Test>::get(42, &subject),
            Some(voters)
        );
        assert_eq!(AdminAccounts::<Test>::get(&key).unwrap().encode(), before);
        assert_eq!(
            PublicAdmins::resolve_admin_account(code, &cid, &anchors[0]),
            Some(anchors[0].clone())
        );
        assert_eq!(
            PublicAdmins::resolve_admin_account(code, &cid, &account(202)),
            None
        );
    });
}

#[test]
fn institution_wallet_complete_roster_removal_keeps_retirement_version_for_recreation() {
    new_test_ext().execute_with(|| {
        let code = code_bytes("CGOV");
        let cid = wallet_fixture_cid(code, 9);
        let anchors = wallet_roster_fixture(code, &cid, 1);
        initialize_wallet_roster(code, &cid, &anchors);
        let key = AdminCidNumber::try_from(cid.clone()).unwrap();
        AdminAccounts::<Test>::remove(&key);
        assert_eq!(
            PublicAdmins::current_institution_wallet(code, &cid, &anchors[0]),
            None
        );
        assert_noop!(
            PublicAdmins::retire_institution_wallet(code_bytes("MFA"), cid.clone(), anchors[0].clone(), 0),
            Error::<Test>::InstitutionCodeMismatch
        );
        assert_ok!(PublicAdmins::retire_institution_wallet(
            code,
            cid.clone(),
            anchors[0].clone(),
            0
        ));
        assert_eq!(
            AdminWallets::<Test>::get(&key, &anchors[0]),
            Some(InstitutionAdminWalletState {
                wallet: None,
                revision: 1
            })
        );
        wallet_roster_fixture(code, &cid, 1);
        assert_noop!(
            PublicAdmins::initialize_institution_wallet(
                code,
                cid.clone(),
                anchors[0].clone(),
                account(203),
                None
            ),
            Error::<Test>::WalletStaleRevision
        );
        assert_ok!(PublicAdmins::initialize_institution_wallet(
            code,
            cid.clone(),
            anchors[0].clone(),
            account(203),
            Some(1)
        ));
        assert_eq!(
            PublicAdmins::institution_wallet_state(code, &cid, &anchors[0])
                .unwrap()
                .revision,
            2
        );
    });
}
