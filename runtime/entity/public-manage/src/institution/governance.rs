//! 公权机构治理结果原子应用。
//!
//! 业务模块不能直接改岗位、任职、机构信息或 admins；只能通过 runtime 路由提交
//! 已经完成的 [`InstitutionGovernanceResult`]。本文件只校验实体不变量，不解释任何
//! 提名、选举或其他业务规则。

extern crate alloc;

use admin_primitives::InstitutionAdminQuery as _;
use alloc::{collections::BTreeMap, vec::Vec};
use entity_primitives::{
    InstitutionAdminAssignment, InstitutionAssignmentSource, InstitutionAssignmentStatus,
    InstitutionCapabilityPolicy as _, InstitutionGovernanceResult,
    InstitutionLegalRepresentativeChange, InstitutionRole, InstitutionRoleMutation,
    InstitutionRoleStatus, RoleBusinessPermission, RoleSubject,
};
use frame_support::{
    dispatch::DispatchResult,
    ensure,
    storage::{with_transaction, TransactionOutcome},
    traits::Get,
};
use sp_std::collections::btree_set::BTreeSet;

use crate::institution::role::{
    AssignmentSourceRefOf, InstitutionRoleOf, ModuleTagOf, RoleAssignmentsOf, RoleCodeOf,
    RolePermissionsOf,
};
use crate::pallet::{
    AccountNameOf, CidNumberOf, Config, Error, InstitutionRoleAssignments, InstitutionRoleNonce,
    InstitutionRolePermissions, InstitutionRoles, Institutions, Pallet, UsedRoleCodes,
};

enum LegalRepresentativeTarget<T: Config> {
    Set(
        AccountNameOf<T>,
        AccountNameOf<T>,
        CidNumberOf<T>,
        T::AccountId,
    ),
    Clear,
}

impl<T: Config> Pallet<T> {
    /// 原子应用公权机构岗位、任职和法定代表人最终状态；管理员集合保持独立。
    pub fn apply_institution_governance_result(
        result: InstitutionGovernanceResult<T::AccountId>,
    ) -> DispatchResult {
        ensure!(
            admin_primitives::is_public_admin_code(&result.institution_code),
            Error::<T>::InvalidInstitutionCode
        );
        ensure!(
            !result.role_mutations.is_empty()
                || !result.assignment_changes.is_empty()
                || result.legal_representative_change.is_some(),
            Error::<T>::GovernanceResultEmpty
        );
        ensure!(
            !result.result_source_ref.is_empty(),
            Error::<T>::AssignmentSourceRefEmpty
        );
        ensure!(
            result.role_mutations.len() as u32 <= T::MaxAdmins::get()
                && result.assignment_changes.len() as u32 <= T::MaxAdmins::get(),
            Error::<T>::TooManyGovernanceChanges
        );
        let result_source_ref: AssignmentSourceRefOf = result
            .result_source_ref
            .try_into()
            .map_err(|_| Error::<T>::AssignmentSourceRefEmpty)?;

        let cid_number: CidNumberOf<T> = result
            .cid_number
            .clone()
            .try_into()
            .map_err(|_| Error::<T>::InvalidAssignmentResultInstitution)?;
        let institution = Institutions::<T>::get(&cid_number)
            .ok_or(Error::<T>::InvalidAssignmentResultInstitution)?;
        ensure!(
            institution.institution_code == result.institution_code,
            Error::<T>::InvalidAssignmentResultInstitution
        );
        let protected_institution = primitives::governance_skeleton::fixed_institution_by_identity(
            result.institution_code,
            cid_number.as_slice(),
        )
        .is_some();
        let member_composition =
            primitives::institution_constraints::member_composition_by_identity(
                result.institution_code,
                cid_number.as_slice(),
            );
        let current_admins = T::InstitutionAdminQuery::institution_admins(
            result.institution_code,
            cid_number.as_slice(),
        )
        .ok_or(Error::<T>::InvalidAssignmentResultInstitution)?;
        let current_admin_set = current_admins.iter().cloned().collect::<BTreeSet<_>>();

        let mut final_roles = InstitutionRoles::<T>::iter_prefix(&cid_number)
            .collect::<BTreeMap<RoleCodeOf, InstitutionRoleOf<T>>>();
        let mut role_writes = BTreeMap::<RoleCodeOf, InstitutionRoleOf<T>>::new();
        let mut permission_writes = BTreeMap::<RoleCodeOf, RolePermissionsOf<T>>::new();
        let mut created_assignment_writes = BTreeMap::<RoleCodeOf, RoleAssignmentsOf<T>>::new();
        let mut role_deletes = BTreeSet::<RoleCodeOf>::new();
        let mut next_role_nonce = InstitutionRoleNonce::<T>::get(&cid_number);
        for mutation in result.role_mutations {
            match mutation {
                InstitutionRoleMutation::Create {
                    role_name,
                    term_required,
                    permissions,
                    assignments,
                } => {
                    ensure!(!role_name.is_empty(), Error::<T>::InvalidRoleName);
                    ensure!(!permissions.is_empty(), Error::<T>::RolePermissionsEmpty);
                    let role_name: AccountNameOf<T> = role_name
                        .try_into()
                        .map_err(|_| Error::<T>::InvalidRoleName)?;
                    let (role_code, following_nonce) = Self::allocate_dynamic_role_code(
                        &cid_number,
                        next_role_nonce,
                        result.proposal_id,
                    )?;
                    next_role_nonce = following_nonce;
                    let role = InstitutionRole {
                        cid_number: cid_number.clone(),
                        role_code: role_code.clone(),
                        role_name,
                        term_required,
                        role_status: InstitutionRoleStatus::Active,
                    };
                    let mut seen_permissions = BTreeSet::new();
                    let mut stored_permissions = Vec::with_capacity(permissions.len());
                    for spec in permissions {
                        ensure!(
                            !spec.business_action_id.module_tag.is_empty(),
                            Error::<T>::InvalidRolePermission
                        );
                        ensure!(
                            T::InstitutionCapabilityPolicy::allows(
                                cid_number.as_slice(),
                                &spec.business_action_id,
                                spec.operation,
                            ),
                            Error::<T>::InstitutionCapabilityDenied
                        );
                        ensure!(
                            seen_permissions.insert((
                                spec.business_action_id.module_tag.clone(),
                                spec.business_action_id.action_code,
                                spec.operation as u8,
                            )),
                            Error::<T>::DuplicateRolePermission
                        );
                        let module_tag: ModuleTagOf = spec
                            .business_action_id
                            .module_tag
                            .try_into()
                            .map_err(|_| Error::<T>::InvalidRolePermission)?;
                        stored_permissions.push(RoleBusinessPermission {
                            role_subject: RoleSubject {
                                cid_number: cid_number.clone(),
                                role_code: role_code.clone(),
                            },
                            business_action_id: entity_primitives::BusinessActionId {
                                module_tag,
                                action_code: spec.business_action_id.action_code,
                            },
                            operation: spec.operation,
                        });
                    }
                    let stored_permissions: RolePermissionsOf<T> = stored_permissions
                        .try_into()
                        .map_err(|_| Error::<T>::TooManyRolePermissions)?;
                    let stored_assignments = Self::build_governance_assignments(
                        &cid_number,
                        &role_code,
                        &role,
                        assignments,
                        &current_admin_set,
                    )?;
                    role_writes.insert(role_code.clone(), role.clone());
                    permission_writes.insert(role_code.clone(), stored_permissions);
                    created_assignment_writes.insert(role_code.clone(), stored_assignments);
                    final_roles.insert(role_code, role);
                }
                InstitutionRoleMutation::Rename {
                    role_code,
                    role_name,
                } => {
                    ensure!(!role_code.is_empty(), Error::<T>::InvalidRoleCode);
                    ensure!(!role_name.is_empty(), Error::<T>::InvalidRoleName);
                    let role_code: RoleCodeOf = role_code
                        .try_into()
                        .map_err(|_| Error::<T>::InvalidRoleCode)?;
                    ensure!(
                        !primitives::institution_constraints::is_legal_representative_role(
                            role_code.as_slice()
                        ) && primitives::governance_skeleton::fixed_role_seats_by_identity(
                            result.institution_code,
                            cid_number.as_slice(),
                            role_code.as_slice(),
                        )
                        .is_none(),
                        Error::<T>::FixedRoleDefinitionImmutable
                    );
                    ensure!(
                        !role_writes.contains_key(&role_code) && !role_deletes.contains(&role_code),
                        Error::<T>::DuplicateGovernanceRoleChange
                    );
                    let mut role = final_roles
                        .get(&role_code)
                        .cloned()
                        .ok_or(Error::<T>::AssignmentRoleNotFound)?;
                    role.role_name = role_name
                        .try_into()
                        .map_err(|_| Error::<T>::InvalidRoleName)?;
                    role_writes.insert(role_code.clone(), role.clone());
                    final_roles.insert(role_code, role);
                }
                InstitutionRoleMutation::Delete { role_code } => {
                    ensure!(!role_code.is_empty(), Error::<T>::InvalidRoleCode);
                    let role_code: RoleCodeOf = role_code
                        .try_into()
                        .map_err(|_| Error::<T>::InvalidRoleCode)?;
                    ensure!(
                        !primitives::institution_constraints::is_legal_representative_role(
                            role_code.as_slice()
                        ) && primitives::governance_skeleton::fixed_role_seats_by_identity(
                            result.institution_code,
                            cid_number.as_slice(),
                            role_code.as_slice(),
                        )
                        .is_none(),
                        Error::<T>::FixedRoleDefinitionImmutable
                    );
                    ensure!(
                        final_roles.remove(&role_code).is_some()
                            && !role_writes.contains_key(&role_code)
                            && role_deletes.insert(role_code.clone()),
                        Error::<T>::DuplicateGovernanceRoleChange
                    );
                }
            }
        }

        // 岗位名称在机构内同样唯一：同名多人应当是同一个岗位的多个任职席位，
        // 不能通过另建岗位复制 LR 或创世固定岗位的公开名称。
        let mut final_role_names = BTreeSet::new();
        for role in final_roles.values() {
            ensure!(
                final_role_names.insert(role.role_name.clone()),
                Error::<T>::DuplicateRoleName
            );
        }

        let mut assignment_changes = BTreeMap::<RoleCodeOf, RoleAssignmentsOf<T>>::new();
        for change in result.assignment_changes {
            ensure!(!change.role_code.is_empty(), Error::<T>::InvalidRoleCode);
            let role_code: RoleCodeOf = change
                .role_code
                .try_into()
                .map_err(|_| Error::<T>::InvalidRoleCode)?;
            let role = final_roles
                .get(&role_code)
                .ok_or(Error::<T>::AssignmentRoleNotFound)?;
            ensure!(
                !assignment_changes.contains_key(&role_code)
                    && !created_assignment_writes.contains_key(&role_code)
                    && !role_deletes.contains(&role_code),
                Error::<T>::DuplicateGovernanceAssignmentChange
            );
            ensure!(
                change.assignments.len() as u32 <= T::MaxAdmins::get(),
                Error::<T>::TooManyInstitutionAdmins
            );
            let bounded = Self::build_governance_assignments(
                &cid_number,
                &role_code,
                role,
                change.assignments,
                &current_admin_set,
            )?;
            assignment_changes.insert(role_code, bounded);
        }

        for (role_code, role) in &final_roles {
            let assignments = assignment_changes
                .get(role_code)
                .or_else(|| created_assignment_writes.get(role_code))
                .cloned()
                .unwrap_or_else(|| InstitutionRoleAssignments::<T>::get(&cid_number, role_code));
            if role.role_status == InstitutionRoleStatus::Inactive {
                ensure!(
                    assignments.is_empty(),
                    Error::<T>::InactiveRoleHasAssignments
                );
                continue;
            }
            for assignment in &assignments {
                ensure!(
                    current_admin_set.contains(&assignment.account_id),
                    Error::<T>::InvalidAssignmentResultAdmins
                );
                ensure!(
                    assignment.assignment_status == InstitutionAssignmentStatus::Active,
                    Error::<T>::InitialAssignmentMustBeActive
                );
                Self::ensure_governance_assignment_term(
                    role,
                    assignment.term_start,
                    assignment.term_end,
                )?;
            }
            if primitives::institution_constraints::is_legal_representative_role(
                role_code.as_slice(),
            ) {
                ensure!(assignments.len() <= 1, Error::<T>::FixedRoleSeatsMismatch);
            }
            if protected_institution {
                if let Some((min_assignments, max_assignments)) =
                    primitives::governance_skeleton::fixed_role_assignment_bounds_by_identity(
                        result.institution_code,
                        cid_number.as_slice(),
                        role_code.as_slice(),
                    )
                {
                    ensure!(
                        assignments.len() >= min_assignments as usize
                            && assignments.len() <= max_assignments as usize,
                        Error::<T>::FixedRoleSeatsMismatch
                    );
                }
            }
        }
        if let Some(spec) = member_composition {
            let required_role_code: RoleCodeOf = spec
                .role_code
                .to_vec()
                .try_into()
                .map_err(|_| Error::<T>::InvalidRoleCode)?;
            let required_role = final_roles
                .get(&required_role_code)
                .ok_or(Error::<T>::RequiredMemberRoleMissing)?;
            ensure!(
                required_role.role_status == InstitutionRoleStatus::Active,
                Error::<T>::RequiredMemberRoleMissing
            );
            ensure!(
                required_role.role_name.as_slice() == spec.role_name,
                Error::<T>::RequiredMemberRoleNameMismatch
            );
            let required_assignments = assignment_changes
                .get(&required_role_code)
                .cloned()
                .unwrap_or_else(|| {
                    InstitutionRoleAssignments::<T>::get(&cid_number, &required_role_code)
                });
            ensure!(
                required_assignments.len() >= spec.min_members as usize
                    && required_assignments.len() <= spec.max_members as usize,
                Error::<T>::RequiredMemberCountOutOfRange
            );
            let required_admins = required_assignments
                .iter()
                .map(|assignment| assignment.account_id.clone())
                .collect::<BTreeSet<_>>();
            ensure!(
                required_admins == current_admin_set,
                Error::<T>::NonMemberAdminForbidden
            );
        }

        let legal_representative_change = result
            .legal_representative_change
            .map(|change| {
                match change {
                    InstitutionLegalRepresentativeChange::Set {
                        family_name,
                        given_name,
                        cid_number,
                        account_id,
                    } => {
                        ensure!(
                            !family_name.is_empty() && !given_name.is_empty(),
                            Error::<T>::EmptyLegalRepresentativeName
                        );
                        ensure!(
                            !cid_number.is_empty(),
                            Error::<T>::EmptyLegalRepresentativeCidNumber
                        );
                        let family_name: AccountNameOf<T> = family_name
                            .try_into()
                            .map_err(|_| Error::<T>::EmptyLegalRepresentativeName)?;
                        let given_name: AccountNameOf<T> = given_name
                            .try_into()
                            .map_err(|_| Error::<T>::EmptyLegalRepresentativeName)?;
                        let citizen_cid: CidNumberOf<T> = cid_number
                            .try_into()
                            .map_err(|_| Error::<T>::EmptyLegalRepresentativeCidNumber)?;
                        Ok::<_, sp_runtime::DispatchError>(LegalRepresentativeTarget::<T>::Set(
                            family_name,
                            given_name,
                            citizen_cid,
                            account_id,
                        ))
                    }
                    // 解除法定代表人只清空 InstitutionInfo 三字段，不影响 LR 岗位本身。
                    InstitutionLegalRepresentativeChange::Clear => {
                        Ok(LegalRepresentativeTarget::<T>::Clear)
                    }
                }
            })
            .transpose()?;
        let legal_representative_account = match &legal_representative_change {
            Some(LegalRepresentativeTarget::Set(_, _, _, account_id)) => Some(account_id.clone()),
            Some(LegalRepresentativeTarget::Clear) => None,
            None => institution
                .legal_representative
                .as_ref()
                .map(|representative| representative.account_id.clone()),
        };
        let legal_role_code: RoleCodeOf =
            primitives::institution_constraints::ROLE_CODE_LEGAL_REPRESENTATIVE
                .to_vec()
                .try_into()
                .map_err(|_| Error::<T>::InvalidRoleCode)?;
        let legal_assignments = assignment_changes
            .get(&legal_role_code)
            .cloned()
            .unwrap_or_else(|| InstitutionRoleAssignments::<T>::get(&cid_number, &legal_role_code));
        ensure!(
            match legal_representative_account {
                Some(account_id) => {
                    legal_assignments.len() == 1 && legal_assignments[0].account_id == account_id
                }
                None => legal_assignments.is_empty(),
            },
            Error::<T>::FixedRoleSeatsMismatch
        );
        let role_mutations_len = (role_writes.len() + role_deletes.len()) as u32;
        let assignment_changes_len = assignment_changes.len() as u32;
        let admins_len = current_admins.len() as u32;
        let legal_representative_updated = legal_representative_change.is_some();

        with_transaction(|| {
            for role_code in &role_deletes {
                InstitutionRoles::<T>::remove(&cid_number, role_code);
                InstitutionRolePermissions::<T>::remove(&cid_number, role_code);
                InstitutionRoleAssignments::<T>::remove(&cid_number, role_code);
            }
            for (role_code, role) in &role_writes {
                InstitutionRoles::<T>::insert(&cid_number, role_code, role.clone());
            }
            for (role_code, permissions) in &permission_writes {
                InstitutionRolePermissions::<T>::insert(
                    &cid_number,
                    role_code,
                    permissions.clone(),
                );
                UsedRoleCodes::<T>::insert(&cid_number, role_code, true);
            }
            for (role_code, assignments) in &created_assignment_writes {
                InstitutionRoleAssignments::<T>::insert(
                    &cid_number,
                    role_code,
                    assignments.clone(),
                );
            }
            for (role_code, assignments) in &assignment_changes {
                InstitutionRoleAssignments::<T>::insert(
                    &cid_number,
                    role_code,
                    assignments.clone(),
                );
            }
            InstitutionRoleNonce::<T>::insert(&cid_number, next_role_nonce);
            if let Some(change) = legal_representative_change {
                Institutions::<T>::mutate(&cid_number, |maybe| {
                    if let Some(info) = maybe {
                        match change {
                            LegalRepresentativeTarget::Set(
                                family_name,
                                given_name,
                                citizen_cid,
                                account_id,
                            ) => {
                                info.legal_representative =
                                    Some(entity_primitives::LegalRepresentative {
                                        family_name,
                                        given_name,
                                        cid_number: citizen_cid,
                                        account_id,
                                    });
                            }
                            LegalRepresentativeTarget::Clear => {
                                info.legal_representative = None;
                            }
                        }
                    }
                });
            }
            Self::deposit_event(crate::pallet::Event::<T>::InstitutionGovernanceApplied {
                cid_number,
                role_mutations: role_mutations_len,
                assignment_changes: assignment_changes_len,
                admins_len,
                legal_representative_updated,
                result_source_ref,
            });
            TransactionOutcome::Commit(Ok(()))
        })
    }

    fn build_governance_assignments(
        cid_number: &CidNumberOf<T>,
        role_code: &RoleCodeOf,
        role: &InstitutionRoleOf<T>,
        targets: Vec<entity_primitives::InstitutionAssignmentTarget<T::AccountId>>,
        current_admin_set: &BTreeSet<T::AccountId>,
    ) -> Result<RoleAssignmentsOf<T>, sp_runtime::DispatchError> {
        ensure!(
            targets.len() as u32 <= T::MaxAdmins::get(),
            Error::<T>::TooManyInstitutionAdmins
        );
        let mut seen_accounts = BTreeSet::new();
        let mut stored_assignments = Vec::with_capacity(targets.len());
        for target in targets {
            ensure!(
                current_admin_set.contains(&target.account_id),
                Error::<T>::InvalidAssignmentResultAdmins
            );
            ensure!(
                target.assignment_status == InstitutionAssignmentStatus::Active,
                Error::<T>::InitialAssignmentMustBeActive
            );
            ensure!(
                matches!(
                    target.assignment_source,
                    InstitutionAssignmentSource::PopularElection
                        | InstitutionAssignmentSource::MutualElection
                        | InstitutionAssignmentSource::NominationAppointment
                        | InstitutionAssignmentSource::InstitutionGovernance
                ),
                Error::<T>::InvalidAssignmentSource
            );
            ensure!(
                !target.assignment_source_ref.is_empty(),
                Error::<T>::AssignmentSourceRefEmpty
            );
            ensure!(
                seen_accounts.insert(target.account_id.clone()),
                Error::<T>::DuplicateAssignment
            );
            Self::ensure_governance_assignment_term(role, target.term_start, target.term_end)?;
            let assignment_source_ref: AssignmentSourceRefOf = target
                .assignment_source_ref
                .try_into()
                .map_err(|_| Error::<T>::AssignmentSourceRefEmpty)?;
            stored_assignments.push(InstitutionAdminAssignment {
                cid_number: cid_number.clone(),
                account_id: target.account_id,
                role_code: role_code.clone(),
                term_start: target.term_start,
                term_end: target.term_end,
                assignment_source: target.assignment_source,
                assignment_source_ref,
                assignment_status: target.assignment_status,
            });
        }
        stored_assignments
            .try_into()
            .map_err(|_| Error::<T>::TooManyInstitutionAdmins.into())
    }

    fn ensure_governance_assignment_term(
        role: &InstitutionRoleOf<T>,
        term_start: u32,
        term_end: u32,
    ) -> DispatchResult {
        if role.term_required {
            ensure!(
                term_start > 0 && term_end >= term_start,
                Error::<T>::InvalidAssignmentTerm
            );
        } else {
            ensure!(
                term_start == 0 && term_end == 0,
                Error::<T>::UnexpectedAssignmentTerm
            );
        }
        Ok(())
    }
}
