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
fn invalid_wallet_replacements_are_atomic_and_do_not_repair_missing_records() {
    new_test_ext().execute_with(|| {
        let code = code_bytes("SFLP");
        let cid = wallet_fixture_cid(code, 3);
        let wallets = wallet_roster_fixture(code, &cid, 2);
        let old = wallets[0].clone();
        let new = account(200);
        for (change, error) in [
            (
                wallet_change(&old, old.clone(), old.clone(), 0),
                Error::<Test>::WalletUnchanged,
            ),
            (
                wallet_change(&old, old.clone(), wallets[1].clone(), 0),
                Error::<Test>::WalletDuplicate,
            ),
            (
                wallet_change(&old, account(99), new.clone(), 0),
                Error::<Test>::WalletOldMismatch,
            ),
            (
                wallet_change(&old, old.clone(), new.clone(), 1),
                Error::<Test>::WalletStaleRevision,
            ),
            (
                wallet_change(&account(99), account(99), new.clone(), 0),
                Error::<Test>::WalletAdminNotFound,
            ),
        ] {
            assert_noop!(
                PrivateAdmins::change_institution_wallet(
                    code,
                    cid.clone(), change),
                error
            );
        }
        let key = AdminCidNumber::try_from(cid.clone()).unwrap();
        AdminWallets::<Test>::remove(&key, &old);
        assert_eq!(PrivateAdmins::resolve_admin_account(code, &cid, &old), None);
        assert_noop!(
            PrivateAdmins::change_institution_wallet(
                code,
                cid.clone(),
                wallet_change(&old, old.clone(), new.clone(), 0)
            ),
            Error::<Test>::WalletUninitialized
        );
        let roster = AdminAccounts::<Test>::get(&key).unwrap();
        assert_noop!(
            PrivateAdmins::do_set_institution_admins(
                cid.clone(),
                code,
                AdminAccountKind::PrivateInstitution,
                roster.admins.to_vec()
            ),
            Error::<Test>::WalletUninitialized
        );
        assert!(!AdminWallets::<Test>::contains_key(&key, &old));
        AdminWallets::<Test>::insert(
            &key,
            &old,
            InstitutionAdminWalletState {
                wallet: Some(old.clone()),
                revision: u64::MAX,
            },
        );
        assert_noop!(
            PrivateAdmins::change_institution_wallet(
                code,
                cid.clone(),
                wallet_change(&old, old.clone(), new.clone(), u64::MAX)
            ),
            Error::<Test>::WalletRevisionOverflow
        );
        AdminWallets::<Test>::insert(
            &key,
            &old,
            InstitutionAdminWalletState {
                wallet: Some(old.clone()),
                revision: 0,
            },
        );
        AdminWallets::<Test>::insert(
            &key,
            &new,
            InstitutionAdminWalletState::<AccountId32> {
                wallet: None,
                revision: u64::MAX,
            },
        );
        assert_noop!(
            PrivateAdmins::change_institution_wallet(
                code,
                cid.clone(),
                wallet_change(&old, old.clone(), new.clone(), 0)
            ),
            Error::<Test>::WalletRevisionOverflow
        );
        WalletRoleBindings::<Test>::remove(&key, &old);
        assert_eq!(
            PrivateAdmins::resolve_role_authorization(code, &cid, &old),
            None
        );
        assert_noop!(
            PrivateAdmins::change_institution_wallet(
                code,
                cid.clone(),
                wallet_change(&old, old.clone(), new, 0)
            ),
            Error::<Test>::WalletUninitialized
        );
    });
}

#[test]
fn readding_retired_wallet_cannot_duplicate_unchanged_role_authority() {
    new_test_ext().execute_with(|| {
        let code = code_bytes("SFLP");
        let cid = wallet_fixture_cid(code, 9);
        let wallets = wallet_roster_fixture(code, &cid, 1);
        let old = wallets[0].clone();
        let new = account(203);
        assert_ok!(PrivateAdmins::change_institution_wallet(
            code,
            cid.clone(),
            wallet_change(&old, old.clone(), new.clone(), 0)
        ));
        let key = AdminCidNumber::try_from(cid.clone()).unwrap();
        let mut next = AdminAccounts::<Test>::get(&key).unwrap().admins.to_vec();
        let mut added = next[0].clone();
        added.account_id = old.clone();
        next.push(added);
        assert_noop!(
            PrivateAdmins::do_set_institution_admins(
                cid.clone(),
                code,
                AdminAccountKind::PrivateInstitution,
                next
            ),
            Error::<Test>::WalletDuplicate
        );
        assert_eq!(PrivateAdmins::resolve_admin_account(code, &cid, &old), None);
        assert_eq!(
            PrivateAdmins::resolve_role_authorization(code, &cid, &new),
            Some(old)
        );
    });
}


#[test]
fn wallet_lifetime_reference_preserves_nonce_without_granting_retired_membership(){
    new_test_ext().execute_with(||{
        let code=code_bytes("SFLP");let cid=wallet_fixture_cid(code,31);
        let wallets=wallet_roster_fixture(code,&cid,1);let old=wallets[0].clone();let next=account(201);
        assert_eq!(System::account(&old).sufficients,1);
        frame_system::Account::<Test>::mutate(&old,|a|a.nonce=7);
        assert_ok!(PrivateAdmins::change_institution_wallet(code,cid.clone(),wallet_change(&old,old.clone(),next.clone(),0)));
        assert_eq!(System::account(&old).sufficients,1);assert_eq!(System::account_nonce(&old),7);
        assert_eq!(System::account(&next).sufficients,1);
        assert!(PrivateAdmins::resolve_admin_account(code,&cid,&old).is_none());
        assert_ok!(PrivateAdmins::change_institution_wallet(code,cid.clone(),wallet_change(&next,next.clone(),old.clone(),0)));
        assert_eq!(System::account_nonce(&old),7);assert_eq!(System::account(&old).sufficients,1);
        let second_cid=wallet_fixture_cid(code,32);wallet_roster_fixture(code,&second_cid,1);
        assert_eq!(System::account(&old).sufficients,2);
        assert_ok!(PrivateAdmins::do_set_institution_admins(cid.clone(),code,AdminAccountKind::PrivateInstitution,vec![]));
        assert_eq!(System::account_nonce(&old),7);assert_eq!(System::account(&old).sufficients,2);
        assert!(PrivateAdmins::resolve_admin_account(code,&cid,&old).is_none());
        assert!(PrivateAdmins::resolve_admin_account(code,&second_cid,&old).is_some());
    });
}
#[test]
fn wallet_system_reference_overflow_rolls_back_original_roster_and_events(){
    new_test_ext().execute_with(||{
        let code=code_bytes("SFLP");let cid=wallet_fixture_cid(code,33);let old=wallet_roster_fixture(code,&cid,1)[0].clone();let next=account(202);
        frame_system::Account::<Test>::mutate(&next,|a|a.sufficients=u32::MAX);
        assert_noop!(PrivateAdmins::change_institution_wallet(code,cid,wallet_change(&old,old.clone(),next,0)),sp_runtime::ArithmeticError::Overflow);
    });
}
