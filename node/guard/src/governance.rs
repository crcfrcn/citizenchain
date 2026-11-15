use adminbase::Admin;
use adminbase::InstitutionAdmins;
use codec::Decode;
#[cfg(test)]
use codec::Encode;
use entitybase::BusinessActionId;
use entitybase::InstitutionAdminAssignment as SharedInstitutionAdminAssignment;
#[cfg(test)]
use entitybase::InstitutionAssignmentSource;
use entitybase::InstitutionAssignmentStatus;
use entitybase::InstitutionRole as SharedInstitutionRole;
use entitybase::InstitutionRoleStatus;
use entitybase::RoleBusinessPermission;
use entitybase::RoleSubject;
use primitives::cid::china::citizenchain::CITIZENCHAIN_FIXED_ROLES;
use primitives::cid::china::citizenchain::CITIZENCHAIN_FOUNDATION;
use primitives::cid::china::citizenchain::CITIZENCHAIN_GENESIS_ADMINS;
use primitives::cid::china::citizenchain::LEGAL_REPRESENTATIVE_CITIZEN_CID_NUMBER;
use primitives::cid::code::FRG;
use primitives::cid::code::PROVINCE_CODE_INFOS;
use primitives::count_const::FRG_PROVINCE_GROUP_ADMIN_COUNT;
use primitives::governance_skeleton::fixed_institutions;
use primitives::governance_skeleton::fixed_role_specs;
use primitives::governance_skeleton::province_commissioner_role_code;
use primitives::governance_skeleton::province_commissioner_role_name;
use primitives::governance_skeleton::FixedInstitution;
use std::collections::BTreeMap;
use std::collections::BTreeSet;

const PUBLIC_ADMINS_PALLET: &[u8] = b"PublicAdmins";

const PUBLIC_MANAGE_PALLET: &[u8] = b"PublicManage";

const PRIVATE_ADMINS_PALLET: &[u8] = b"PrivateAdmins";

const PRIVATE_MANAGE_PALLET: &[u8] = b"PrivateManage";

const CITIZEN_IDENTITY_PALLET: &[u8] = b"CitizenIdentity";

#[derive(Clone, Debug, Eq, PartialEq)]
struct ExpectedRole {
    role_code: Vec<u8>,
    role_name: Vec<u8>,
    seats: u32,
}

fn protected_institutions() -> Vec<FixedInstitution> {
    let mut institutions = fixed_institutions();
    institutions.push(FixedInstitution {
        code: *b"SFGY",
        main_account: CITIZENCHAIN_FOUNDATION.main_account,
        cid_number: CITIZENCHAIN_FOUNDATION.cid_number,
        expected_len: 1,
    });
    institutions
}

fn is_private_protected_cid(cid_number: &[u8]) -> bool {
    cid_number == CITIZENCHAIN_FOUNDATION.cid_number.as_bytes()
}

/// 基金会创世管理员的**永久冻结身份**:公民 CID + 姓 + 名。
///
/// 三个创世岗位(`LR` / `GENESIS_PRODUCT_MANAGER` / `GENESIS_PROGRAMMER`)永久由
/// **同一位公民**担任,钱包账户可依法更换,身份三要素永不可变——「换钱包不换人」。
/// 「只能一个人」由既有的 `expected_len = 1` + 每岗 `seats = 1` + 任职账户必须来自
/// 名册集合共同锁死,本函数只补上「这个人是谁」。
///
/// 单源沿用创世 seeder 同一组常量:CID 复用法定代表人真源,姓名取创世管理员表,
/// 不在守卫里另造第二份身份数据。
fn expected_foundation_identity() -> (&'static [u8], &'static [u8], &'static [u8]) {
    let admin = &CITIZENCHAIN_GENESIS_ADMINS[0];
    (
        LEGAL_REPRESENTATIVE_CITIZEN_CID_NUMBER.as_bytes(),
        admin.family_name.as_bytes(),
        admin.given_name.as_bytes(),
    )
}

fn expected_roles(institution: &FixedInstitution) -> Vec<ExpectedRole> {
    if is_private_protected_cid(institution.cid_number.as_bytes()) {
        return CITIZENCHAIN_FIXED_ROLES
            .iter()
            .map(|role| ExpectedRole {
                role_code: role.role_code.to_vec(),
                role_name: role.role_name.to_vec(),
                seats: role.seats,
            })
            .collect();
    }
    let code = institution.code;
    if code == FRG {
        let mut roles = PROVINCE_CODE_INFOS
            .iter()
            .map(|province| ExpectedRole {
                role_code: province_commissioner_role_code(province.province_code),
                role_name: province_commissioner_role_name(province.province_name),
                seats: FRG_PROVINCE_GROUP_ADMIN_COUNT,
            })
            .collect::<Vec<_>>();
        roles.push(ExpectedRole {
            role_code: primitives::institution_constraints::ROLE_CODE_LEGAL_REPRESENTATIVE.to_vec(),
            role_name: primitives::institution_constraints::ROLE_NAME_LEGAL_REPRESENTATIVE.to_vec(),
            seats: 0,
        });
        return roles;
    }
    fixed_role_specs(code)
        .into_iter()
        .map(|role| ExpectedRole {
            role_code: role.role_code.to_vec(),
            role_name: role.role_name.to_vec(),
            seats: role.seats,
        })
        .collect()
}

/// 节点直接使用共享协议类型解码，避免维护第二份字段顺序和枚举判别值。
type DecodedPrivateInstitutionAdmins = InstitutionAdmins<Vec<Admin<[u8; 32]>>>;

type DecodedPublicInstitutionAdmins = InstitutionAdmins<Vec<Admin<[u8; 32]>>>;

type DecodedInstitutionRole = SharedInstitutionRole<Vec<u8>, Vec<u8>, Vec<u8>>;

type DecodedInstitutionAdminAssignment =
    SharedInstitutionAdminAssignment<Vec<u8>, [u8; 32], Vec<u8>, Vec<u8>>;

type DecodedRoleBusinessPermission = RoleBusinessPermission<Vec<u8>, Vec<u8>, Vec<u8>>;

type DecodedInstitutionInfo = entitybase::InstitutionInfo<u32, Vec<u8>, Vec<u8>, [u8; 32]>;

/// 固定治理骨架校验失败原因。
#[derive(Debug, PartialEq)]
pub(crate) enum GuardError {
    FixedInstitutionMissing([u8; 4]),
    AdminAccountDecodeFailed([u8; 4]),
    InstitutionCodeChanged([u8; 4]),
    AdminsLenChanged {
        code: [u8; 4],
        expected: u32,
        found: u32,
    },
    InvalidAdminPersonName([u8; 4]),
    /// 基金会创世管理员的身份三要素(公民 CID / 姓 / 名)被篡改。
    /// 三个创世岗位永久由同一位公民担任,只允许更换钱包账户。
    FoundationGenesisIdentityChanged([u8; 4]),
    /// 基金会名册账户与该公民 CID 在 identity 中当前绑定的钱包不一致。
    /// 换钱包必须同步换绑,不允许只换其一。
    FoundationWalletBindingMismatch([u8; 4]),
    DuplicateAdminAccountId([u8; 4]),
    RoleMissing {
        code: [u8; 4],
        role_code: Vec<u8>,
    },
    RoleDecodeFailed {
        code: [u8; 4],
        role_code: Vec<u8>,
    },
    RoleCidChanged {
        code: [u8; 4],
        role_code: Vec<u8>,
    },
    RoleCodeChanged {
        code: [u8; 4],
        role_code: Vec<u8>,
    },
    RoleNameChanged {
        code: [u8; 4],
        role_code: Vec<u8>,
    },
    RoleNotActive {
        code: [u8; 4],
        role_code: Vec<u8>,
    },
    AssignmentsMissing {
        code: [u8; 4],
        role_code: Vec<u8>,
    },
    AssignmentsDecodeFailed {
        code: [u8; 4],
        role_code: Vec<u8>,
    },
    PermissionsMissing {
        code: [u8; 4],
        role_code: Vec<u8>,
    },
    PermissionsDecodeFailed {
        code: [u8; 4],
        role_code: Vec<u8>,
    },
    PermissionsChanged {
        code: [u8; 4],
        role_code: Vec<u8>,
    },
    SeatsChanged {
        code: [u8; 4],
        role_code: Vec<u8>,
        expected: u32,
        found: u32,
    },
    AssignmentCidChanged {
        code: [u8; 4],
        role_code: Vec<u8>,
    },
    AssignmentRoleChanged {
        code: [u8; 4],
        role_code: Vec<u8>,
    },
    AssignmentNotActive {
        code: [u8; 4],
        role_code: Vec<u8>,
    },
    DuplicateAssignmentAccountId([u8; 4]),
    AdminAssignmentSetMismatch([u8; 4]),
    LegalRepresentativeInfoMissing([u8; 4]),
    LegalRepresentativeAssignmentMismatch([u8; 4]),
    InstitutionFullNameChanged([u8; 4]),
}

fn decode_exact<T: Decode>(raw: &[u8]) -> Result<T, ()> {
    let mut input = raw;
    let value = T::decode(&mut input).map_err(|_| ())?;
    if !input.is_empty() {
        return Err(());
    }
    Ok(value)
}

fn decode_concat_vec(input: &[u8]) -> Result<(Vec<u8>, usize), ()> {
    if input.len() < 16 {
        return Err(());
    }
    let hash = &input[..16];
    let encoded = &input[16..];
    let mut remaining = encoded;
    let value = Vec::<u8>::decode(&mut remaining).map_err(|_| ())?;
    let consumed = encoded.len().saturating_sub(remaining.len());
    if sp_crypto_hashing::blake2_128(&encoded[..consumed]) != hash {
        return Err(());
    }
    Ok((value, 16 + consumed))
}

fn parse_double_map_key(key: &[u8], prefix: &[u8]) -> Result<(Vec<u8>, Vec<u8>), ()> {
    let mut remaining = key.strip_prefix(prefix).ok_or(())?;
    let (cid_number, cid_consumed) = decode_concat_vec(remaining)?;
    remaining = &remaining[cid_consumed..];
    let (role_code, role_consumed) = decode_concat_vec(remaining)?;
    remaining = &remaining[role_consumed..];
    if !remaining.is_empty() {
        return Err(());
    }
    Ok((cid_number, role_code))
}

/// 解析岗位、任职和权限 RAW key 形态。
///
/// 固定岗位逐项强校验；受保护机构依法新增的动态岗位由 runtime 治理，节点不得误判为
/// 非法“额外固定岗位”。无法解析的非规范 key 同样交由 FRAME/runtime 处理。
pub(crate) fn check_catalog_keys<I, K>(keys: I) -> Result<(), GuardError>
where
    I: IntoIterator<Item = K>,
    K: AsRef<[u8]>,
{
    let prefixes = [
        crate::guard::governance_skeleton::storage_key::institution_roles_prefix(),
        crate::guard::governance_skeleton::storage_key::private_institution_roles_prefix(),
        crate::guard::governance_skeleton::storage_key::institution_role_assignments_prefix(),
        crate::guard::governance_skeleton::storage_key::private_institution_role_assignments_prefix(
        ),
        crate::guard::governance_skeleton::storage_key::institution_role_permissions_prefix(),
        crate::guard::governance_skeleton::storage_key::private_institution_role_permissions_prefix(
        ),
    ];

    for raw_key in keys {
        let key = raw_key.as_ref();
        let _parsed = prefixes
            .iter()
            .find(|prefix| key.starts_with(prefix.as_slice()))
            .map(|prefix| parse_double_map_key(key, prefix));
    }
    Ok(())
}

/// 校验全部 90 个受保护创世机构的管理员人员集合、固定岗位和任职席位。
///
/// 固定岗位代码、名称、所属机构和席位数不可改变；管理员账户可以依法更新。任职来源、
/// 来源引用与任期只要求共享 SCALE 能完整解码，具体业务合法性由 runtime 与投票引擎负责。
pub(crate) fn check_skeleton_invariants<F>(read_raw: F) -> Result<(), GuardError>
where
    F: Fn(&[u8]) -> Option<Vec<u8>>,
{
    for institution in protected_institutions() {
        check_institution_invariants(&institution, &read_raw)?;
    }
    Ok(())
}

/// 只校验一个受保护创世机构，供普通区块按受影响身份精确执行。
fn check_institution_invariants<F>(
    institution: &FixedInstitution,
    read_raw: &F,
) -> Result<(), GuardError>
where
    F: Fn(&[u8]) -> Option<Vec<u8>>,
{
    let expected_cid = institution.cid_number.as_bytes();
    let private_info = if is_private_protected_cid(expected_cid) {
        let raw = read_raw(
            &crate::guard::governance_skeleton::storage_key::private_institution(expected_cid),
        )
        .ok_or(
            crate::guard::governance_skeleton::GuardError::LegalRepresentativeInfoMissing(
                institution.code,
            ),
        )?;
        let info: DecodedInstitutionInfo = decode_exact(&raw).map_err(|_| {
            crate::guard::governance_skeleton::GuardError::LegalRepresentativeInfoMissing(
                institution.code,
            )
        })?;
        let legal_representative_fields_valid =
            info.legal_representative
                .as_ref()
                .is_none_or(|representative| {
                    !representative.family_name.is_empty()
                        && !representative.given_name.is_empty()
                        && !representative.cid_number.is_empty()
                });
        if info.institution_code != institution.code || !legal_representative_fields_valid {
            return Err(
                crate::guard::governance_skeleton::GuardError::LegalRepresentativeInfoMissing(
                    institution.code,
                ),
            );
        }
        if info.cid_full_name != CITIZENCHAIN_FOUNDATION.cid_full_name.as_bytes() {
            return Err(
                crate::guard::governance_skeleton::GuardError::InstitutionFullNameChanged(
                    institution.code,
                ),
            );
        }
        Some(info)
    } else {
        None
    };
    let raw = read_raw(&crate::guard::governance_skeleton::storage_key::account_id(
        expected_cid,
    ))
    .ok_or(
        crate::guard::governance_skeleton::GuardError::FixedInstitutionMissing(institution.code),
    )?;
    let (stored_code, admin_account_ids, invalid_private_name) =
        if is_private_protected_cid(expected_cid) {
            let account: DecodedPrivateInstitutionAdmins = decode_exact(&raw).map_err(|_| {
                crate::guard::governance_skeleton::GuardError::AdminAccountDecodeFailed(
                    institution.code,
                )
            })?;
            let invalid_name = account.admins.iter().any(|admin| {
                admin.family_name.is_empty()
                    || admin.given_name.is_empty()
                    || core::str::from_utf8(admin.family_name.as_slice()).is_err()
                    || core::str::from_utf8(admin.given_name.as_slice()).is_err()
            });
            // 冻结「人」：无论钱包账户怎么换，公民 CID、姓、名必须逐字节等于创世常量。
            // 岗位码/岗位名/席位已由 CITIZENCHAIN_FIXED_ROLES 冻结，此处补上身份三要素。
            let (expected_cid, expected_family, expected_given) = expected_foundation_identity();
            if account.admins.iter().any(|admin| {
                admin.cid_number.as_slice() != expected_cid
                    || admin.family_name.as_slice() != expected_family
                    || admin.given_name.as_slice() != expected_given
            }) {
                return Err(
                    crate::guard::governance_skeleton::GuardError::FoundationGenesisIdentityChanged(
                        institution.code,
                    ),
                );
            }
            // 换钱包必须同步换绑：名册账户须等于该公民 CID 当前绑定的签名账户。
            // 该绑定由 identity 在运行期登记；创世不播种，故「无绑定」放行，
            // 只有已登记却指向别的账户时才判定篡改（无条件校验会否掉创世状态本身）。
            if let Some(bound_raw) = read_raw(
                &crate::guard::governance_skeleton::storage_key::citizen_account_id_by_cid(
                    expected_cid,
                ),
            ) {
                let bound: [u8; 32] = decode_exact(&bound_raw).map_err(|_| {
                    crate::guard::governance_skeleton::GuardError::AdminAccountDecodeFailed(
                        institution.code,
                    )
                })?;
                if account.admins.iter().any(|admin| admin.account_id != bound) {
                    return Err(
                    crate::guard::governance_skeleton::GuardError::FoundationWalletBindingMismatch(
                        institution.code,
                    ),
                );
                }
            }
            (
                account.institution_code,
                account
                    .admins
                    .into_iter()
                    .map(|admin| admin.account_id)
                    .collect::<Vec<_>>(),
                invalid_name,
            )
        } else {
            let account: DecodedPublicInstitutionAdmins = decode_exact(&raw).map_err(|_| {
                crate::guard::governance_skeleton::GuardError::AdminAccountDecodeFailed(
                    institution.code,
                )
            })?;
            (
                account.institution_code,
                account
                    .admins
                    .into_iter()
                    .map(|admin| admin.account_id)
                    .collect::<Vec<_>>(),
                false,
            )
        };
    if stored_code != institution.code {
        return Err(
            crate::guard::governance_skeleton::GuardError::InstitutionCodeChanged(institution.code),
        );
    }
    let found = admin_account_ids.len() as u32;
    if found != institution.expected_len {
        return Err(
            crate::guard::governance_skeleton::GuardError::AdminsLenChanged {
                code: institution.code,
                expected: institution.expected_len,
                found,
            },
        );
    }
    if invalid_private_name {
        return Err(
            crate::guard::governance_skeleton::GuardError::InvalidAdminPersonName(institution.code),
        );
    }
    let admin_set = admin_account_ids.iter().copied().collect::<BTreeSet<_>>();
    if admin_set.len() != admin_account_ids.len() {
        return Err(
            crate::guard::governance_skeleton::GuardError::DuplicateAdminAccountId(
                institution.code,
            ),
        );
    }

    let mut legal_representative_assignment = None;
    for expected_role in expected_roles(institution) {
        let role_key = crate::guard::governance_skeleton::storage_key::institution_role(
            expected_cid,
            &expected_role.role_code,
        );
        let role_raw = read_raw(&role_key).ok_or_else(|| {
            crate::guard::governance_skeleton::GuardError::RoleMissing {
                code: institution.code,
                role_code: expected_role.role_code.clone(),
            }
        })?;
        let role: DecodedInstitutionRole = decode_exact(&role_raw).map_err(|_| {
            crate::guard::governance_skeleton::GuardError::RoleDecodeFailed {
                code: institution.code,
                role_code: expected_role.role_code.clone(),
            }
        })?;
        if role.cid_number != expected_cid {
            return Err(
                crate::guard::governance_skeleton::GuardError::RoleCidChanged {
                    code: institution.code,
                    role_code: expected_role.role_code,
                },
            );
        }
        if role.role_code != expected_role.role_code {
            return Err(
                crate::guard::governance_skeleton::GuardError::RoleCodeChanged {
                    code: institution.code,
                    role_code: expected_role.role_code,
                },
            );
        }
        if role.role_name != expected_role.role_name {
            return Err(
                crate::guard::governance_skeleton::GuardError::RoleNameChanged {
                    code: institution.code,
                    role_code: expected_role.role_code,
                },
            );
        }
        if role.role_status != InstitutionRoleStatus::Active {
            return Err(
                crate::guard::governance_skeleton::GuardError::RoleNotActive {
                    code: institution.code,
                    role_code: expected_role.role_code,
                },
            );
        }

        let permissions_key =
            crate::guard::governance_skeleton::storage_key::institution_role_permissions(
                expected_cid,
                &expected_role.role_code,
            );
        let permissions_raw = read_raw(&permissions_key).ok_or_else(|| {
            crate::guard::governance_skeleton::GuardError::PermissionsMissing {
                code: institution.code,
                role_code: expected_role.role_code.clone(),
            }
        })?;
        let permissions: Vec<DecodedRoleBusinessPermission> = decode_exact(&permissions_raw)
            .map_err(|_| {
                crate::guard::governance_skeleton::GuardError::PermissionsDecodeFailed {
                    code: institution.code,
                    role_code: expected_role.role_code.clone(),
                }
            })?;
        let expected_permissions = entitybase::fixed_role_permission_specs(
            institution.code,
            expected_cid,
            &expected_role.role_code,
        )
        .into_iter()
        .map(|permission| RoleBusinessPermission {
            role_subject: RoleSubject {
                cid_number: expected_cid.to_vec(),
                role_code: expected_role.role_code.clone(),
            },
            business_action_id: BusinessActionId {
                module_tag: permission.module_tag.to_vec(),
                action_code: permission.action_code,
            },
            operation: permission.operation,
        })
        .collect::<Vec<_>>();
        if permissions != expected_permissions {
            return Err(
                crate::guard::governance_skeleton::GuardError::PermissionsChanged {
                    code: institution.code,
                    role_code: expected_role.role_code.clone(),
                },
            );
        }

        let assignments_key =
            crate::guard::governance_skeleton::storage_key::institution_role_assignments(
                expected_cid,
                &expected_role.role_code,
            );
        let assignments_raw = read_raw(&assignments_key).ok_or_else(|| {
            crate::guard::governance_skeleton::GuardError::AssignmentsMissing {
                code: institution.code,
                role_code: expected_role.role_code.clone(),
            }
        })?;
        let assignments: Vec<DecodedInstitutionAdminAssignment> = decode_exact(&assignments_raw)
            .map_err(|_| {
                crate::guard::governance_skeleton::GuardError::AssignmentsDecodeFailed {
                    code: institution.code,
                    role_code: expected_role.role_code.clone(),
                }
            })?;
        let found = assignments.len() as u32;
        let is_legal_representative = expected_role.role_code
            == primitives::institution_constraints::ROLE_CODE_LEGAL_REPRESENTATIVE;
        let (min_assignments, max_assignments) = if is_private_protected_cid(expected_cid) {
            if is_legal_representative {
                (0, 1)
            } else {
                (expected_role.seats, expected_role.seats)
            }
        } else {
            primitives::governance_skeleton::fixed_role_assignment_bounds_by_identity(
                institution.code,
                expected_cid,
                &expected_role.role_code,
            )
            .ok_or_else(|| {
                crate::guard::governance_skeleton::GuardError::RoleMissing {
                    code: institution.code,
                    role_code: expected_role.role_code.clone(),
                }
            })?
        };
        if found < min_assignments || found > max_assignments {
            return Err(
                crate::guard::governance_skeleton::GuardError::SeatsChanged {
                    code: institution.code,
                    role_code: expected_role.role_code,
                    expected: max_assignments,
                    found,
                },
            );
        }
        let mut role_assignment_account_ids = BTreeSet::new();
        for assignment in assignments {
            if assignment.cid_number != expected_cid {
                return Err(
                    crate::guard::governance_skeleton::GuardError::AssignmentCidChanged {
                        code: institution.code,
                        role_code: expected_role.role_code,
                    },
                );
            }
            if assignment.role_code != expected_role.role_code {
                return Err(
                    crate::guard::governance_skeleton::GuardError::AssignmentRoleChanged {
                        code: institution.code,
                        role_code: expected_role.role_code,
                    },
                );
            }
            if assignment.assignment_status != InstitutionAssignmentStatus::Active {
                return Err(
                    crate::guard::governance_skeleton::GuardError::AssignmentNotActive {
                        code: institution.code,
                        role_code: expected_role.role_code,
                    },
                );
            }
            if !admin_set.contains(&assignment.account_id) {
                return Err(
                    crate::guard::governance_skeleton::GuardError::AdminAssignmentSetMismatch(
                        institution.code,
                    ),
                );
            }
            if !role_assignment_account_ids.insert(assignment.account_id) {
                return Err(
                    crate::guard::governance_skeleton::GuardError::DuplicateAssignmentAccountId(
                        institution.code,
                    ),
                );
            }
            if is_legal_representative {
                legal_representative_assignment = Some(assignment.account_id);
            }
        }
    }
    if let Some(info) = private_info {
        if info
            .legal_representative
            .as_ref()
            .map(|representative| representative.account_id)
            != legal_representative_assignment
        {
            return Err(crate::guard::governance_skeleton::GuardError::LegalRepresentativeAssignmentMismatch(
                institution.code,
            ));
        }
    }
    Ok(())
}

/// 普通区块仅复核实际被修改的受保护创世机构；runtime 升级仍全量复核。
pub(super) fn check_affected_institutions<F>(
    delta: &BTreeMap<Vec<u8>, Option<Vec<u8>>>,
    read_raw: F,
) -> Result<(), GuardError>
where
    F: Fn(&[u8]) -> Option<Vec<u8>>,
{
    if delta.contains_key(sp_storage::well_known_keys::CODE) {
        return check_skeleton_invariants(read_raw);
    }
    let mut affected = BTreeMap::new();
    for key in delta.keys() {
        if let Some(institution) =
            crate::guard::governance_skeleton::storage_key::protected_institution_for_key(key)
        {
            // 机构唯一主键始终是 CID；主账户只是该 CID 下的一种协议账户，不能用于机构去重。
            affected.insert(institution.cid_number, institution);
        }
    }
    for institution in affected.values() {
        check_institution_invariants(institution, &read_raw)?;
    }
    Ok(())
}

/// 只有受保护机构治理 key 或 runtime code 变化时才复核治理骨架。
pub(super) fn needs_full_check(delta: &BTreeMap<Vec<u8>, Option<Vec<u8>>>) -> bool {
    delta
        .keys()
        .any(|key| crate::guard::governance_skeleton::storage_key::is_relevant(key))
        || delta.contains_key(sp_storage::well_known_keys::CODE)
}

/// 三张受保护 storage 的完整 RAW key 和精确前缀。
pub mod storage_key {
    use crate::guard::governance_skeleton::is_private_protected_cid;
    use crate::guard::governance_skeleton::protected_institutions;
    use crate::guard::governance_skeleton::CITIZEN_IDENTITY_PALLET;
    use crate::guard::governance_skeleton::PRIVATE_ADMINS_PALLET;
    use crate::guard::governance_skeleton::PRIVATE_MANAGE_PALLET;
    use crate::guard::governance_skeleton::PUBLIC_ADMINS_PALLET;
    use crate::guard::governance_skeleton::PUBLIC_MANAGE_PALLET;
    use codec::Decode;
    use codec::Encode;
    use primitives::governance_skeleton::FixedInstitution;
    use sp_crypto_hashing::blake2_128;

    fn storage_prefix(pallet: &[u8], storage: &[u8]) -> Vec<u8> {
        nodedesktop::storage::prefix(pallet, storage)
    }

    fn blake2_128_concat(encoded: &[u8]) -> Vec<u8> {
        nodedesktop::storage::blake2_128_concat(encoded)
    }

    pub fn admin_accounts_prefix() -> Vec<u8> {
        storage_prefix(PUBLIC_ADMINS_PALLET, b"AdminAccounts")
    }

    pub fn private_admin_accounts_prefix() -> Vec<u8> {
        storage_prefix(PRIVATE_ADMINS_PALLET, b"AdminAccounts")
    }

    pub fn institution_roles_prefix() -> Vec<u8> {
        storage_prefix(PUBLIC_MANAGE_PALLET, b"InstitutionRoles")
    }

    pub fn institution_role_assignments_prefix() -> Vec<u8> {
        storage_prefix(PUBLIC_MANAGE_PALLET, b"InstitutionRoleAssignments")
    }

    pub fn private_institution_roles_prefix() -> Vec<u8> {
        storage_prefix(PRIVATE_MANAGE_PALLET, b"InstitutionRoles")
    }

    pub fn private_institution_role_assignments_prefix() -> Vec<u8> {
        storage_prefix(PRIVATE_MANAGE_PALLET, b"InstitutionRoleAssignments")
    }

    pub fn institution_role_permissions_prefix() -> Vec<u8> {
        storage_prefix(PUBLIC_MANAGE_PALLET, b"InstitutionRolePermissions")
    }

    pub fn private_institution_role_permissions_prefix() -> Vec<u8> {
        storage_prefix(PRIVATE_MANAGE_PALLET, b"InstitutionRolePermissions")
    }

    pub fn private_institutions_prefix() -> Vec<u8> {
        storage_prefix(PRIVATE_MANAGE_PALLET, b"Institutions")
    }

    pub fn private_institution(cid_number: &[u8]) -> Vec<u8> {
        let mut key = private_institutions_prefix();
        key.extend_from_slice(&blake2_128_concat(&cid_number.to_vec().encode()));
        key
    }

    /// `CitizenIdentity::AccountIdByCid[公民CID]`：该公民 CID 当前绑定的唯一签名账户。
    /// key 与 runtime 的 `Blake2_128Concat` + `BoundedVec` 编码逐字节一致
    /// （`blake2_128(Compact(len)||bytes) || Compact(len)||bytes`）。
    pub fn citizen_account_id_by_cid(citizen_cid_number: &[u8]) -> Vec<u8> {
        let mut key = storage_prefix(CITIZEN_IDENTITY_PALLET, b"AccountIdByCid");
        key.extend_from_slice(&blake2_128_concat(&citizen_cid_number.to_vec().encode()));
        key
    }

    pub fn account_id(cid_number: &[u8]) -> Vec<u8> {
        let mut key = if is_private_protected_cid(cid_number) {
            private_admin_accounts_prefix()
        } else {
            admin_accounts_prefix()
        };
        key.extend_from_slice(&blake2_128_concat(&cid_number.to_vec().encode()));
        key
    }

    fn admin_cid_from_key(key: &[u8], prefix: &[u8]) -> Option<Vec<u8>> {
        let encoded = key.strip_prefix(prefix)?;
        if encoded.len() < 17 || blake2_128(&encoded[16..]) != encoded[..16] {
            return None;
        }
        let mut input = &encoded[16..];
        let cid_number = Vec::<u8>::decode(&mut input).ok()?;
        if !input.is_empty() {
            return None;
        }
        Some(cid_number)
    }

    fn double_map_key(storage_prefix: Vec<u8>, cid_number: &[u8], role_code: &[u8]) -> Vec<u8> {
        let mut key = storage_prefix;
        key.extend_from_slice(&blake2_128_concat(&cid_number.to_vec().encode()));
        key.extend_from_slice(&blake2_128_concat(&role_code.to_vec().encode()));
        key
    }

    pub fn institution_role(cid_number: &[u8], role_code: &[u8]) -> Vec<u8> {
        let prefix = if is_private_protected_cid(cid_number) {
            private_institution_roles_prefix()
        } else {
            institution_roles_prefix()
        };
        double_map_key(prefix, cid_number, role_code)
    }

    pub fn institution_role_assignments(cid_number: &[u8], role_code: &[u8]) -> Vec<u8> {
        let prefix = if is_private_protected_cid(cid_number) {
            private_institution_role_assignments_prefix()
        } else {
            institution_role_assignments_prefix()
        };
        double_map_key(prefix, cid_number, role_code)
    }

    pub fn institution_role_permissions(cid_number: &[u8], role_code: &[u8]) -> Vec<u8> {
        let prefix = if is_private_protected_cid(cid_number) {
            private_institution_role_permissions_prefix()
        } else {
            institution_role_permissions_prefix()
        };
        double_map_key(prefix, cid_number, role_code)
    }

    fn fixed_cid_prefix(storage_prefix: Vec<u8>, cid_number: &[u8]) -> Vec<u8> {
        let mut prefix = storage_prefix;
        prefix.extend_from_slice(&blake2_128_concat(&cid_number.to_vec().encode()));
        prefix
    }

    /// 启动时只枚举固定机构的岗位/任职子树，不扫描全部公权机构。
    pub fn fixed_catalog_prefixes() -> Vec<Vec<u8>> {
        let mut prefixes = Vec::new();
        for institution in protected_institutions() {
            let cid = institution.cid_number.as_bytes();
            let (roles, assignments, permissions) = if is_private_protected_cid(cid) {
                (
                    private_institution_roles_prefix(),
                    private_institution_role_assignments_prefix(),
                    private_institution_role_permissions_prefix(),
                )
            } else {
                (
                    institution_roles_prefix(),
                    institution_role_assignments_prefix(),
                    institution_role_permissions_prefix(),
                )
            };
            prefixes.push(fixed_cid_prefix(roles, cid));
            prefixes.push(fixed_cid_prefix(assignments, cid));
            prefixes.push(fixed_cid_prefix(permissions, cid));
            if is_private_protected_cid(cid) {
                prefixes.push(private_institution(cid));
            }
        }
        prefixes
    }

    /// 返回 RAW key 对应的受保护创世机构；普通机构治理 key 明确返回 `None`。
    pub fn protected_institution_for_key(key: &[u8]) -> Option<FixedInstitution> {
        if key
            == private_institution(
                primitives::cid::china::citizenchain::CITIZENCHAIN_FOUNDATION
                    .cid_number
                    .as_bytes(),
            )
        {
            return protected_institutions()
                .into_iter()
                .find(|institution| is_private_protected_cid(institution.cid_number.as_bytes()));
        }
        for prefix in [admin_accounts_prefix(), private_admin_accounts_prefix()] {
            if let Some(cid_number) = admin_cid_from_key(key, &prefix) {
                return protected_institutions()
                    .into_iter()
                    .find(|institution| institution.cid_number.as_bytes() == cid_number);
            }
        }
        let prefixes = [
            institution_roles_prefix(),
            institution_role_assignments_prefix(),
            institution_role_permissions_prefix(),
            private_institution_roles_prefix(),
            private_institution_role_assignments_prefix(),
            private_institution_role_permissions_prefix(),
        ];
        let parsed = prefixes
            .iter()
            .find(|prefix| key.starts_with(prefix.as_slice()))
            .map(|prefix| crate::guard::governance_skeleton::parse_double_map_key(key, prefix))?;
        let (cid_number, _) = parsed.ok()?;
        protected_institutions()
            .into_iter()
            .find(|institution| institution.cid_number.as_bytes() == cid_number)
    }

    /// 普通区块和完整状态分区只关注 90 个受保护机构，不收集一般机构治理状态。
    pub fn is_relevant(key: &[u8]) -> bool {
        protected_institution_for_key(key).is_some()
    }
}
#[cfg(test)]
#[path = "../tests/governance.rs"]
mod tests;
