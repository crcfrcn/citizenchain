//! 原有行为回归完整迁入，复用所属包测试夹具。
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
