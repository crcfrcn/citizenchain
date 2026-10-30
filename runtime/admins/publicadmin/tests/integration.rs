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
