//! 私权生命周期入口和读取接口共享同一CID名册，替换后旧人员立即失权。
use super::*;

#[test]
fn private_lifecycle_replaces_roster_and_normalizes_names_once() {
    new_test_ext().execute_with(|| {
        let cid = b"GD001-SFLP0-923456789-2026".to_vec();
        let code = code_bytes("SFLP");
        assert_ok!(
            <PrivateAdmins as InstitutionAdminLifecycle<AccountId32>>::set_institution_admins(
                b"fixture",
                cid.clone(),
                code,
                AdminAccountKind::PrivateInstitution,
                admins(2)
            )
        );
        let mut next = admins(1);
        next[0].account_id = account(9);
        next[0].family_name = Default::default();
        next[0].given_name = Default::default();
        assert_ok!(
            <PrivateAdmins as InstitutionAdminLifecycle<AccountId32>>::set_institution_admins(
                b"fixture",
                cid.clone(),
                code,
                AdminAccountKind::PrivateInstitution,
                next
            )
        );
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
