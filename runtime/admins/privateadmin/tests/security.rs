//! 私权名册拒绝路径覆盖真实事务状态，不能因默认姓名而绕过去重或容量。
use super::*;
use frame_support::{assert_noop, traits::Get};

#[test]
fn invalid_private_rosters_and_cross_code_overwrites_are_atomic() {
    new_test_ext().execute_with(|| {
        let cid = b"GD001-SFLP0-923456789-2026".to_vec();
        let code = code_bytes("SFLP");
        assert_ok!(PrivateAdmins::do_set_institution_admins(
            cid.clone(),
            code,
            AdminAccountKind::PrivateInstitution,
            admins(2)
        ));
        let mut repeated = admins(2);
        repeated[1].account_id = repeated[0].account_id.clone();
        assert_noop!(
            PrivateAdmins::do_set_institution_admins(
                cid.clone(),
                code,
                AdminAccountKind::PrivateInstitution,
                repeated
            ),
            Error::<Test>::DuplicateAdmin
        );
        assert_noop!(
            PrivateAdmins::do_set_institution_admins(
                cid.clone(),
                code,
                AdminAccountKind::PrivateInstitution,
                Vec::new()
            ),
            Error::<Test>::InvalidAdminsLen
        );
        let max = <<Test as Config>::MaxAdminsPerInstitution as Get<u32>>::get() as usize;
        assert_noop!(
            PrivateAdmins::do_set_institution_admins(
                cid.clone(),
                code,
                AdminAccountKind::PrivateInstitution,
                vec![admins(1)[0].clone(); max + 1]
            ),
            Error::<Test>::InvalidAdminsLen
        );
        assert_noop!(
            PrivateAdmins::do_set_institution_admins(
                cid.clone(),
                code_bytes("SFGQ"),
                AdminAccountKind::PrivateInstitution,
                admins(1)
            ),
            Error::<Test>::InstitutionCodeMismatch
        );
        assert_noop!(
            PrivateAdmins::do_set_institution_admins(
                vec![0; primitives::core_const::CID_NUMBER_MAX_BYTES as usize + 1],
                code,
                AdminAccountKind::PrivateInstitution,
                admins(1)
            ),
            Error::<Test>::InvalidInstitution
        );
        assert_eq!(PrivateAdmins::institution_admins_len(code, &cid), Some(2));
    });
}

#[test]
fn institution_wallet_conflicts_leave_wallet_roster_events_and_other_state_unchanged() {
    new_test_ext().execute_with(|| {
        let code = code_bytes("SFLP");
        let cid = wallet_fixture_cid(code, 3);
        let anchors = wallet_roster_fixture(code, &cid, 2);
        assert_noop!(
            PrivateAdmins::initialize_institution_wallet(
                code,
                cid.clone(),
                account(99),
                account(100),
                None
            ),
            Error::<Test>::WalletAdminNotFound
        );
        assert_noop!(
            PrivateAdmins::change_institution_wallet(
                code,
                cid.clone(),
                wallet_change(&anchors[0], anchors[0].clone(), account(200), 0)
            ),
            Error::<Test>::WalletUninitialized
        );
        assert_ok!(PrivateAdmins::initialize_institution_wallet(
            code,
            cid.clone(),
            anchors[0].clone(),
            account(100),
            None
        ));
        // 另一管理员尚未初始化：更新必须失败，而不是猜测对方钱包。
        assert_noop!(
            PrivateAdmins::change_institution_wallet(
                code,
                cid.clone(),
                wallet_change(&anchors[0], account(100), account(200), 0)
            ),
            Error::<Test>::WalletUninitialized
        );
        assert_noop!(
            PrivateAdmins::initialize_institution_wallet(
                code,
                cid.clone(),
                anchors[1].clone(),
                account(100),
                None
            ),
            Error::<Test>::WalletDuplicate
        );
        assert_ok!(PrivateAdmins::initialize_institution_wallet(
            code,
            cid.clone(),
            anchors[1].clone(),
            account(101),
            None
        ));
        assert_noop!(
            PrivateAdmins::initialize_institution_wallet(
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
                PrivateAdmins::change_institution_wallet(
                    code,
                    cid.clone(),
                    wallet_change(&anchors[0], old, new, version)
                ),
                error
            );
        }
        assert_noop!(
            PrivateAdmins::change_institution_wallet(
                code_bytes("SFGQ"),
                cid.clone(),
                wallet_change(&anchors[0], account(100), account(102), 0)
            ),
            Error::<Test>::InstitutionCodeMismatch
        );
        assert_noop!(
            PrivateAdmins::retire_institution_wallet(code, cid.clone(), anchors[0].clone(), 0),
            Error::<Test>::WalletAdminStillPresent
        );
        assert_eq!(
            PrivateAdmins::current_institution_wallet(code, &cid, &anchors[0]),
            Some(account(100))
        );
    });
}

#[test]
fn institution_wallet_missing_targets_and_overflow_are_atomic() {
    new_test_ext().execute_with(|| {
        let code = code_bytes("SFLP");
        let cid = wallet_fixture_cid(code, 4);
        let anchors = wallet_roster_fixture(code, &cid, 1);
        assert_noop!(
            PrivateAdmins::initialize_institution_wallet(
                code,
                cid.clone(),
                anchors[0].clone(),
                account(100),
                Some(0)
            ),
            Error::<Test>::WalletStaleRevision
        );
        assert_noop!(
            PrivateAdmins::initialize_institution_wallet(
                code,
                Vec::new(),
                anchors[0].clone(),
                account(100),
                None
            ),
            Error::<Test>::InvalidInstitution
        );
        assert_noop!(
            PrivateAdmins::initialize_institution_wallet(
                code,
                vec![b'A'; primitives::core_const::CID_NUMBER_MAX_BYTES as usize + 1],
                anchors[0].clone(),
                account(100),
                None
            ),
            Error::<Test>::InvalidInstitution
        );
        assert_noop!(
            PrivateAdmins::initialize_institution_wallet(
                code,
                wallet_fixture_cid(code, 5),
                anchors[0].clone(),
                account(100),
                None
            ),
            Error::<Test>::InvalidInstitution
        );
        assert_noop!(
            PrivateAdmins::initialize_institution_wallet(
                code_bytes("PMUL"),
                cid.clone(),
                anchors[0].clone(),
                account(100),
                None
            ),
            Error::<Test>::InvalidInstitution
        );
        assert_noop!(
            PrivateAdmins::change_institution_wallet(
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
            PrivateAdmins::change_institution_wallet(
                code,
                cid.clone(),
                wallet_change(&anchors[0], account(100), account(101), u64::MAX)
            ),
            Error::<Test>::WalletRevisionOverflow
        );
        assert_eq!(
            PrivateAdmins::institution_wallet_state(code, &cid, &anchors[0])
                .unwrap()
                .revision,
            u64::MAX
        );
        AdminAccounts::<Test>::remove(&key);
        assert_eq!(
            PrivateAdmins::current_institution_wallet(code, &cid, &anchors[0]),
            None
        );
        assert_noop!(
            PrivateAdmins::retire_institution_wallet(
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
            PrivateAdmins::initialize_institution_wallet(
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
