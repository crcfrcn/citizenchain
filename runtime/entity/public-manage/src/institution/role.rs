//! 公权机构岗位目录与管理员任职。
//!
//! `admins` 模组独立保存 `account_id + family_name + given_name` 人员集合；本模块只保存
//! “管理员在本机构担任什么岗位”的事实。运行期首次登记仅建立空缺的法定代表人岗位，
//! 不根据岗位或任职生成管理员。

extern crate alloc;

use admin_primitives::InstitutionAdminQuery as _;
use alloc::vec::Vec;
use entity_primitives::{
    BusinessActionId, InstitutionAdminAssignment, InstitutionAssignmentSource,
    InstitutionAssignmentStatus, InstitutionCapabilityPolicy as _, InstitutionRole,
    InstitutionRoleAuthorizationQuery, InstitutionRoleQuery, InstitutionRoleStatus,
    RoleBusinessPermission, RolePermissionOperation, RoleSubject, ASSIGNMENT_SOURCE_REF_MAX_BYTES,
    BUSINESS_MODULE_TAG_MAX_BYTES, INSTITUTION_ROLE_CODE_MAX_BYTES, MAX_ROLE_PERMISSIONS_PER_ROLE,
};
use frame_support::{
    dispatch::DispatchResult,
    ensure,
    traits::{ConstU32, UnixTime},
    BoundedVec,
};
use sp_std::collections::btree_set::BTreeSet;

use crate::pallet::{
    AccountNameOf, CidNumberOf, Config, Error, InstitutionRoleAssignments,
    InstitutionRolePermissions, InstitutionRoles, Institutions, Pallet, UsedRoleCodes,
};

pub type RoleCodeOf = BoundedVec<u8, ConstU32<INSTITUTION_ROLE_CODE_MAX_BYTES>>;
pub type AssignmentSourceRefOf = BoundedVec<u8, ConstU32<ASSIGNMENT_SOURCE_REF_MAX_BYTES>>;
pub type ModuleTagOf = BoundedVec<u8, ConstU32<BUSINESS_MODULE_TAG_MAX_BYTES>>;
pub type InstitutionRoleOf<T> = InstitutionRole<CidNumberOf<T>, RoleCodeOf, AccountNameOf<T>>;
pub type InstitutionRolesOf<T> = BoundedVec<InstitutionRoleOf<T>, <T as Config>::MaxAdmins>;
pub type InstitutionAdminAssignmentOf<T> = InstitutionAdminAssignment<
    CidNumberOf<T>,
    <T as frame_system::Config>::AccountId,
    RoleCodeOf,
    AssignmentSourceRefOf,
>;
pub type InstitutionAdminAssignmentsOf<T> =
    BoundedVec<InstitutionAdminAssignmentOf<T>, <T as Config>::MaxAdmins>;
pub type RoleAssignmentsOf<T> =
    BoundedVec<InstitutionAdminAssignmentOf<T>, <T as Config>::MaxAdmins>;
pub type RolePermissionOf<T> = RoleBusinessPermission<CidNumberOf<T>, RoleCodeOf, ModuleTagOf>;
pub type RolePermissionsOf<T> =
    BoundedVec<RolePermissionOf<T>, ConstU32<MAX_ROLE_PERMISSIONS_PER_ROLE>>;

const MAX_ROLE_CODE_GENERATION_ATTEMPTS: u32 = 16;

impl<T: Config> Pallet<T> {
    /// 为已通过提案分配永不复用的动态岗位码；调用方没有提交岗位码的入口。
    pub(crate) fn allocate_dynamic_role_code(
        cid_number: &CidNumberOf<T>,
        current_nonce: u64,
        proposal_id: u64,
    ) -> Result<(RoleCodeOf, u64), Error<T>> {
        let mut nonce = current_nonce;
        for _ in 0..MAX_ROLE_CODE_GENERATION_ATTEMPTS {
            let raw = entity_primitives::generate_dynamic_role_code(
                crate::MODULE_TAG,
                cid_number.as_slice(),
                nonce,
                proposal_id,
            );
            nonce = nonce.checked_add(1).ok_or(Error::<T>::RoleNonceOverflow)?;
            let role_code: RoleCodeOf = raw.try_into().map_err(|_| Error::<T>::InvalidRoleCode)?;
            if !UsedRoleCodes::<T>::get(cid_number, &role_code)
                && !InstitutionRoles::<T>::contains_key(cid_number, &role_code)
            {
                return Ok((role_code, nonce));
            }
        }
        Err(Error::<T>::RoleCodeGenerationExhausted)
    }

    fn is_assignment_effective(
        role: &InstitutionRoleOf<T>,
        assignment: &InstitutionAdminAssignmentOf<T>,
    ) -> bool {
        if assignment.assignment_status != InstitutionAssignmentStatus::Active {
            return false;
        }
        if !role.term_required {
            return assignment.term_start == 0 && assignment.term_end == 0;
        }
        let current_day = <T::TimeProvider as UnixTime>::now().as_secs() / 86_400;
        let Ok(current_day) = u32::try_from(current_day) else {
            return false;
        };
        assignment.term_start > 0
            && assignment.term_start <= current_day
            && current_day <= assignment.term_end
    }

    /// 校验并写入一组岗位、任职。
    ///
    /// 注册创建只接受 `Registry`，创世构建只接受 `Genesis`；投票中的中间状态
    /// 不得写入 entity，投票引擎只能在最终结果确定后调用任职结果入口。
    pub fn store_roles_and_assignments(
        cid_number: &CidNumberOf<T>,
        roles: &InstitutionRolesOf<T>,
        assignments: &InstitutionAdminAssignmentsOf<T>,
        expected_source: InstitutionAssignmentSource,
    ) -> DispatchResult {
        ensure!(!roles.is_empty(), Error::<T>::InstitutionRolesEmpty);

        let mut role_codes = BTreeSet::new();
        let mut role_names = BTreeSet::new();
        for role in roles.iter() {
            ensure!(role.cid_number == *cid_number, Error::<T>::RoleCidMismatch);
            ensure!(!role.role_code.is_empty(), Error::<T>::InvalidRoleCode);
            ensure!(!role.role_name.is_empty(), Error::<T>::InvalidRoleName);
            ensure!(
                !InstitutionRoles::<T>::contains_key(cid_number, &role.role_code),
                Error::<T>::DuplicateRoleCode
            );
            ensure!(
                !InstitutionRoles::<T>::iter_prefix(cid_number)
                    .any(|(_, existing)| existing.role_name == role.role_name),
                Error::<T>::DuplicateRoleName
            );
            ensure!(
                role.role_status == InstitutionRoleStatus::Active,
                Error::<T>::InitialRoleMustBeActive
            );
            ensure!(
                role_codes.insert(role.role_code.clone()),
                Error::<T>::DuplicateRoleCode
            );
            ensure!(
                role_names.insert(role.role_name.clone()),
                Error::<T>::DuplicateRoleName
            );
        }

        let mut assignment_keys = BTreeSet::new();
        for assignment in assignments.iter() {
            ensure!(
                assignment.cid_number == *cid_number,
                Error::<T>::AssignmentCidMismatch
            );
            ensure!(
                assignment.assignment_source == expected_source,
                Error::<T>::InvalidAssignmentSource
            );
            ensure!(
                assignment.assignment_status == InstitutionAssignmentStatus::Active,
                Error::<T>::InitialAssignmentMustBeActive
            );
            let role = roles
                .iter()
                .find(|role| role.role_code == assignment.role_code)
                .ok_or(Error::<T>::AssignmentRoleNotFound)?;
            if role.term_required {
                ensure!(
                    assignment.term_start > 0 && assignment.term_end >= assignment.term_start,
                    Error::<T>::InvalidAssignmentTerm
                );
            } else {
                ensure!(
                    assignment.term_start == 0 && assignment.term_end == 0,
                    Error::<T>::UnexpectedAssignmentTerm
                );
            }
            ensure!(
                assignment_keys
                    .insert((assignment.account_id.clone(), assignment.role_code.clone(),)),
                Error::<T>::DuplicateAssignment
            );
        }

        for role in roles.iter() {
            let role_assignments: Vec<InstitutionAdminAssignmentOf<T>> = assignments
                .iter()
                .filter(|assignment| assignment.role_code == role.role_code)
                .cloned()
                .collect();
            if primitives::institution_constraints::is_legal_representative_role(
                role.role_code.as_slice(),
            ) {
                ensure!(
                    role_assignments.len() <= 1,
                    Error::<T>::FixedRoleSeatsMismatch
                );
            }
            let bounded_assignments: RoleAssignmentsOf<T> = role_assignments
                .try_into()
                .map_err(|_| Error::<T>::TooManyInstitutionAdmins)?;
            InstitutionRoles::<T>::insert(cid_number, &role.role_code, role.clone());
            InstitutionRoleAssignments::<T>::insert(
                cid_number,
                &role.role_code,
                bounded_assignments,
            );
        }

        Ok(())
    }

    /// 为新机构写入唯一默认法定代表人岗位；首次登记允许岗位空缺。
    pub fn store_default_legal_representative_role(cid_number: &CidNumberOf<T>) -> DispatchResult {
        Self::store_vacant_genesis_role(
            cid_number,
            primitives::institution_constraints::ROLE_CODE_LEGAL_REPRESENTATIVE,
            primitives::institution_constraints::ROLE_NAME_LEGAL_REPRESENTATIVE,
        )
    }

    /// 为联邦安全局写入创世「局长」岗位；与该机构自带的 LR 并存。
    pub fn store_genesis_director_role(cid_number: &CidNumberOf<T>) -> DispatchResult {
        Self::store_vacant_genesis_role(
            cid_number,
            primitives::institution_constraints::ROLE_CODE_DIRECTOR,
            primitives::institution_constraints::ROLE_NAME_DIRECTOR,
        )
    }

    /// 写入一个**空缺**的创世固定岗位。
    ///
    /// `term_required = false`：创世期一律不设任期，任期规则由运行期业务模块逐个规范。
    /// 幂等：岗位已存在且形状一致时只补权限，不改写。
    fn store_vacant_genesis_role(
        cid_number: &CidNumberOf<T>,
        role_code_bytes: &[u8],
        role_name_bytes: &[u8],
    ) -> DispatchResult {
        let role_code: RoleCodeOf = role_code_bytes
            .to_vec()
            .try_into()
            .map_err(|_| Error::<T>::InvalidRoleCode)?;
        let role_name: AccountNameOf<T> = role_name_bytes
            .to_vec()
            .try_into()
            .map_err(|_| Error::<T>::InvalidRoleName)?;
        ensure!(
            !InstitutionRoles::<T>::iter_prefix(cid_number).any(|(existing_code, existing)| {
                existing_code != role_code && existing.role_name == role_name
            }),
            Error::<T>::DuplicateRoleName
        );
        if let Some(existing) = InstitutionRoles::<T>::get(cid_number, &role_code) {
            ensure!(
                existing.cid_number == *cid_number
                    && existing.role_code == role_code
                    && existing.role_name == role_name
                    && !existing.term_required
                    && existing.role_status == InstitutionRoleStatus::Active
                    && InstitutionRoleAssignments::<T>::get(cid_number, &role_code).len() <= 1,
                Error::<T>::DuplicateRoleCode
            );
            Self::store_role_permissions_from_fixed_directory(cid_number, &role_code)?;
            return Ok(());
        }
        InstitutionRoles::<T>::insert(
            cid_number,
            &role_code,
            InstitutionRole {
                cid_number: cid_number.clone(),
                role_code: role_code.clone(),
                role_name,
                term_required: false,
                role_status: InstitutionRoleStatus::Active,
            },
        );
        InstitutionRoleAssignments::<T>::insert(
            cid_number,
            &role_code,
            RoleAssignmentsOf::<T>::default(),
        );
        // 权限以准确 CID + 岗位码入库：承担法律签署职责的机构（LR）写入 leg-yuan Vote，
        // 固定目录中无该岗位规格的一律保持空权限。
        Self::store_role_permissions_from_fixed_directory(cid_number, &role_code)?;
        Ok(())
    }

    /// 把共享固定权限目录转换成当前 entity 的有界存储结构。
    fn store_role_permissions_from_fixed_directory(
        cid_number: &CidNumberOf<T>,
        role_code: &RoleCodeOf,
    ) -> DispatchResult {
        let institution =
            Institutions::<T>::get(cid_number).ok_or(Error::<T>::InstitutionNotFound)?;
        let mut permissions = Vec::new();
        for spec in entity_primitives::fixed_role_permission_specs(
            institution.institution_code,
            cid_number.as_slice(),
            role_code.as_slice(),
        ) {
            let module_tag: ModuleTagOf = spec
                .module_tag
                .to_vec()
                .try_into()
                .map_err(|_| Error::<T>::InvalidRolePermission)?;
            let business_action_id = BusinessActionId {
                module_tag,
                action_code: spec.action_code,
            };
            ensure!(
                T::InstitutionCapabilityPolicy::allows(
                    cid_number.as_slice(),
                    &BusinessActionId {
                        module_tag: business_action_id.module_tag.clone().into_inner(),
                        action_code: business_action_id.action_code,
                    },
                    spec.operation,
                ),
                Error::<T>::InstitutionCapabilityDenied
            );
            permissions.push(RoleBusinessPermission {
                role_subject: RoleSubject {
                    cid_number: cid_number.clone(),
                    role_code: role_code.clone(),
                },
                business_action_id,
                operation: spec.operation,
            });
        }
        let permissions: RolePermissionsOf<T> = permissions
            .try_into()
            .map_err(|_| Error::<T>::TooManyRolePermissions)?;
        InstitutionRolePermissions::<T>::insert(cid_number, role_code, permissions);
        UsedRoleCodes::<T>::insert(cid_number, role_code, true);
        Ok(())
    }

    /// 创世专用入口：来源必须为 `Genesis`，失败由创世构建直接中止。
    pub fn store_genesis_roles_and_assignments(
        cid_number: &CidNumberOf<T>,
        roles: &InstitutionRolesOf<T>,
        assignments: &InstitutionAdminAssignmentsOf<T>,
    ) -> DispatchResult {
        Self::store_roles_and_assignments(
            cid_number,
            roles,
            assignments,
            InstitutionAssignmentSource::Genesis,
        )
    }

    /// 创世专用入口：按共享固定目录写入受保护岗位权限，包含永久空权限的 LR。
    ///
    /// 权限主体必须是准确的创世 CID + 固定岗位码；任一动作超出该 CID 顶层能力时，
    /// 创世构建立即失败，禁止写入半套权限或按机构码扩大授权。
    pub fn store_genesis_fixed_role_permissions(
        cid_number: &CidNumberOf<T>,
        role_code: &RoleCodeOf,
    ) -> DispatchResult {
        let institution =
            Institutions::<T>::get(cid_number).ok_or(Error::<T>::InstitutionNotFound)?;
        ensure!(
            primitives::governance_skeleton::fixed_role_seats_by_identity(
                institution.institution_code,
                cid_number.as_slice(),
                role_code.as_slice(),
            )
            .is_some(),
            Error::<T>::FixedRoleDefinitionImmutable
        );
        ensure!(
            InstitutionRoles::<T>::contains_key(cid_number, role_code),
            Error::<T>::AssignmentRoleNotFound
        );

        Self::store_role_permissions_from_fixed_directory(cid_number, role_code)
    }
}

impl<T: Config> InstitutionRoleQuery<T::AccountId> for Pallet<T> {
    fn is_active_assignment(cid_number: &[u8], admin: &T::AccountId, role_code: &[u8]) -> bool {
        let Ok(cid_number) = CidNumberOf::<T>::try_from(cid_number.to_vec()) else {
            return false;
        };
        let Ok(role_code) = RoleCodeOf::try_from(role_code.to_vec()) else {
            return false;
        };
        let Some(institution) = Institutions::<T>::get(&cid_number) else {
            return false;
        };
        // 把调用者钱包解析为名册规范账户（运行期按 CID 绑定、否则 account_id）；None = 非该机构管理员。
        let Some(canonical) = T::InstitutionAdminQuery::resolve_admin_account(
            institution.institution_code,
            cid_number.as_slice(),
            admin,
        ) else {
            return false;
        };
        let Some(role) = InstitutionRoles::<T>::get(&cid_number, &role_code) else {
            return false;
        };
        if role.role_status != InstitutionRoleStatus::Active {
            return false;
        }
        InstitutionRoleAssignments::<T>::get(&cid_number, role_code)
            .into_iter()
            .any(|assignment| {
                assignment.account_id == canonical
                    && Pallet::<T>::is_assignment_effective(&role, &assignment)
            })
    }

    fn active_accounts_for_role(cid_number: &[u8], role_code: &[u8]) -> Vec<T::AccountId> {
        let Ok(cid_number) = CidNumberOf::<T>::try_from(cid_number.to_vec()) else {
            return Vec::new();
        };
        let Ok(role_code) = RoleCodeOf::try_from(role_code.to_vec()) else {
            return Vec::new();
        };
        let Some(institution) = Institutions::<T>::get(&cid_number) else {
            return Vec::new();
        };
        let Some(role) = InstitutionRoles::<T>::get(&cid_number, &role_code) else {
            return Vec::new();
        };
        if role.role_status != InstitutionRoleStatus::Active {
            return Vec::new();
        }
        InstitutionRoleAssignments::<T>::get(&cid_number, role_code)
            .into_iter()
            .filter(|assignment| {
                T::InstitutionAdminQuery::is_institution_admin(
                    institution.institution_code,
                    cid_number.as_slice(),
                    &assignment.account_id,
                ) && Pallet::<T>::is_assignment_effective(&role, assignment)
            })
            .map(|assignment| assignment.account_id)
            .collect()
    }

    fn active_role_codes(cid_number: &[u8], admin: &T::AccountId) -> Vec<Vec<u8>> {
        let Ok(cid_number) = CidNumberOf::<T>::try_from(cid_number.to_vec()) else {
            return Vec::new();
        };
        let Some(institution) = Institutions::<T>::get(&cid_number) else {
            return Vec::new();
        };
        InstitutionRoles::<T>::iter_prefix(&cid_number)
            .filter(|(_, role)| role.role_status == InstitutionRoleStatus::Active)
            .filter_map(|(role_code, role)| {
                let active = InstitutionRoleAssignments::<T>::get(&cid_number, &role_code)
                    .into_iter()
                    .any(|assignment| {
                        &assignment.account_id == admin
                            && T::InstitutionAdminQuery::is_institution_admin(
                                institution.institution_code,
                                cid_number.as_slice(),
                                admin,
                            )
                            && Pallet::<T>::is_assignment_effective(&role, &assignment)
                    });
                active.then(|| role_code.into_inner())
            })
            .collect()
    }
}

impl<T: Config> InstitutionRoleAuthorizationQuery<T::AccountId> for Pallet<T> {
    fn role_has_permission(
        role_subject: &RoleSubject<Vec<u8>, Vec<u8>>,
        business_action_id: &BusinessActionId<Vec<u8>>,
        operation: RolePermissionOperation,
    ) -> bool {
        let Ok(cid_number) = CidNumberOf::<T>::try_from(role_subject.cid_number.clone()) else {
            return false;
        };
        let Ok(role_code) = RoleCodeOf::try_from(role_subject.role_code.clone()) else {
            return false;
        };
        if !InstitutionRoles::<T>::contains_key(&cid_number, &role_code)
            || !T::InstitutionCapabilityPolicy::allows(
                cid_number.as_slice(),
                business_action_id,
                operation,
            )
        {
            return false;
        }
        InstitutionRolePermissions::<T>::get(&cid_number, &role_code)
            .into_iter()
            .any(|permission| {
                permission.role_subject.cid_number.as_slice() == cid_number.as_slice()
                    && permission.role_subject.role_code.as_slice() == role_code.as_slice()
                    && permission.business_action_id.module_tag.as_slice()
                        == business_action_id.module_tag.as_slice()
                    && permission.business_action_id.action_code == business_action_id.action_code
                    && permission.operation == operation
            })
    }

    fn is_authorized(
        admin: &T::AccountId,
        role_subject: &RoleSubject<Vec<u8>, Vec<u8>>,
        business_action_id: &BusinessActionId<Vec<u8>>,
        operation: RolePermissionOperation,
    ) -> bool {
        let Ok(cid_number) = CidNumberOf::<T>::try_from(role_subject.cid_number.clone()) else {
            return false;
        };
        // 名册成员校验已并入 is_active_assignment 内的 resolve_admin_account，不再单独调 is_institution_admin。
        <Self as InstitutionRoleQuery<T::AccountId>>::is_active_assignment(
            cid_number.as_slice(),
            admin,
            role_subject.role_code.as_slice(),
        ) && Self::role_has_permission(role_subject, business_action_id, operation)
    }

    fn role_subjects_with_permission(
        cid_number: &[u8],
        business_action_id: &BusinessActionId<Vec<u8>>,
        operation: RolePermissionOperation,
    ) -> Vec<RoleSubject<Vec<u8>, Vec<u8>>> {
        let Ok(cid_number) = CidNumberOf::<T>::try_from(cid_number.to_vec()) else {
            return Vec::new();
        };
        if !Institutions::<T>::contains_key(&cid_number)
            || !T::InstitutionCapabilityPolicy::allows(
                cid_number.as_slice(),
                business_action_id,
                operation,
            )
        {
            return Vec::new();
        }

        let mut subjects = InstitutionRolePermissions::<T>::iter_prefix(&cid_number)
            .filter_map(|(role_code, permissions)| {
                if !InstitutionRoles::<T>::contains_key(&cid_number, &role_code)
                    || !permissions.iter().any(|permission| {
                        permission.role_subject.cid_number.as_slice() == cid_number.as_slice()
                            && permission.role_subject.role_code.as_slice() == role_code.as_slice()
                            && permission.business_action_id.module_tag.as_slice()
                                == business_action_id.module_tag.as_slice()
                            && permission.business_action_id.action_code
                                == business_action_id.action_code
                            && permission.operation == operation
                    })
                {
                    return None;
                }
                Some(RoleSubject {
                    cid_number: cid_number.to_vec(),
                    role_code: role_code.to_vec(),
                })
            })
            .collect::<Vec<_>>();
        subjects.sort_by(|left, right| left.role_code.cmp(&right.role_code));
        subjects.dedup_by(|left, right| left.role_code == right.role_code);
        subjects
    }
}
