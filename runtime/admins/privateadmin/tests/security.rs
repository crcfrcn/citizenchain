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
