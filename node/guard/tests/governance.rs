#![allow(clippy::expect_used)]
use super::*;

use crate::guard::governance_skeleton::check_affected_institutions;
use crate::guard::governance_skeleton::check_catalog_keys;
use crate::guard::governance_skeleton::check_skeleton_invariants;
use crate::guard::governance_skeleton::decode_exact;
use crate::guard::governance_skeleton::expected_foundation_identity;
use crate::guard::governance_skeleton::expected_roles;
use crate::guard::governance_skeleton::is_private_protected_cid;
use crate::guard::governance_skeleton::needs_full_check;
use crate::guard::governance_skeleton::protected_institutions;
use crate::guard::governance_skeleton::DecodedInstitutionAdminAssignment;
use crate::guard::governance_skeleton::DecodedInstitutionInfo;
use crate::guard::governance_skeleton::DecodedInstitutionRole;
use crate::guard::governance_skeleton::DecodedPrivateInstitutionAdmins;
use crate::guard::governance_skeleton::DecodedPublicInstitutionAdmins;
use crate::guard::governance_skeleton::DecodedRoleBusinessPermission;
use crate::guard::governance_skeleton::ExpectedRole;
use crate::guard::governance_skeleton::GuardError;
use adminbase::Admin;
use entitybase::BusinessActionId;
use entitybase::RoleBusinessPermission;
use entitybase::RoleSubject;
use primitives::cid::china::citizenchain::CITIZENCHAIN_FOUNDATION;
use primitives::governance_skeleton::fixed_institutions;
use primitives::governance_skeleton::FixedInstitution;
use sp_crypto_hashing::blake2_128;
use sp_crypto_hashing::twox_128;
use std::collections::BTreeMap;

fn accounts_for(institution: &FixedInstitution) -> Vec<[u8; 32]> {
    (0..institution.expected_len)
        .map(|index| [(index + 1) as u8; 32])
        .collect()
}

fn account_bytes(institution: &FixedInstitution, admins: Vec<[u8; 32]>) -> Vec<u8> {
    if is_private_protected_cid(institution.cid_number.as_bytes()) {
        // 基金会名册必须与创世 seeder 写入的身份逐字节一致：公民 CID 复用法定
        // 代表人真源，姓名取创世管理员表。守卫冻结这三要素，只放行钱包账户更换。
        let (cid_number, family_name, given_name) = expected_foundation_identity();
        return DecodedPrivateInstitutionAdmins {
            institution_code: institution.code,
            admins: admins
                .into_iter()
                .map(|account_id| Admin {
                    account_id,
                    cid_number: cid_number.to_vec().try_into().expect("cid fits"),
                    family_name: family_name.to_vec().try_into().expect("name fits"),
                    given_name: given_name.to_vec().try_into().expect("name fits"),
                })
                .collect(),
        }
        .encode();
    }
    DecodedPublicInstitutionAdmins {
        institution_code: institution.code,
        admins: admins
            .into_iter()
            .map(|account_id| Admin {
                account_id,
                cid_number: Default::default(),
                family_name: Default::default(),
                given_name: Default::default(),
            })
            .collect(),
    }
    .encode()
}

fn role_bytes(institution: &FixedInstitution, role: &ExpectedRole) -> Vec<u8> {
    DecodedInstitutionRole {
        cid_number: institution.cid_number.as_bytes().to_vec(),
        role_code: role.role_code.clone(),
        role_name: role.role_name.clone(),
        term_required: false,
        role_status: InstitutionRoleStatus::Active,
    }
    .encode()
}

fn private_info_bytes(institution: &FixedInstitution, legal_account: [u8; 32]) -> Vec<u8> {
    DecodedInstitutionInfo {
        cid_full_name: CITIZENCHAIN_FOUNDATION.cid_full_name.as_bytes().to_vec(),
        cid_short_name: CITIZENCHAIN_FOUNDATION.cid_short_name.as_bytes().to_vec(),
        town_code: Vec::new(),
        legal_representative: Some(entitybase::LegalRepresentative {
            family_name: primitives::cid::china::citizenchain::LEGAL_REPRESENTATIVE_FAMILY_NAME
                .as_bytes()
                .to_vec(),
            given_name: primitives::cid::china::citizenchain::LEGAL_REPRESENTATIVE_GIVEN_NAME
                .as_bytes()
                .to_vec(),
            cid_number:
                primitives::cid::china::citizenchain::LEGAL_REPRESENTATIVE_CITIZEN_CID_NUMBER
                    .as_bytes()
                    .to_vec(),
            account_id: legal_account,
        }),
        institution_code: institution.code,
        created_at: 0,
    }
    .encode()
}

fn permission_bytes(institution: &FixedInstitution, role: &ExpectedRole) -> Vec<u8> {
    entitybase::fixed_role_permission_specs(
        institution.code,
        institution.cid_number.as_bytes(),
        &role.role_code,
    )
    .into_iter()
    .map(|permission| RoleBusinessPermission {
        role_subject: RoleSubject {
            cid_number: institution.cid_number.as_bytes().to_vec(),
            role_code: role.role_code.clone(),
        },
        business_action_id: BusinessActionId {
            module_tag: permission.module_tag.to_vec(),
            action_code: permission.action_code,
        },
        operation: permission.operation,
    })
    .collect::<Vec<DecodedRoleBusinessPermission>>()
    .encode()
}

fn assignment(
    institution: &FixedInstitution,
    role: &ExpectedRole,
    account_id: [u8; 32],
) -> DecodedInstitutionAdminAssignment {
    DecodedInstitutionAdminAssignment {
        cid_number: institution.cid_number.as_bytes().to_vec(),
        account_id,
        role_code: role.role_code.clone(),
        term_start: 0,
        term_end: 0,
        assignment_source: InstitutionAssignmentSource::Genesis,
        assignment_source_ref: Vec::new(),
        assignment_status: InstitutionAssignmentStatus::Active,
    }
}

fn valid_state() -> BTreeMap<Vec<u8>, Vec<u8>> {
    let mut state = BTreeMap::new();
    for institution in protected_institutions() {
        let admins = accounts_for(&institution);
        state.insert(
            crate::guard::governance_skeleton::storage_key::account_id(
                institution.cid_number.as_bytes(),
            ),
            account_bytes(&institution, admins.clone()),
        );
        if is_private_protected_cid(institution.cid_number.as_bytes()) {
            state.insert(
                crate::guard::governance_skeleton::storage_key::private_institution(
                    institution.cid_number.as_bytes(),
                ),
                private_info_bytes(&institution, admins[0]),
            );
        }
        let mut offset = 0usize;
        for role in expected_roles(&institution) {
            state.insert(
                crate::guard::governance_skeleton::storage_key::institution_role(
                    institution.cid_number.as_bytes(),
                    &role.role_code,
                ),
                role_bytes(&institution, &role),
            );
            state.insert(
                crate::guard::governance_skeleton::storage_key::institution_role_permissions(
                    institution.cid_number.as_bytes(),
                    &role.role_code,
                ),
                permission_bytes(&institution, &role),
            );
            let assignments = if is_private_protected_cid(institution.cid_number.as_bytes()) {
                (0..role.seats)
                    .map(|_| assignment(&institution, &role, admins[0]))
                    .collect::<Vec<_>>()
            } else {
                let end = offset + role.seats as usize;
                let assignments = admins[offset..end]
                    .iter()
                    .copied()
                    .map(|admin| assignment(&institution, &role, admin))
                    .collect::<Vec<_>>();
                offset = end;
                assignments
            };
            state.insert(
                crate::guard::governance_skeleton::storage_key::institution_role_assignments(
                    institution.cid_number.as_bytes(),
                    &role.role_code,
                ),
                assignments.encode(),
            );
        }
        if !is_private_protected_cid(institution.cid_number.as_bytes()) {
            assert_eq!(offset, admins.len());
        }
    }
    state
}

fn check_state(state: &BTreeMap<Vec<u8>, Vec<u8>>) -> Result<(), GuardError> {
    check_catalog_keys(state.keys())?;
    check_skeleton_invariants(|key| state.get(key).cloned())
}

#[test]
fn valid_fixed_admin_role_and_assignment_state_passes() {
    assert_eq!(check_state(&valid_state()), Ok(()));
}

/// 基金会三个创世岗位永久由同一位公民担任：钱包账户可换，
/// 公民 CID / 姓 / 名永久冻结；换钱包必须同步换绑 identity。
#[test]
fn foundation_genesis_identity_is_frozen_and_only_wallet_may_change() {
    let foundation = protected_institutions()
        .into_iter()
        .find(|institution| is_private_protected_cid(institution.cid_number.as_bytes()))
        .expect("protected private genesis foundation");
    let admin_key = crate::guard::governance_skeleton::storage_key::account_id(
        foundation.cid_number.as_bytes(),
    );
    let (expected_cid, _, _) = expected_foundation_identity();

    let decode_admins = |state: &BTreeMap<Vec<u8>, Vec<u8>>| -> DecodedPrivateInstitutionAdmins {
        decode_exact(state.get(&admin_key).expect("foundation admins exist"))
            .expect("foundation admins decode")
    };

    // 基线：创世状态必须放行（此时 identity 尚无绑定）。
    assert_eq!(check_state(&valid_state()), Ok(()));

    // 身份三要素逐项冻结：改任一项即判 KnownBad。
    for mutate in [0u8, 1, 2] {
        let mut state = valid_state();
        let mut account = decode_admins(&state);
        match mutate {
            0 => {
                account.admins[0].cid_number = "GZ000-CTZN6-190001010-2026"
                    .as_bytes()
                    .to_vec()
                    .try_into()
                    .expect("cid fits")
            }
            1 => {
                account.admins[0].family_name =
                    "李".as_bytes().to_vec().try_into().expect("name fits")
            }
            _ => {
                account.admins[0].given_name =
                    "四".as_bytes().to_vec().try_into().expect("name fits")
            }
        }
        state.insert(admin_key.clone(), account.encode());
        assert_eq!(
            check_state(&state),
            Err(
                crate::guard::governance_skeleton::GuardError::FoundationGenesisIdentityChanged(
                    foundation.code
                )
            ),
            "身份三要素第 {mutate} 项被改动必须判 KnownBad"
        );
    }

    // 只换钱包账户（名册与三条任职同步换）：必须放行 —— 换钱包不换人。
    let rotated_wallet = [77u8; 32];
    let mut rotated = valid_state();
    let mut account = decode_admins(&rotated);
    account.admins[0].account_id = rotated_wallet;
    rotated.insert(admin_key.clone(), account.encode());
    for role in expected_roles(&foundation) {
        rotated.insert(
            crate::guard::governance_skeleton::storage_key::institution_role_assignments(
                foundation.cid_number.as_bytes(),
                &role.role_code,
            ),
            vec![assignment(&foundation, &role, rotated_wallet)].encode(),
        );
    }
    // 机构信息里的法定代表人账户须一并跟换（既有守卫本就要求 LR 任职与其一致）。
    rotated.insert(
        crate::guard::governance_skeleton::storage_key::private_institution(
            foundation.cid_number.as_bytes(),
        ),
        private_info_bytes(&foundation, rotated_wallet),
    );
    assert_eq!(check_state(&rotated), Ok(()));

    // identity 已登记绑定但指向别的钱包：判 KnownBad（换钱包没同步换绑）。
    let mut mismatched = rotated.clone();
    mismatched.insert(
        crate::guard::governance_skeleton::storage_key::citizen_account_id_by_cid(expected_cid),
        [9u8; 32].encode(),
    );
    assert_eq!(
        check_state(&mismatched),
        Err(
            crate::guard::governance_skeleton::GuardError::FoundationWalletBindingMismatch(
                foundation.code
            )
        )
    );

    // 绑定与名册一致：放行。
    let mut aligned = rotated;
    aligned.insert(
        crate::guard::governance_skeleton::storage_key::citizen_account_id_by_cid(expected_cid),
        rotated_wallet.encode(),
    );
    assert_eq!(check_state(&aligned), Ok(()));
}

#[test]
fn citizenchain_foundation_uses_private_storage_and_allows_one_person_three_roles() {
    let foundation = protected_institutions()
        .into_iter()
        .find(|institution| is_private_protected_cid(institution.cid_number.as_bytes()))
        .expect("protected private genesis foundation");
    assert!(crate::guard::governance_skeleton::storage_key::account_id(
        foundation.cid_number.as_bytes()
    )
    .starts_with(&crate::guard::governance_skeleton::storage_key::private_admin_accounts_prefix()));
    assert!(
        crate::guard::governance_skeleton::storage_key::institution_role(
            foundation.cid_number.as_bytes(),
            primitives::institution_constraints::ROLE_CODE_LEGAL_REPRESENTATIVE,
        )
        .starts_with(
            &crate::guard::governance_skeleton::storage_key::private_institution_roles_prefix()
        )
    );

    let mut missing = valid_state();
    missing.remove(
        &crate::guard::governance_skeleton::storage_key::private_institution(
            foundation.cid_number.as_bytes(),
        ),
    );
    assert_eq!(
        check_state(&missing),
        Err(
            crate::guard::governance_skeleton::GuardError::LegalRepresentativeInfoMissing(
                foundation.code
            )
        )
    );

    let mut mismatched = valid_state();
    mismatched.insert(
        crate::guard::governance_skeleton::storage_key::private_institution(
            foundation.cid_number.as_bytes(),
        ),
        private_info_bytes(&foundation, [250u8; 32]),
    );
    assert_eq!(
        check_state(&mismatched),
        Err(
            crate::guard::governance_skeleton::GuardError::LegalRepresentativeAssignmentMismatch(
                foundation.code
            )
        )
    );

    let mut renamed = valid_state();
    let mut info: DecodedInstitutionInfo = decode_exact(
        renamed
            .get(
                &crate::guard::governance_skeleton::storage_key::private_institution(
                    foundation.cid_number.as_bytes(),
                ),
            )
            .expect("foundation institution info exists"),
    )
    .expect("foundation institution info decodes");
    info.cid_full_name = "错误公司全称".as_bytes().to_vec();
    renamed.insert(
        crate::guard::governance_skeleton::storage_key::private_institution(
            foundation.cid_number.as_bytes(),
        ),
        info.encode(),
    );
    assert_eq!(
        check_state(&renamed),
        Err(
            crate::guard::governance_skeleton::GuardError::InstitutionFullNameChanged(
                foundation.code
            )
        )
    );

    let lr_role = expected_roles(&foundation)
        .into_iter()
        .find(|role| {
            role.role_code == primitives::institution_constraints::ROLE_CODE_LEGAL_REPRESENTATIVE
        })
        .expect("foundation LR role");
    let lr_assignments_key =
        crate::guard::governance_skeleton::storage_key::institution_role_assignments(
            foundation.cid_number.as_bytes(),
            &lr_role.role_code,
        );

    // LR 岗位永久存在，但允许机构法定代表人字段与该岗位任职同时清空。
    let mut vacant = valid_state();
    let info_key = crate::guard::governance_skeleton::storage_key::private_institution(
        foundation.cid_number.as_bytes(),
    );
    let mut info: DecodedInstitutionInfo =
        decode_exact(vacant.get(&info_key).expect("foundation info exists"))
            .expect("foundation info decodes");
    info.legal_representative = None;
    vacant.insert(info_key.clone(), info.encode());
    vacant.insert(
        lr_assignments_key.clone(),
        Vec::<DecodedInstitutionAdminAssignment>::new().encode(),
    );
    assert_eq!(check_state(&vacant), Ok(()));

    // 默认有效状态即是一名管理员同时担任三个固定岗位。
    assert_eq!(check_state(&valid_state()), Ok(()));

    let mut two_representatives = valid_state();
    two_representatives.insert(
        lr_assignments_key,
        vec![
            assignment(&foundation, &lr_role, accounts_for(&foundation)[0]),
            assignment(&foundation, &lr_role, [250u8; 32]),
        ]
        .encode(),
    );
    assert_eq!(
        check_state(&two_representatives),
        Err(
            crate::guard::governance_skeleton::GuardError::SeatsChanged {
                code: foundation.code,
                role_code: lr_role.role_code,
                expected: 1,
                found: 2,
            }
        )
    );
}

#[test]
fn institution_admins_layout_is_exact() {
    let institution = fixed_institutions()[0];
    let raw = account_bytes(&institution, accounts_for(&institution));
    let decoded: DecodedPublicInstitutionAdmins = decode_exact(&raw).expect("layout decodes");
    assert_eq!(decoded.institution_code, institution.code);
    assert_eq!(decoded.admins.len() as u32, institution.expected_len);
}

#[test]
fn public_person_identity_fields_can_remain_empty_without_changing_authority() {
    let institution = fixed_institutions()[0];
    let admin_key = crate::guard::governance_skeleton::storage_key::account_id(
        institution.cid_number.as_bytes(),
    );
    let mut state = valid_state();
    let mut account: DecodedPublicInstitutionAdmins =
        decode_exact(state.get(&admin_key).expect("admin account exists"))
            .expect("admin account decodes");
    account.admins[0].family_name = "张".as_bytes().to_vec().try_into().expect("name fits");
    account.admins[0].given_name = "三".as_bytes().to_vec().try_into().expect("name fits");
    state.insert(admin_key.clone(), account.encode());
    assert_eq!(check_state(&state), Ok(()));

    let mut account: DecodedPublicInstitutionAdmins =
        decode_exact(state.get(&admin_key).expect("admin account exists"))
            .expect("admin account decodes");
    account.admins[0].family_name = Vec::new().try_into().expect("empty name fits");
    account.admins[0].given_name = Default::default();
    account.admins[0].cid_number = Default::default();
    state.insert(admin_key, account.encode());
    assert_eq!(check_state(&state), Ok(()));
}

#[test]
fn missing_or_wrong_code_fixed_account_is_rejected() {
    let institution = fixed_institutions()[0];
    let mut state = valid_state();
    state.remove(&crate::guard::governance_skeleton::storage_key::account_id(
        institution.cid_number.as_bytes(),
    ));
    assert_eq!(
        check_state(&state),
        Err(
            crate::guard::governance_skeleton::GuardError::FixedInstitutionMissing(
                institution.code
            )
        )
    );

    let mut state = valid_state();
    state.insert(
        crate::guard::governance_skeleton::storage_key::account_id(
            institution.cid_number.as_bytes(),
        ),
        DecodedPublicInstitutionAdmins {
            institution_code: *b"BAD\0",
            admins: accounts_for(&institution)
                .into_iter()
                .map(|account_id| Admin {
                    account_id,
                    cid_number: Default::default(),
                    family_name: Default::default(),
                    given_name: Default::default(),
                })
                .collect(),
        }
        .encode(),
    );
    assert_eq!(
        check_state(&state),
        Err(
            crate::guard::governance_skeleton::GuardError::InstitutionCodeChanged(institution.code)
        )
    );
}

#[test]
fn missing_or_renamed_fixed_role_is_rejected_and_dynamic_role_is_allowed() {
    let institution = fixed_institutions()[0];
    let role = expected_roles(&institution)[0].clone();
    let role_key = crate::guard::governance_skeleton::storage_key::institution_role(
        institution.cid_number.as_bytes(),
        &role.role_code,
    );

    let mut state = valid_state();
    state.remove(&role_key);
    assert_eq!(
        check_state(&state),
        Err(crate::guard::governance_skeleton::GuardError::RoleMissing {
            code: institution.code,
            role_code: role.role_code.clone(),
        })
    );

    let mut state = valid_state();
    let mut renamed: DecodedInstitutionRole =
        decode_exact(state.get(&role_key).expect("role exists")).expect("role decodes");
    renamed.role_name = "攻击者岗位".as_bytes().to_vec();
    state.insert(role_key, renamed.encode());
    assert_eq!(
        check_state(&state),
        Err(
            crate::guard::governance_skeleton::GuardError::RoleNameChanged {
                code: institution.code,
                role_code: role.role_code.clone(),
            }
        )
    );

    let mut state = valid_state();
    state.insert(
        crate::guard::governance_skeleton::storage_key::institution_role(
            institution.cid_number.as_bytes(),
            b"EXTRA_ROLE",
        ),
        Vec::new(),
    );
    assert_eq!(check_state(&state), Ok(()));
}

#[test]
fn missing_or_changed_fixed_role_permissions_are_rejected() {
    let institution = fixed_institutions()[0];
    let role = expected_roles(&institution)[0].clone();
    let key = crate::guard::governance_skeleton::storage_key::institution_role_permissions(
        institution.cid_number.as_bytes(),
        &role.role_code,
    );

    let mut missing = valid_state();
    missing.remove(&key);
    assert_eq!(
        check_state(&missing),
        Err(
            crate::guard::governance_skeleton::GuardError::PermissionsMissing {
                code: institution.code,
                role_code: role.role_code.clone(),
            }
        )
    );

    let mut changed = valid_state();
    let mut permissions: Vec<DecodedRoleBusinessPermission> =
        decode_exact(changed.get(&key).expect("permissions exist")).expect("permissions decode");
    permissions[0].business_action_id.action_code += 1;
    changed.insert(key, permissions.encode());
    assert_eq!(
        check_state(&changed),
        Err(
            crate::guard::governance_skeleton::GuardError::PermissionsChanged {
                code: institution.code,
                role_code: role.role_code,
            }
        )
    );
}

#[test]
fn changed_seat_count_or_non_admin_assignment_is_rejected() {
    let institution = fixed_institutions()[0];
    let role = expected_roles(&institution)[0].clone();
    let assignments_key =
        crate::guard::governance_skeleton::storage_key::institution_role_assignments(
            institution.cid_number.as_bytes(),
            &role.role_code,
        );

    let mut state = valid_state();
    let mut assignments: Vec<DecodedInstitutionAdminAssignment> =
        decode_exact(state.get(&assignments_key).expect("assignments exist"))
            .expect("assignments decode");
    assignments.pop();
    state.insert(assignments_key.clone(), assignments.encode());
    assert_eq!(
        check_state(&state),
        Err(
            crate::guard::governance_skeleton::GuardError::SeatsChanged {
                code: institution.code,
                role_code: role.role_code.clone(),
                expected: role.seats,
                found: role.seats - 1,
            }
        )
    );

    let mut state = valid_state();
    let mut assignments: Vec<DecodedInstitutionAdminAssignment> =
        decode_exact(state.get(&assignments_key).expect("assignments exist"))
            .expect("assignments decode");
    assignments[0].account_id = [250u8; 32];
    state.insert(assignments_key, assignments.encode());
    assert_eq!(
        check_state(&state),
        Err(
            crate::guard::governance_skeleton::GuardError::AdminAssignmentSetMismatch(
                institution.code
            )
        )
    );
}

#[test]
fn member_source_and_term_fields_are_outside_native_structure_guard() {
    let institution = fixed_institutions()[0];
    let role = expected_roles(&institution)[0].clone();
    let role_key = crate::guard::governance_skeleton::storage_key::institution_role(
        institution.cid_number.as_bytes(),
        &role.role_code,
    );
    let assignments_key =
        crate::guard::governance_skeleton::storage_key::institution_role_assignments(
            institution.cid_number.as_bytes(),
            &role.role_code,
        );
    let admin_key = crate::guard::governance_skeleton::storage_key::account_id(
        institution.cid_number.as_bytes(),
    );
    let mut state = valid_state();

    let mut role_value: DecodedInstitutionRole =
        decode_exact(state.get(&role_key).expect("role exists")).expect("role decodes");
    role_value.term_required = true;
    state.insert(role_key, role_value.encode());

    let mut assignments: Vec<DecodedInstitutionAdminAssignment> =
        decode_exact(state.get(&assignments_key).expect("assignments exist"))
            .expect("assignments decode");
    for assignment in &mut assignments {
        assignment.term_start = 10;
        assignment.term_end = 5;
        assignment.assignment_source = InstitutionAssignmentSource::PopularElection;
        assignment.assignment_source_ref = b"VOTE-1".to_vec();
    }
    assignments[0].account_id = [250u8; 32];
    state.insert(assignments_key, assignments.encode());

    let mut account: DecodedPublicInstitutionAdmins =
        decode_exact(state.get(&admin_key).expect("admin account exists"))
            .expect("admin account decodes");
    account.admins[0].account_id = [250u8; 32];
    state.insert(admin_key, account.encode());

    assert_eq!(check_state(&state), Ok(()));
}

#[test]
fn raw_key_derivation_and_trigger_prefixes_are_stable() {
    let account = b"CID-KEY";
    let mut expected_admin = twox_128(b"PublicAdmins").to_vec();
    expected_admin.extend_from_slice(&twox_128(b"AdminAccounts"));
    let encoded_account = account.to_vec().encode();
    expected_admin.extend_from_slice(&blake2_128(&encoded_account));
    expected_admin.extend_from_slice(&encoded_account);
    assert_eq!(
        crate::guard::governance_skeleton::storage_key::account_id(account),
        expected_admin
    );

    let cid = b"CID-1".to_vec().encode();
    let role = b"ROLE-1".to_vec().encode();
    let mut expected_role = twox_128(b"PublicManage").to_vec();
    expected_role.extend_from_slice(&twox_128(b"InstitutionRoles"));
    expected_role.extend_from_slice(&blake2_128(&cid));
    expected_role.extend_from_slice(&cid);
    expected_role.extend_from_slice(&blake2_128(&role));
    expected_role.extend_from_slice(&role);
    assert_eq!(
        crate::guard::governance_skeleton::storage_key::institution_role(b"CID-1", b"ROLE-1"),
        expected_role
    );

    let mut delta = BTreeMap::new();
    delta.insert(expected_role, Some(Vec::new()));
    assert!(
        !needs_full_check(&delta),
        "普通机构岗位变化不得触发创世治理骨架"
    );
    let protected = fixed_institutions()[0];
    delta.insert(
        crate::guard::governance_skeleton::storage_key::institution_role(
            protected.cid_number.as_bytes(),
            &expected_roles(&protected)[0].role_code,
        ),
        Some(Vec::new()),
    );
    assert!(needs_full_check(&delta));
    assert!(crate::guard::governance_skeleton::storage_key::fixed_catalog_prefixes().len() >= 2);
}

#[test]
fn ordinary_block_checks_only_the_affected_protected_institution() {
    let fixed = fixed_institutions();
    let affected = fixed[0];
    let unrelated = fixed[1];
    let mut state = valid_state();
    state.remove(&crate::guard::governance_skeleton::storage_key::account_id(
        unrelated.cid_number.as_bytes(),
    ));

    let delta = BTreeMap::from([(
        crate::guard::governance_skeleton::storage_key::account_id(affected.cid_number.as_bytes()),
        state
            .get(&crate::guard::governance_skeleton::storage_key::account_id(
                affected.cid_number.as_bytes(),
            ))
            .cloned(),
    )]);
    assert_eq!(
        check_affected_institutions(&delta, |key| state.get(key).cloned()),
        Ok(())
    );
    assert_eq!(
        check_skeleton_invariants(|key| state.get(key).cloned()),
        Err(crate::guard::governance_skeleton::GuardError::FixedInstitutionMissing(unrelated.code))
    );
}

#[test]
fn malformed_extra_role_key_is_ignored_but_trailing_value_is_rejected() {
    let mut malformed = crate::guard::governance_skeleton::storage_key::institution_roles_prefix();
    malformed.extend_from_slice(b"bad");
    assert_eq!(check_catalog_keys([malformed]), Ok(()));

    let institution = fixed_institutions()[0];
    let role = expected_roles(&institution)[0].clone();
    let role_key = crate::guard::governance_skeleton::storage_key::institution_role(
        institution.cid_number.as_bytes(),
        &role.role_code,
    );
    let mut state = valid_state();
    state.get_mut(&role_key).expect("role exists").push(0);
    assert_eq!(
        check_state(&state),
        Err(
            crate::guard::governance_skeleton::GuardError::RoleDecodeFailed {
                code: institution.code,
                role_code: role.role_code,
            }
        )
    );
}
