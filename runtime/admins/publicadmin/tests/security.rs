//! 校验失败时原名册、事件与其它链状态必须全部保持。
use super::*;
use frame_support::assert_noop;

#[test]
fn invalid_public_rosters_do_not_write_storage_or_events() {
    new_test_ext().execute_with(|| {
        let cid = b"GD001-CGOV0-923456789-2026".to_vec();
        let code = code_bytes("CGOV");
        let mut repeated = admins(2);
        repeated[1].account_id = repeated[0].account_id.clone();
        assert_noop!(
            PublicAdmins::do_set_institution_admins(
                cid.clone(),
                code,
                AdminAccountKind::PublicInstitution,
                repeated
            ),
            Error::<Test>::DuplicateAdmin
        );
        assert_noop!(
            PublicAdmins::do_set_institution_admins(
                cid.clone(),
                code,
                AdminAccountKind::PublicInstitution,
                Vec::new()
            ),
            Error::<Test>::InvalidAdminsLen
        );
        assert_noop!(
            PublicAdmins::do_set_institution_admins(
                cid.clone(),
                code,
                AdminAccountKind::PrivateInstitution,
                admins(1)
            ),
            Error::<Test>::InvalidAdminAccountKind
        );
        assert_noop!(
            PublicAdmins::do_set_institution_admins(
                vec![b'X'; primitives::core_const::CID_NUMBER_MAX_BYTES as usize + 1],
                code,
                AdminAccountKind::PublicInstitution,
                admins(1)
            ),
            Error::<Test>::InvalidInstitution
        );
        assert_noop!(
            PublicAdmins::do_set_institution_admins(
                cid,
                code,
                AdminAccountKind::PublicInstitution,
                indexed_admins(1990)
            ),
            Error::<Test>::InvalidAdminsLen
        );
    });
}

#[test]
fn institution_wallet_conflicts_leave_wallet_roster_events_and_other_state_unchanged() {
    new_test_ext().execute_with(|| {
        let code = code_bytes("CGOV");
        let cid = wallet_fixture_cid(code, 3);
        let anchors = wallet_roster_fixture(code, &cid, 2);
        assert_noop!(
            PublicAdmins::initialize_institution_wallet(
                code,
                cid.clone(),
                account(99),
                account(100),
                None
            ),
            Error::<Test>::WalletAdminNotFound
        );
        assert_noop!(
            PublicAdmins::change_institution_wallet(
                code,
                cid.clone(),
                wallet_change(&anchors[0], anchors[0].clone(), account(200), 0)
            ),
            Error::<Test>::WalletUninitialized
        );
        assert_ok!(PublicAdmins::initialize_institution_wallet(
            code,
            cid.clone(),
            anchors[0].clone(),
            account(100),
            None
        ));
        // 另一管理员尚未初始化：更新必须失败，而不是猜测对方钱包。
        assert_noop!(
            PublicAdmins::change_institution_wallet(
                code,
                cid.clone(),
                wallet_change(&anchors[0], account(100), account(200), 0)
            ),
            Error::<Test>::WalletUninitialized
        );
        assert_noop!(
            PublicAdmins::initialize_institution_wallet(
                code,
                cid.clone(),
                anchors[1].clone(),
                account(100),
                None
            ),
            Error::<Test>::WalletDuplicate
        );
        assert_ok!(PublicAdmins::initialize_institution_wallet(
            code,
            cid.clone(),
            anchors[1].clone(),
            account(101),
            None
        ));
        assert_noop!(
            PublicAdmins::initialize_institution_wallet(
                code,
                cid.clone(),
                anchors[0].clone(),
                account(102),
                None
            ),
            Error::<Test>::WalletAlreadyInitialized
        );
        for (old, new, version, error) in [
            (
                account(100),
                account(102),
                1,
                Error::<Test>::WalletStaleRevision,
            ),
            (
                account(90),
                account(102),
                0,
                Error::<Test>::WalletOldMismatch,
            ),
            (
                account(100),
                account(100),
                0,
                Error::<Test>::WalletUnchanged,
            ),
            (
                account(100),
                account(101),
                0,
                Error::<Test>::WalletDuplicate,
            ),
        ] {
            assert_noop!(
                PublicAdmins::change_institution_wallet(
                    code,
                    cid.clone(),
                    wallet_change(&anchors[0], old, new, version)
                ),
                error
            );
        }
        assert_noop!(
            PublicAdmins::change_institution_wallet(
                code_bytes("MFA"),
                cid.clone(),
                wallet_change(&anchors[0], account(100), account(102), 0)
            ),
            Error::<Test>::InstitutionCodeMismatch
        );
        assert_noop!(
            PublicAdmins::retire_institution_wallet(code, cid.clone(), anchors[0].clone(), 0),
            Error::<Test>::WalletAdminStillPresent
        );
        assert_eq!(
            PublicAdmins::current_institution_wallet(code, &cid, &anchors[0]),
            Some(account(100))
        );
    });
}

#[test]
fn institution_wallet_missing_targets_and_overflow_are_atomic() {
    new_test_ext().execute_with(|| {
        let code = code_bytes("CGOV");
        let cid = wallet_fixture_cid(code, 4);
        let anchors = wallet_roster_fixture(code, &cid, 1);
        assert_noop!(
            PublicAdmins::initialize_institution_wallet(
                code,
                cid.clone(),
                anchors[0].clone(),
                account(100),
                Some(0)
            ),
            Error::<Test>::WalletStaleRevision
        );
        assert_noop!(
            PublicAdmins::initialize_institution_wallet(
                code,
                Vec::new(),
                anchors[0].clone(),
                account(100),
                None
            ),
            Error::<Test>::InvalidInstitution
        );
        assert_noop!(
            PublicAdmins::initialize_institution_wallet(
                code,
                vec![b'A'; primitives::core_const::CID_NUMBER_MAX_BYTES as usize + 1],
                anchors[0].clone(),
                account(100),
                None
            ),
            Error::<Test>::InvalidInstitution
        );
        assert_noop!(
            PublicAdmins::initialize_institution_wallet(
                code,
                wallet_fixture_cid(code, 5),
                anchors[0].clone(),
                account(100),
                None
            ),
            Error::<Test>::InvalidInstitution
        );
        assert_noop!(
            PublicAdmins::initialize_institution_wallet(
                code_bytes("PMUL"),
                cid.clone(),
                anchors[0].clone(),
                account(100),
                None
            ),
            Error::<Test>::InvalidInstitution
        );
        assert_noop!(
            PublicAdmins::change_institution_wallet(
                code,
                cid.clone(),
                wallet_change(&anchors[0], anchors[0].clone(), account(100), 0)
            ),
            Error::<Test>::WalletUninitialized
        );
        let key = AdminCidNumber::try_from(cid.clone()).unwrap();
        AdminWallets::<Test>::insert(
            &key,
            &anchors[0],
            InstitutionAdminWalletState {
                wallet: Some(account(100)),
                revision: u64::MAX,
            },
        );
        assert_noop!(
            PublicAdmins::change_institution_wallet(
                code,
                cid.clone(),
                wallet_change(&anchors[0], account(100), account(101), u64::MAX)
            ),
            Error::<Test>::WalletRevisionOverflow
        );
        assert_eq!(
            PublicAdmins::institution_wallet_state(code, &cid, &anchors[0])
                .unwrap()
                .revision,
            u64::MAX
        );
        AdminAccounts::<Test>::remove(&key);
        assert_eq!(
            PublicAdmins::current_institution_wallet(code, &cid, &anchors[0]),
            None
        );
        assert_noop!(
            PublicAdmins::retire_institution_wallet(
                code,
                cid.clone(),
                anchors[0].clone(),
                u64::MAX
            ),
            Error::<Test>::WalletRevisionOverflow
        );
        AdminWallets::<Test>::insert(
            &key,
            &anchors[0],
            InstitutionAdminWalletState {
                wallet: None::<AccountId32>,
                revision: u64::MAX,
            },
        );
        wallet_roster_fixture(code, &cid, 1);
        assert_noop!(
            PublicAdmins::initialize_institution_wallet(
                code,
                cid,
                anchors[0].clone(),
                account(100),
                Some(u64::MAX)
            ),
            Error::<Test>::WalletRevisionOverflow
        );
    });
}
