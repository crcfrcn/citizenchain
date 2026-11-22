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
fn institution_wallet_policy_lifecycle_never_resets_versions() {
    let initial = initial_institution_wallet_state(None, &9u8, None, &[]).unwrap();
    assert_eq!(
        initial,
        InstitutionAdminWalletState {
            wallet: Some(9),
            revision: 0
        }
    );
    let request = InstitutionAdminWalletChange {
        admin_account: 1,
        expected_wallet: 9,
        new_wallet: 10,
        expected_revision: 0,
    };
    let changed = changed_institution_wallet_state(Some(&initial), &request, &[20]).unwrap();
    assert_eq!(
        changed,
        InstitutionAdminWalletState {
            wallet: Some(10),
            revision: 1
        }
    );
    let retired = retired_institution_wallet_state(Some(&changed), 1).unwrap();
    assert_eq!(
        retired,
        InstitutionAdminWalletState {
            wallet: None,
            revision: 2
        }
    );
    assert_eq!(
        initial_institution_wallet_state(Some(&retired), &11, None, &[]),
        Err(InstitutionAdminWalletError::StaleRevision)
    );
    let readded = initial_institution_wallet_state(Some(&retired), &11, Some(2), &[]).unwrap();
    assert_eq!(readded.revision, 3);
    assert_eq!(
        changed_institution_wallet_state(Some(&readded), &request, &[]),
        Err(InstitutionAdminWalletError::StaleRevision)
    );
}

#[test]
fn institution_wallet_policy_rejects_conflicts_and_revision_overflow() {
    use InstitutionAdminWalletError as Error;
    let state = InstitutionAdminWalletState {
        wallet: Some(9u8),
        revision: 4,
    };
    let request = InstitutionAdminWalletChange {
        admin_account: 1,
        expected_wallet: 9,
        new_wallet: 10,
        expected_revision: 4,
    };
    assert_eq!(
        initial_institution_wallet_state(Some(&state), &10, Some(4), &[]),
        Err(Error::AlreadyInitialized)
    );
    assert_eq!(
        initial_institution_wallet_state(None, &9, Some(0), &[]),
        Err(Error::StaleRevision)
    );
    assert_eq!(
        initial_institution_wallet_state(None, &9, None, &[9]),
        Err(Error::DuplicateWallet)
    );
    assert_eq!(
        changed_institution_wallet_state(None, &request, &[]),
        Err(Error::Uninitialized)
    );
    assert_eq!(
        changed_institution_wallet_state(Some(&state), &request, &[10]),
        Err(Error::DuplicateWallet)
    );
    assert_eq!(
        changed_institution_wallet_state(
            Some(&state),
            &InstitutionAdminWalletChange {
                expected_wallet: 8,
                ..request.clone()
            },
            &[]
        ),
        Err(Error::OldWalletMismatch)
    );
    assert_eq!(
        changed_institution_wallet_state(
            Some(&state),
            &InstitutionAdminWalletChange {
                new_wallet: 9,
                ..request.clone()
            },
            &[]
        ),
        Err(Error::Unchanged)
    );
    let inactive = InstitutionAdminWalletState {
        wallet: None,
        revision: 5,
    };
    assert_eq!(
        changed_institution_wallet_state(Some(&inactive), &request, &[]),
        Err(Error::Inactive)
    );
    assert_eq!(
        retired_institution_wallet_state(Some(&inactive), 5),
        Err(Error::Inactive)
    );
    assert_eq!(
        retired_institution_wallet_state::<u8>(None, 0),
        Err(Error::Uninitialized)
    );
    assert_eq!(
        retired_institution_wallet_state(Some(&state), 3),
        Err(Error::StaleRevision)
    );
    let max = InstitutionAdminWalletState {
        wallet: Some(9),
        revision: u64::MAX,
    };
    assert_eq!(
        changed_institution_wallet_state(
            Some(&max),
            &InstitutionAdminWalletChange {
                expected_revision: u64::MAX,
                ..request
            },
            &[]
        ),
        Err(Error::RevisionOverflow)
    );
    assert_eq!(
        retired_institution_wallet_state(Some(&max), u64::MAX),
        Err(Error::RevisionOverflow)
    );
    let max_retired = InstitutionAdminWalletState {
        wallet: None,
        revision: u64::MAX,
    };
    assert_eq!(
        initial_institution_wallet_state(Some(&max_retired), &9, Some(u64::MAX), &[]),
        Err(Error::RevisionOverflow)
    );
}
