//! 管理员共用类型与要素规则的单元测试。

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
