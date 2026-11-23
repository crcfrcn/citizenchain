//! 验证管理员字段编码、默认姓名、任职布局及阶段要素规则。

use super::*;

/// 管理员声明序固定为账户、CID、姓、名，机构值只在前面增加机构码。
#[test]
fn institution_assignment_account_id_field_order_matches_node_guard() {
    use codec::Encode;

    let admin = Admin {
        account_id: 7u8,
        cid_number: AdminCidNumber::default(),
        family_name: FamilyName::truncate_from("张".as_bytes().to_vec()),
        given_name: GivenName::truncate_from("三".as_bytes().to_vec()),
    };
    let value = InstitutionAdmins {
        institution_code: *b"NRCG",
        admins: vec![admin.clone()],
    };
    assert_eq!(
        value.encode(),
        (
            *b"NRCG",
            vec![(7u8, admin.cid_number, admin.family_name, admin.given_name)]
        )
            .encode()
    );
}

#[test]
fn admin_field_order_is_account_cid_family_given() {
    use codec::Encode;

    let cid_number = AdminCidNumber::truncate_from(b"CN220-CTZN2-198805200-2026".to_vec());
    let admin = Admin {
        account_id: 7u8,
        cid_number: cid_number.clone(),
        family_name: FamilyName::truncate_from("程".as_bytes().to_vec()),
        given_name: GivenName::truncate_from("伟".as_bytes().to_vec()),
    };
    assert_eq!(
        admin.encode(),
        (
            7u8,
            cid_number,
            admin.family_name.clone(),
            admin.given_name.clone()
        )
            .encode()
    );
}

#[test]
fn missing_person_name_uses_management_default() {
    let admin = Admin {
        account_id: 1u8,
        cid_number: AdminCidNumber::default(),
        family_name: FamilyName::default(),
        given_name: GivenName::default(),
    }
    .normalize_names();
    assert_eq!(admin.family_name.as_slice(), "管理".as_bytes());
    assert_eq!(admin.given_name.as_slice(), "员".as_bytes());
}

/// 运行期真值表:公权全强制、私权 LR 岗全强制、私权非 LR/个人多签仅 account_id。
#[test]
fn required_admin_elements_operation_truth_table() {
    let public = required_admin_elements(AdminAccountKind::PublicInstitution, false);
    assert!(public.cid && public.family && public.given);

    let private_lr = required_admin_elements(AdminAccountKind::PrivateInstitution, true);
    assert!(private_lr.cid && private_lr.family && private_lr.given);

    let private_other = required_admin_elements(AdminAccountKind::PrivateInstitution, false);
    assert!(!private_other.cid && !private_other.family && !private_other.given);

    let personal = required_admin_elements(AdminAccountKind::PersonalMultisig, false);
    assert!(!personal.cid && !personal.family && !personal.given);
}

/// 死规则:个人多签 cid 无论是否 LR 均不强制（节点守卫据此锁死）。
#[test]
fn personal_multisig_cid_never_required() {
    for is_lr in [true, false] {
        assert!(!required_admin_elements(AdminAccountKind::PersonalMultisig, is_lr).cid);
    }
}

/// `satisfies` 按必填要素校验原始字段:必填项为空即不满足,全不必填则恒满足。
#[test]
fn admin_satisfies_by_required_elements() {
    let empty = Admin {
        account_id: 1u8,
        cid_number: AdminCidNumber::default(),
        family_name: FamilyName::default(),
        given_name: GivenName::default(),
    };
    // 全不必填 → 恒满足（私权非 LR / 个人多签）。
    assert!(empty.satisfies(RequiredAdminElements {
        cid: false,
        family: false,
        given: false,
    }));
    // 公权真值表要求四要素完整 → 空字段不满足。
    assert!(!empty.satisfies(required_admin_elements(
        AdminAccountKind::PublicInstitution,
        false,
    )));

    let full = Admin {
        account_id: 1u8,
        cid_number: AdminCidNumber::truncate_from(b"CN220-CTZN2-198805200-2026".to_vec()),
        family_name: FamilyName::truncate_from("张".as_bytes().to_vec()),
        given_name: GivenName::truncate_from("三".as_bytes().to_vec()),
    };
    assert!(full.satisfies(required_admin_elements(
        AdminAccountKind::PublicInstitution,
        false,
    )));
}

#[test]
fn wallet_policy_tracks_actual_address_history_and_rejects_aliases() {
    let old = InstitutionAdminWalletState {
        wallet: Some(9u8),
        revision: 4,
    };
    let request = InstitutionAdminWalletChange {
        admin_account: 9,
        expected_wallet: 9,
        new_wallet: 10,
        expected_revision: 4,
    };
    let (retired, next) =
        changed_institution_wallet_state(Some(&old), None, &request, &[]).unwrap();
    assert_eq!(
        retired,
        InstitutionAdminWalletState {
            wallet: None,
            revision: 5
        }
    );
    assert_eq!(
        next,
        InstitutionAdminWalletState {
            wallet: Some(10),
            revision: 0
        }
    );
    let returning = InstitutionAdminWalletChange {
        admin_account: 10,
        expected_wallet: 10,
        new_wallet: 9,
        expected_revision: 0,
    };
    let (_, back) =
        changed_institution_wallet_state(Some(&next), Some(&retired), &returning, &[]).unwrap();
    assert_eq!(
        back,
        InstitutionAdminWalletState {
            wallet: Some(9),
            revision: 6
        }
    );
    use InstitutionAdminWalletError as E;
    assert_eq!(
        changed_institution_wallet_state(None, None, &request, &[]),
        Err(E::Uninitialized)
    );
    assert_eq!(
        changed_institution_wallet_state(Some(&retired), None, &request, &[]),
        Err(E::Inactive)
    );
    assert_eq!(
        changed_institution_wallet_state(
            Some(&old),
            None,
            &InstitutionAdminWalletChange {
                admin_account: 1,
                ..request.clone()
            },
            &[]
        ),
        Err(E::OldWalletMismatch)
    );
    assert_eq!(
        changed_institution_wallet_state(
            Some(&old), None, &request, &[10]),
        Err(E::DuplicateWallet)
    );
    assert_eq!(
        changed_institution_wallet_state(
            Some(&old),
            None,
            &InstitutionAdminWalletChange {
                new_wallet: 9,
                ..request.clone()
            },
            &[]
        ),
        Err(E::Unchanged)
    );
    assert_eq!(
        changed_institution_wallet_state(
            Some(&old),
            None,
            &InstitutionAdminWalletChange {
                expected_revision: 3,
                ..request.clone()
            },
            &[]
        ),
        Err(E::StaleRevision)
    );
    let max_old = InstitutionAdminWalletState {
        wallet: Some(9),
        revision: u64::MAX,
    };
    assert_eq!(
        changed_institution_wallet_state(
            Some(&max_old),
            None,
            &InstitutionAdminWalletChange {
                expected_revision: u64::MAX,
                ..request.clone()
            },
            &[]
        ),
        Err(E::RevisionOverflow)
    );
    let max_new = InstitutionAdminWalletState {
        wallet: None,
        revision: u64::MAX,
    };
    assert_eq!(
        changed_institution_wallet_state(Some(&old), Some(&max_new), &request, &[]),
        Err(E::RevisionOverflow)
    );
    assert_eq!(
        initial_institution_wallet_state(Some(&old), &9, Some(4), &[]),
        Err(E::AlreadyInitialized)
    );
    assert_eq!(
        initial_institution_wallet_state(Some(&retired), &9, None, &[]),
        Err(E::StaleRevision)
    );
    assert_eq!(
        initial_institution_wallet_state(None, &9, Some(0), &[]),
        Err(E::StaleRevision)
    );
    assert_eq!(
        retired_institution_wallet_state(Some(&retired), 5),
        Err(E::Inactive)
    );
    assert_eq!(
        retired_institution_wallet_state(Some(&old), 3),
        Err(E::StaleRevision)
    );
}
