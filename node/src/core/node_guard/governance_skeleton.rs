//! 固定治理机构的管理员、岗位与任职节点策略（档 A）。
//!
//! `PublicAdmins::AdminAccounts` 保存公权管理员账户及可逐步补齐的公民资料，`PublicManage`
//! 保存岗位与任职。本策略按共享编译期清单校验 89 个公权固定治理机构和公民链基金会：
//! 固定岗位目录、席位与权限不允许漂移，具体管理员允许依法原子轮换。任职来源、引用和任期
//! 属于 runtime 业务合法性，不在原生组织结构守卫中重复解释。
//! 既有公权机构的法定代表人不是创世必填项；基金会明确具有法定代表人，守卫只额外
//! 校验其机构信息账户与固定 `LR` 任职一致，不复制公民资料真源。

use std::collections::{BTreeMap, BTreeSet};

use admin_primitives::{Admin, InstitutionAdmins};
use codec::Decode;
#[cfg(test)]
use codec::Encode;
#[cfg(test)]
use entity_primitives::InstitutionAssignmentSource;
use entity_primitives::{
    BusinessActionId, InstitutionAdminAssignment as SharedInstitutionAdminAssignment,
    InstitutionAssignmentStatus, InstitutionRole as SharedInstitutionRole, InstitutionRoleStatus,
    RoleBusinessPermission, RoleSubject,
};

use primitives::{
    cid::china::citizenchain::{
        CITIZENCHAIN_FIXED_ROLES, CITIZENCHAIN_FOUNDATION, CITIZENCHAIN_GENESIS_ADMINS,
        LEGAL_REPRESENTATIVE_CITIZEN_CID_NUMBER,
    },
    cid::code::{FRG, PROVINCE_CODE_INFOS},
    count_const::FRG_PROVINCE_GROUP_ADMIN_COUNT,
    governance_skeleton::{
        fixed_institutions, fixed_role_specs, province_commissioner_role_code,
        province_commissioner_role_name, FixedInstitution,
    },
};

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

/// 三张受保护 storage 的完整 RAW key 和精确前缀。
pub mod storage_key {
    use super::{
        is_private_protected_cid, protected_institutions, FixedInstitution,
        CITIZEN_IDENTITY_PALLET, PRIVATE_ADMINS_PALLET, PRIVATE_MANAGE_PALLET,
        PUBLIC_ADMINS_PALLET, PUBLIC_MANAGE_PALLET,
    };
    use codec::{Decode, Encode};
    use sp_crypto_hashing::blake2_128;

    fn storage_prefix(pallet: &[u8], storage: &[u8]) -> Vec<u8> {
        crate::shared::storage_keys::prefix(pallet, storage)
    }

    fn blake2_128_concat(encoded: &[u8]) -> Vec<u8> {
        crate::shared::storage_keys::blake2_128_concat(encoded)
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
        if key == private_institution(super::CITIZENCHAIN_FOUNDATION.cid_number.as_bytes()) {
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
            .map(|prefix| super::parse_double_map_key(key, prefix))?;
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

/// 节点直接使用共享协议类型解码，避免维护第二份字段顺序和枚举判别值。
type DecodedPrivateInstitutionAdmins = InstitutionAdmins<Vec<Admin<[u8; 32]>>>;
type DecodedPublicInstitutionAdmins = InstitutionAdmins<Vec<Admin<[u8; 32]>>>;
type DecodedInstitutionRole = SharedInstitutionRole<Vec<u8>, Vec<u8>, Vec<u8>>;
type DecodedInstitutionAdminAssignment =
    SharedInstitutionAdminAssignment<Vec<u8>, [u8; 32], Vec<u8>, Vec<u8>>;
type DecodedRoleBusinessPermission = RoleBusinessPermission<Vec<u8>, Vec<u8>, Vec<u8>>;
type DecodedInstitutionInfo = entity_primitives::InstitutionInfo<u32, Vec<u8>, Vec<u8>, [u8; 32]>;

/// 固定治理骨架校验失败原因。
#[derive(Debug, PartialEq)]
pub enum GuardError {
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
    /// 基金会名册账户与该公民 CID 在 citizen-identity 中当前绑定的钱包不一致。
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
pub fn check_catalog_keys<I, K>(keys: I) -> Result<(), GuardError>
where
    I: IntoIterator<Item = K>,
    K: AsRef<[u8]>,
{
    let prefixes = [
        storage_key::institution_roles_prefix(),
        storage_key::private_institution_roles_prefix(),
        storage_key::institution_role_assignments_prefix(),
        storage_key::private_institution_role_assignments_prefix(),
        storage_key::institution_role_permissions_prefix(),
        storage_key::private_institution_role_permissions_prefix(),
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
pub fn check_skeleton_invariants<F>(read_raw: F) -> Result<(), GuardError>
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
        let raw = read_raw(&storage_key::private_institution(expected_cid))
            .ok_or(GuardError::LegalRepresentativeInfoMissing(institution.code))?;
        let info: DecodedInstitutionInfo = decode_exact(&raw)
            .map_err(|_| GuardError::LegalRepresentativeInfoMissing(institution.code))?;
        let legal_representative_fields_valid =
            info.legal_representative
                .as_ref()
                .is_none_or(|representative| {
                    !representative.family_name.is_empty()
                        && !representative.given_name.is_empty()
                        && !representative.cid_number.is_empty()
                });
        if info.institution_code != institution.code || !legal_representative_fields_valid {
            return Err(GuardError::LegalRepresentativeInfoMissing(institution.code));
        }
        if info.cid_full_name != CITIZENCHAIN_FOUNDATION.cid_full_name.as_bytes() {
            return Err(GuardError::InstitutionFullNameChanged(institution.code));
        }
        Some(info)
    } else {
        None
    };
    let raw = read_raw(&storage_key::account_id(expected_cid))
        .ok_or(GuardError::FixedInstitutionMissing(institution.code))?;
    let (stored_code, admin_account_ids, invalid_private_name) =
        if is_private_protected_cid(expected_cid) {
            let account: DecodedPrivateInstitutionAdmins = decode_exact(&raw)
                .map_err(|_| GuardError::AdminAccountDecodeFailed(institution.code))?;
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
                return Err(GuardError::FoundationGenesisIdentityChanged(
                    institution.code,
                ));
            }
            // 换钱包必须同步换绑：名册账户须等于该公民 CID 当前绑定的签名账户。
            // 该绑定由 citizen-identity 在运行期登记；创世不播种，故「无绑定」放行，
            // 只有已登记却指向别的账户时才判定篡改（无条件校验会否掉创世状态本身）。
            if let Some(bound_raw) = read_raw(&storage_key::citizen_account_id_by_cid(expected_cid))
            {
                let bound: [u8; 32] = decode_exact(&bound_raw)
                    .map_err(|_| GuardError::AdminAccountDecodeFailed(institution.code))?;
                if account.admins.iter().any(|admin| admin.account_id != bound) {
                    return Err(GuardError::FoundationWalletBindingMismatch(
                        institution.code,
                    ));
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
            let account: DecodedPublicInstitutionAdmins = decode_exact(&raw)
                .map_err(|_| GuardError::AdminAccountDecodeFailed(institution.code))?;
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
        return Err(GuardError::InstitutionCodeChanged(institution.code));
    }
    let found = admin_account_ids.len() as u32;
    if found != institution.expected_len {
        return Err(GuardError::AdminsLenChanged {
            code: institution.code,
            expected: institution.expected_len,
            found,
        });
    }
    if invalid_private_name {
        return Err(GuardError::InvalidAdminPersonName(institution.code));
    }
    let admin_set = admin_account_ids.iter().copied().collect::<BTreeSet<_>>();
    if admin_set.len() != admin_account_ids.len() {
        return Err(GuardError::DuplicateAdminAccountId(institution.code));
    }

    let mut legal_representative_assignment = None;
    for expected_role in expected_roles(institution) {
        let role_key = storage_key::institution_role(expected_cid, &expected_role.role_code);
        let role_raw = read_raw(&role_key).ok_or_else(|| GuardError::RoleMissing {
            code: institution.code,
            role_code: expected_role.role_code.clone(),
        })?;
        let role: DecodedInstitutionRole =
            decode_exact(&role_raw).map_err(|_| GuardError::RoleDecodeFailed {
                code: institution.code,
                role_code: expected_role.role_code.clone(),
            })?;
        if role.cid_number != expected_cid {
            return Err(GuardError::RoleCidChanged {
                code: institution.code,
                role_code: expected_role.role_code,
            });
        }
        if role.role_code != expected_role.role_code {
            return Err(GuardError::RoleCodeChanged {
                code: institution.code,
                role_code: expected_role.role_code,
            });
        }
        if role.role_name != expected_role.role_name {
            return Err(GuardError::RoleNameChanged {
                code: institution.code,
                role_code: expected_role.role_code,
            });
        }
        if role.role_status != InstitutionRoleStatus::Active {
            return Err(GuardError::RoleNotActive {
                code: institution.code,
                role_code: expected_role.role_code,
            });
        }

        let permissions_key =
            storage_key::institution_role_permissions(expected_cid, &expected_role.role_code);
        let permissions_raw =
            read_raw(&permissions_key).ok_or_else(|| GuardError::PermissionsMissing {
                code: institution.code,
                role_code: expected_role.role_code.clone(),
            })?;
        let permissions: Vec<DecodedRoleBusinessPermission> = decode_exact(&permissions_raw)
            .map_err(|_| GuardError::PermissionsDecodeFailed {
                code: institution.code,
                role_code: expected_role.role_code.clone(),
            })?;
        let expected_permissions = entity_primitives::fixed_role_permission_specs(
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
            return Err(GuardError::PermissionsChanged {
                code: institution.code,
                role_code: expected_role.role_code.clone(),
            });
        }

        let assignments_key =
            storage_key::institution_role_assignments(expected_cid, &expected_role.role_code);
        let assignments_raw =
            read_raw(&assignments_key).ok_or_else(|| GuardError::AssignmentsMissing {
                code: institution.code,
                role_code: expected_role.role_code.clone(),
            })?;
        let assignments: Vec<DecodedInstitutionAdminAssignment> = decode_exact(&assignments_raw)
            .map_err(|_| GuardError::AssignmentsDecodeFailed {
                code: institution.code,
                role_code: expected_role.role_code.clone(),
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
            .ok_or_else(|| GuardError::RoleMissing {
                code: institution.code,
                role_code: expected_role.role_code.clone(),
            })?
        };
        if found < min_assignments || found > max_assignments {
            return Err(GuardError::SeatsChanged {
                code: institution.code,
                role_code: expected_role.role_code,
                expected: max_assignments,
                found,
            });
        }
        let mut role_assignment_account_ids = BTreeSet::new();
        for assignment in assignments {
            if assignment.cid_number != expected_cid {
                return Err(GuardError::AssignmentCidChanged {
                    code: institution.code,
                    role_code: expected_role.role_code,
                });
            }
            if assignment.role_code != expected_role.role_code {
                return Err(GuardError::AssignmentRoleChanged {
                    code: institution.code,
                    role_code: expected_role.role_code,
                });
            }
            if assignment.assignment_status != InstitutionAssignmentStatus::Active {
                return Err(GuardError::AssignmentNotActive {
                    code: institution.code,
                    role_code: expected_role.role_code,
                });
            }
            if !admin_set.contains(&assignment.account_id) {
                return Err(GuardError::AdminAssignmentSetMismatch(institution.code));
            }
            if !role_assignment_account_ids.insert(assignment.account_id) {
                return Err(GuardError::DuplicateAssignmentAccountId(institution.code));
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
            return Err(GuardError::LegalRepresentativeAssignmentMismatch(
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
        if let Some(institution) = storage_key::protected_institution_for_key(key) {
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
    delta.keys().any(|key| storage_key::is_relevant(key))
        || delta.contains_key(sp_storage::well_known_keys::CODE)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::*;
    use sp_crypto_hashing::{blake2_128, twox_128};

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
            legal_representative: Some(entity_primitives::LegalRepresentative {
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
        entity_primitives::fixed_role_permission_specs(
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
                storage_key::account_id(institution.cid_number.as_bytes()),
                account_bytes(&institution, admins.clone()),
            );
            if is_private_protected_cid(institution.cid_number.as_bytes()) {
                state.insert(
                    storage_key::private_institution(institution.cid_number.as_bytes()),
                    private_info_bytes(&institution, admins[0]),
                );
            }
            let mut offset = 0usize;
            for role in expected_roles(&institution) {
                state.insert(
                    storage_key::institution_role(
                        institution.cid_number.as_bytes(),
                        &role.role_code,
                    ),
                    role_bytes(&institution, &role),
                );
                state.insert(
                    storage_key::institution_role_permissions(
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
                    storage_key::institution_role_assignments(
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
    /// 公民 CID / 姓 / 名永久冻结；换钱包必须同步换绑 citizen-identity。
    #[test]
    fn foundation_genesis_identity_is_frozen_and_only_wallet_may_change() {
        let foundation = protected_institutions()
            .into_iter()
            .find(|institution| is_private_protected_cid(institution.cid_number.as_bytes()))
            .expect("protected private genesis foundation");
        let admin_key = storage_key::account_id(foundation.cid_number.as_bytes());
        let (expected_cid, _, _) = expected_foundation_identity();

        let decode_admins =
            |state: &BTreeMap<Vec<u8>, Vec<u8>>| -> DecodedPrivateInstitutionAdmins {
                decode_exact(state.get(&admin_key).expect("foundation admins exist"))
                    .expect("foundation admins decode")
            };

        // 基线：创世状态必须放行（此时 citizen-identity 尚无绑定）。
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
                Err(GuardError::FoundationGenesisIdentityChanged(
                    foundation.code
                )),
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
                storage_key::institution_role_assignments(
                    foundation.cid_number.as_bytes(),
                    &role.role_code,
                ),
                vec![assignment(&foundation, &role, rotated_wallet)].encode(),
            );
        }
        // 机构信息里的法定代表人账户须一并跟换（既有守卫本就要求 LR 任职与其一致）。
        rotated.insert(
            storage_key::private_institution(foundation.cid_number.as_bytes()),
            private_info_bytes(&foundation, rotated_wallet),
        );
        assert_eq!(check_state(&rotated), Ok(()));

        // citizen-identity 已登记绑定但指向别的钱包：判 KnownBad（换钱包没同步换绑）。
        let mut mismatched = rotated.clone();
        mismatched.insert(
            storage_key::citizen_account_id_by_cid(expected_cid),
            [9u8; 32].encode(),
        );
        assert_eq!(
            check_state(&mismatched),
            Err(GuardError::FoundationWalletBindingMismatch(foundation.code))
        );

        // 绑定与名册一致：放行。
        let mut aligned = rotated;
        aligned.insert(
            storage_key::citizen_account_id_by_cid(expected_cid),
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
        assert!(storage_key::account_id(foundation.cid_number.as_bytes())
            .starts_with(&storage_key::private_admin_accounts_prefix()));
        assert!(storage_key::institution_role(
            foundation.cid_number.as_bytes(),
            primitives::institution_constraints::ROLE_CODE_LEGAL_REPRESENTATIVE,
        )
        .starts_with(&storage_key::private_institution_roles_prefix()));

        let mut missing = valid_state();
        missing.remove(&storage_key::private_institution(
            foundation.cid_number.as_bytes(),
        ));
        assert_eq!(
            check_state(&missing),
            Err(GuardError::LegalRepresentativeInfoMissing(foundation.code))
        );

        let mut mismatched = valid_state();
        mismatched.insert(
            storage_key::private_institution(foundation.cid_number.as_bytes()),
            private_info_bytes(&foundation, [250u8; 32]),
        );
        assert_eq!(
            check_state(&mismatched),
            Err(GuardError::LegalRepresentativeAssignmentMismatch(
                foundation.code
            ))
        );

        let mut renamed = valid_state();
        let mut info: DecodedInstitutionInfo = decode_exact(
            renamed
                .get(&storage_key::private_institution(
                    foundation.cid_number.as_bytes(),
                ))
                .expect("foundation institution info exists"),
        )
        .expect("foundation institution info decodes");
        info.cid_full_name = "错误公司全称".as_bytes().to_vec();
        renamed.insert(
            storage_key::private_institution(foundation.cid_number.as_bytes()),
            info.encode(),
        );
        assert_eq!(
            check_state(&renamed),
            Err(GuardError::InstitutionFullNameChanged(foundation.code))
        );

        let lr_role = expected_roles(&foundation)
            .into_iter()
            .find(|role| {
                role.role_code
                    == primitives::institution_constraints::ROLE_CODE_LEGAL_REPRESENTATIVE
            })
            .expect("foundation LR role");
        let lr_assignments_key = storage_key::institution_role_assignments(
            foundation.cid_number.as_bytes(),
            &lr_role.role_code,
        );

        // LR 岗位永久存在，但允许机构法定代表人字段与该岗位任职同时清空。
        let mut vacant = valid_state();
        let info_key = storage_key::private_institution(foundation.cid_number.as_bytes());
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
            Err(GuardError::SeatsChanged {
                code: foundation.code,
                role_code: lr_role.role_code,
                expected: 1,
                found: 2,
            })
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
        let admin_key = storage_key::account_id(institution.cid_number.as_bytes());
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
        state.remove(&storage_key::account_id(institution.cid_number.as_bytes()));
        assert_eq!(
            check_state(&state),
            Err(GuardError::FixedInstitutionMissing(institution.code))
        );

        let mut state = valid_state();
        state.insert(
            storage_key::account_id(institution.cid_number.as_bytes()),
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
            Err(GuardError::InstitutionCodeChanged(institution.code))
        );
    }

    #[test]
    fn missing_or_renamed_fixed_role_is_rejected_and_dynamic_role_is_allowed() {
        let institution = fixed_institutions()[0];
        let role = expected_roles(&institution)[0].clone();
        let role_key =
            storage_key::institution_role(institution.cid_number.as_bytes(), &role.role_code);

        let mut state = valid_state();
        state.remove(&role_key);
        assert_eq!(
            check_state(&state),
            Err(GuardError::RoleMissing {
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
            Err(GuardError::RoleNameChanged {
                code: institution.code,
                role_code: role.role_code.clone(),
            })
        );

        let mut state = valid_state();
        state.insert(
            storage_key::institution_role(institution.cid_number.as_bytes(), b"EXTRA_ROLE"),
            Vec::new(),
        );
        assert_eq!(check_state(&state), Ok(()));
    }

    #[test]
    fn missing_or_changed_fixed_role_permissions_are_rejected() {
        let institution = fixed_institutions()[0];
        let role = expected_roles(&institution)[0].clone();
        let key = storage_key::institution_role_permissions(
            institution.cid_number.as_bytes(),
            &role.role_code,
        );

        let mut missing = valid_state();
        missing.remove(&key);
        assert_eq!(
            check_state(&missing),
            Err(GuardError::PermissionsMissing {
                code: institution.code,
                role_code: role.role_code.clone(),
            })
        );

        let mut changed = valid_state();
        let mut permissions: Vec<DecodedRoleBusinessPermission> =
            decode_exact(changed.get(&key).expect("permissions exist"))
                .expect("permissions decode");
        permissions[0].business_action_id.action_code += 1;
        changed.insert(key, permissions.encode());
        assert_eq!(
            check_state(&changed),
            Err(GuardError::PermissionsChanged {
                code: institution.code,
                role_code: role.role_code,
            })
        );
    }

    #[test]
    fn changed_seat_count_or_non_admin_assignment_is_rejected() {
        let institution = fixed_institutions()[0];
        let role = expected_roles(&institution)[0].clone();
        let assignments_key = storage_key::institution_role_assignments(
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
            Err(GuardError::SeatsChanged {
                code: institution.code,
                role_code: role.role_code.clone(),
                expected: role.seats,
                found: role.seats - 1,
            })
        );

        let mut state = valid_state();
        let mut assignments: Vec<DecodedInstitutionAdminAssignment> =
            decode_exact(state.get(&assignments_key).expect("assignments exist"))
                .expect("assignments decode");
        assignments[0].account_id = [250u8; 32];
        state.insert(assignments_key, assignments.encode());
        assert_eq!(
            check_state(&state),
            Err(GuardError::AdminAssignmentSetMismatch(institution.code))
        );
    }

    #[test]
    fn member_source_and_term_fields_are_outside_native_structure_guard() {
        let institution = fixed_institutions()[0];
        let role = expected_roles(&institution)[0].clone();
        let role_key =
            storage_key::institution_role(institution.cid_number.as_bytes(), &role.role_code);
        let assignments_key = storage_key::institution_role_assignments(
            institution.cid_number.as_bytes(),
            &role.role_code,
        );
        let admin_key = storage_key::account_id(institution.cid_number.as_bytes());
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
        assert_eq!(storage_key::account_id(account), expected_admin);

        let cid = b"CID-1".to_vec().encode();
        let role = b"ROLE-1".to_vec().encode();
        let mut expected_role = twox_128(b"PublicManage").to_vec();
        expected_role.extend_from_slice(&twox_128(b"InstitutionRoles"));
        expected_role.extend_from_slice(&blake2_128(&cid));
        expected_role.extend_from_slice(&cid);
        expected_role.extend_from_slice(&blake2_128(&role));
        expected_role.extend_from_slice(&role);
        assert_eq!(
            storage_key::institution_role(b"CID-1", b"ROLE-1"),
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
            storage_key::institution_role(
                protected.cid_number.as_bytes(),
                &expected_roles(&protected)[0].role_code,
            ),
            Some(Vec::new()),
        );
        assert!(needs_full_check(&delta));
        assert!(storage_key::fixed_catalog_prefixes().len() >= 2);
    }

    #[test]
    fn ordinary_block_checks_only_the_affected_protected_institution() {
        let fixed = fixed_institutions();
        let affected = fixed[0];
        let unrelated = fixed[1];
        let mut state = valid_state();
        state.remove(&storage_key::account_id(unrelated.cid_number.as_bytes()));

        let delta = BTreeMap::from([(
            storage_key::account_id(affected.cid_number.as_bytes()),
            state
                .get(&storage_key::account_id(affected.cid_number.as_bytes()))
                .cloned(),
        )]);
        assert_eq!(
            check_affected_institutions(&delta, |key| state.get(key).cloned()),
            Ok(())
        );
        assert_eq!(
            check_skeleton_invariants(|key| state.get(key).cloned()),
            Err(GuardError::FixedInstitutionMissing(unrelated.code))
        );
    }

    #[test]
    fn malformed_extra_role_key_is_ignored_but_trailing_value_is_rejected() {
        let mut malformed = storage_key::institution_roles_prefix();
        malformed.extend_from_slice(b"bad");
        assert_eq!(check_catalog_keys([malformed]), Ok(()));

        let institution = fixed_institutions()[0];
        let role = expected_roles(&institution)[0].clone();
        let role_key =
            storage_key::institution_role(institution.cid_number.as_bytes(), &role.role_code);
        let mut state = valid_state();
        state.get_mut(&role_key).expect("role exists").push(0);
        assert_eq!(
            check_state(&state),
            Err(GuardError::RoleDecodeFailed {
                code: institution.code,
                role_code: role.role_code,
            })
        );
    }
}
