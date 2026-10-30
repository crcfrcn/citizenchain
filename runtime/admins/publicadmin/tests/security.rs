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
