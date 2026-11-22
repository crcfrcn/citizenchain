//! 原子应用私权机构治理及注册局登记，核对状态指纹和派生不变量。

extern crate alloc;

use adminbase::{ChainPhaseCheck as _, InstitutionAdminQuery as _};
use alloc::{collections::BTreeMap, vec::Vec};
use entitybase::RegistryAuthority as _;
use entitybase::{
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
    /// 原子应用私权机构岗位、任职和法定代表人最终状态；管理员集合保持独立。
    pub fn apply_institution_governance_result(
        result: InstitutionGovernanceResult<T::AccountId>,
    ) -> DispatchResult {
        Self::apply_role_changes(
            result.institution_code,
            result.cid_number,
            result.role_mutations,
            result.assignment_changes,
            result.legal_representative_change,
            result.result_source_ref,
            Some(result.proposal_id),
        )
    }

    /// 登记和治理共用实体校验与原子写入；来源由已授权入口确定，不能由客户端降级。
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn apply_role_changes(
        institution_code: primitives::cid::code::InstitutionCode,
        cid_number: Vec<u8>,
        role_mutations: Vec<entitybase::InstitutionRoleMutation<T::AccountId>>,
        assignment_changes_input: Vec<entitybase::InstitutionRoleAssignmentChange<T::AccountId>>,
        legal_representative_change_input: Option<
            entitybase::InstitutionLegalRepresentativeChange<T::AccountId>,
        >,
        source_ref: Vec<u8>,
        proposal_id: Option<u128>,
    ) -> DispatchResult {
        ensure!(
            adminbase::is_private_admin_code(&institution_code),
            Error::<T>::InvalidInstitutionCode
        );
        ensure!(
            !role_mutations.is_empty()
                || !assignment_changes_input.is_empty()
                || legal_representative_change_input.is_some(),
            Error::<T>::GovernanceResultEmpty
        );
        ensure!(!source_ref.is_empty(), Error::<T>::AssignmentSourceRefEmpty);
        ensure!(
            role_mutations.len() as u32 <= T::MaxAdmins::get()
                && assignment_changes_input.len() as u32 <= T::MaxAdmins::get(),
            Error::<T>::TooManyGovernanceChanges
        );
        let result_source_ref: AssignmentSourceRefOf = source_ref
            .try_into()
            .map_err(|_| Error::<T>::AssignmentSourceRefEmpty)?;

        let cid_number: CidNumberOf<T> = cid_number
            .clone()
            .try_into()
            .map_err(|_| Error::<T>::InvalidAssignmentResultInstitution)?;
        let institution = Institutions::<T>::get(&cid_number)
            .ok_or(Error::<T>::InvalidAssignmentResultInstitution)?;
        ensure!(
            institution.institution_code == institution_code,
            Error::<T>::InvalidAssignmentResultInstitution
        );
        let protected_foundation =
            primitives::cid::china::citizenchain::is_citizenchain_foundation_identity(
                institution_code,
                cid_number.as_slice(),
            );
        let current_admins =
            T::InstitutionAdminQuery::institution_admins(institution_code, cid_number.as_slice())
                .ok_or(Error::<T>::InvalidAssignmentResultInstitution)?;
        let current_admin_set = current_admins.iter().cloned().collect::<BTreeSet<_>>();

        let mut role_mutations = role_mutations;
        let mut assignment_changes_input = assignment_changes_input;
        if proposal_id.is_none() {
            for mutation in &mut role_mutations {
                if let InstitutionRoleMutation::Create { assignments, .. } = mutation {
                    for target in assignments {
                        ensure!(
                            target.assignment_source == InstitutionAssignmentSource::Registry,
                            Error::<T>::InvalidAssignmentSource
                        );
                        ensure!(
                            target.assignment_source_ref.is_empty(),
                            Error::<T>::InvalidAssignmentSource
                        );
                        target.assignment_source_ref = result_source_ref.to_vec();
                    }
                }
            }
            for change in &mut assignment_changes_input {
                let code: RoleCodeOf = change
                    .role_code
                    .clone()
                    .try_into()
                    .map_err(|_| Error::<T>::InvalidRoleCode)?;
                let previous = InstitutionRoleAssignments::<T>::get(&cid_number, &code)
                    .into_iter()
                    .map(|assignment| (assignment.account_id.clone(), assignment))
                    .collect::<BTreeMap<_, _>>();
                // 受保护岗位的任职仍由既有治理/任免处理，登记只能透传未变更记录。
                let protected_role = primitives::institution_constraints::is_legal_representative_role(&change.role_code)
                    || primitives::governance_skeleton::fixed_role_seats_by_identity(institution_code, cid_number.as_slice(), &change.role_code).is_some()
                    || (primitives::cid::china::citizenchain::is_citizenchain_foundation_identity(institution_code, cid_number.as_slice())
                        && primitives::cid::china::citizenchain::fixed_role(&change.role_code).is_some());
                if protected_role {
                    ensure!(
                        change.assignments.len() == previous.len()
                            && change.assignments.iter().all(|target| previous
                                .get(&target.account_id)
                                .is_some_and(|stored| Self::same_assignment(stored, target))),
                        Error::<T>::RegistrationRoleChangeDenied
                    );
                }
                for target in &mut change.assignments {
                    if previous
                        .get(&target.account_id)
                        .is_some_and(|stored| Self::same_assignment(stored, target))
                    {
                        continue;
                    }
                    ensure!(
                        target.assignment_source == InstitutionAssignmentSource::Registry
                            && target.assignment_source_ref.is_empty(),
                        Error::<T>::InvalidAssignmentSource
                    );
                    target.assignment_source_ref = result_source_ref.to_vec();
                }
            }
        }

        let mut final_roles = InstitutionRoles::<T>::iter_prefix(&cid_number)
            .collect::<BTreeMap<RoleCodeOf, InstitutionRoleOf<T>>>();
        let mut role_writes = BTreeMap::<RoleCodeOf, InstitutionRoleOf<T>>::new();
        let mut permission_writes = BTreeMap::<RoleCodeOf, RolePermissionsOf<T>>::new();
        let mut created_assignment_writes = BTreeMap::<RoleCodeOf, RoleAssignmentsOf<T>>::new();
        let mut role_deletes = BTreeSet::<RoleCodeOf>::new();
        let mut next_role_nonce = InstitutionRoleNonce::<T>::get(&cid_number);
        for mutation in role_mutations {
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
                    let (role_code, following_nonce) = match proposal_id {
                        Some(id) => {
                            Self::allocate_dynamic_role_code(&cid_number, next_role_nonce, id)?
                        }
                        None => Self::allocate_registration_role_code(
                            &cid_number,
                            next_role_nonce,
                            result_source_ref.as_slice(),
                        )?,
                    };
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
                            business_action_id: entitybase::BusinessActionId {
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
                        proposal_id
                            .is_none()
                            .then_some(result_source_ref.as_slice()),
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
                        ) && (!protected_foundation
                            || primitives::cid::china::citizenchain::fixed_role(
                                role_code.as_slice()
                            )
                            .is_none()),
                        Error::<T>::FixedRoleSeatsMismatch
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
                        ) && (!protected_foundation
                            || primitives::cid::china::citizenchain::fixed_role(
                                role_code.as_slice()
                            )
                            .is_none()),
                        Error::<T>::FixedRoleSeatsMismatch
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

        // 岗位名称在机构内同样唯一；同名多人通过一个岗位的多个任职表达。
        let mut final_role_names = BTreeSet::new();
        for role in final_roles.values() {
            ensure!(
                final_role_names.insert(role.role_name.clone()),
                Error::<T>::DuplicateRoleName
            );
        }

        let mut assignment_changes = BTreeMap::<RoleCodeOf, RoleAssignmentsOf<T>>::new();
        for change in assignment_changes_input {
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
                proposal_id
                    .is_none()
                    .then_some(result_source_ref.as_slice()),
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
            if protected_foundation {
                if let Some(fixed_role) =
                    primitives::cid::china::citizenchain::fixed_role(role_code.as_slice())
                {
                    let assignments_valid =
                        if primitives::institution_constraints::is_legal_representative_role(
                            role_code.as_slice(),
                        ) {
                            assignments.len() <= 1
                        } else {
                            assignments.len() == fixed_role.seats as usize
                        };
                    ensure!(
                        role.role_status == InstitutionRoleStatus::Active
                            && role.role_name.as_slice() == fixed_role.role_name
                            && assignments_valid,
                        Error::<T>::FixedRoleSeatsMismatch
                    );
                }
            }
        }

        let legal_representative_change = legal_representative_change_input
            .map(|change| {
                match change {
                    InstitutionLegalRepresentativeChange::Set {
                        family_name,
                        given_name,
                        cid_number,
                        account_id,
                    } => {
                        // 运行期(Operation):LR 岗四要素完整(单源自 required_admin_elements
                        // 的私权 LR 岗真值);Genesis 放行=允许空。一次 runtime 升级即启用。
                        if T::ChainPhase::is_operation() {
                            let req = adminbase::required_admin_elements(
                                adminbase::AdminAccountKind::PrivateInstitution,
                                true, // LR 岗
                            );
                            ensure!(
                                (!req.family || !family_name.is_empty())
                                    && (!req.given || !given_name.is_empty()),
                                Error::<T>::EmptyLegalRepresentativeName
                            );
                            ensure!(
                                !req.cid || !cid_number.is_empty(),
                                Error::<T>::EmptyLegalRepresentativeCidNumber
                            );
                        }
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
                                info.legal_representative = Some(entitybase::LegalRepresentative {
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

    /// 只读授权状态哈希：名册规范账户、岗位/权限/任职、nonce、阈值和机构信息。
    /// 名册每次成功写入都推进现有nonce，因此并发人员资料变更也使旧签名失效。
    pub fn registration_state_hash(
        cid: &CidNumberOf<T>,
    ) -> Result<[u8; 32], sp_runtime::DispatchError> {
        let institution = Institutions::<T>::get(cid).ok_or(Error::<T>::InstitutionNotFound)?;
        let admins = T::InstitutionAdminQuery::institution_admins(
            institution.institution_code,
            cid.as_slice(),
        );
        let roles = InstitutionRoles::<T>::iter_prefix(cid)
            .take(T::MaxAdmins::get() as usize + 1)
            .collect::<BTreeMap<_, _>>();
        ensure!(
            roles.len() <= T::MaxAdmins::get() as usize,
            Error::<T>::TooManyGovernanceChanges
        );
        let permissions = roles
            .keys()
            .map(|role| {
                (
                    role.clone(),
                    InstitutionRolePermissions::<T>::get(cid, role),
                )
            })
            .collect::<Vec<_>>();
        let assignments = roles
            .keys()
            .map(|role| {
                (
                    role.clone(),
                    InstitutionRoleAssignments::<T>::get(cid, role),
                )
            })
            .collect::<Vec<_>>();
        Ok(entitybase::institution_governance::registration_state_hash(
            crate::MODULE_TAG,
            &(
                cid,
                institution,
                admins,
                roles,
                permissions,
                assignments,
                InstitutionRoleNonce::<T>::get(cid),
                crate::pallet::InstitutionGovernanceThresholds::<T>::get(cid),
            ),
        ))
    }

    /// 权重涵盖当前快照以及本次载荷，遍历上限与实际登记校验相同。
    pub fn registration_weight(
        cid: &CidNumberOf<T>,
        action: &crate::pallet::InstitutionGovernanceActionOf<T>,
    ) -> frame_support::weights::Weight {
        let (n, mut r, mut p, mut a) = action.registration_work();
        // 只读compact长度估价，避免在入池/报价阶段解码整批任职或权限值。
        for (role, _) in
            InstitutionRoles::<T>::iter_prefix(cid).take(T::MaxAdmins::get() as usize + 1)
        {
            r = r.saturating_add(1);
            p = p.saturating_add(
                InstitutionRolePermissions::<T>::decode_len(cid, &role).unwrap_or(0) as u32,
            );
            a = a.saturating_add(
                InstitutionRoleAssignments::<T>::decode_len(cid, &role).unwrap_or(0) as u32,
            );
        }
        let previous_admins = Institutions::<T>::get(cid)
            .and_then(|institution| {
                T::InstitutionAdminQuery::institution_admins_len(
                    institution.institution_code,
                    cid.as_slice(),
                )
            })
            .unwrap_or(0);
        <<T as Config>::WeightInfo as crate::weights::WeightInfo>::register_institution_admins(
            n.saturating_add(previous_admins),
            r,
            p,
            a,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn apply_registration(
        who: T::AccountId,
        cid: CidNumberOf<T>,
        action: crate::pallet::InstitutionGovernanceActionOf<T>,
        actor_cid: Vec<u8>,
        actor_role: Vec<u8>,
        expected_state_hash: [u8; 32],
        initial_threshold: Option<u32>,
    ) -> DispatchResult {
        let institution = Institutions::<T>::get(&cid).ok_or(Error::<T>::InstitutionNotFound)?;
        Self::ensure_lifecycle_institution_code(&institution.institution_code)?;
        ensure!(
            T::RegistryAuthority::can_register_institution_origin(
                &who,
                &actor_cid,
                &actor_role,
                &cid,
                institution.institution_code
            ),
            Error::<T>::RegistryAuthorityDenied
        );
        ensure!(
            Self::registration_state_hash(&cid)? == expected_state_hash,
            Error::<T>::StaleRegistrationState
        );
        // 来源绑定已签名登记载荷和origin；不接受客户端自造来源，也不伪造proposal_id。
        let reference = entitybase::institution_governance::registration_reference(
            crate::MODULE_TAG,
            &(
                &who,
                &cid,
                &action,
                &actor_cid,
                &actor_role,
                expected_state_hash,
                initial_threshold,
            ),
        );
        let (admins, mutations, assignments) = action
            .into_registration_parts()
            .ok_or(Error::<T>::InvalidRegistrationAction)?;
        ensure!(
            mutations.len() <= T::MaxAdmins::get() as usize
                && assignments.len() <= T::MaxAdmins::get() as usize,
            Error::<T>::TooManyGovernanceChanges
        );
        let initialized = crate::pallet::InstitutionGovernanceThresholds::<T>::get(&cid);
        match initialized {
            Some(_) => {
                ensure!(initial_threshold.is_none(), Error::<T>::AlreadyInitialized);
                ensure!(
                    mutations.is_empty(),
                    Error::<T>::RegistrationRoleChangeDenied
                );
            }
            None => ensure!(
                initial_threshold.is_some() && !mutations.is_empty(),
                Error::<T>::MissingInitialConfiguration
            ),
        }
        // 登记允许首次动态建岗，不允许改名/删除既有岗位或隐式任命法定代表人。
        ensure!(
            mutations
                .iter()
                .all(|mutation| matches!(mutation, InstitutionRoleMutation::Create { .. })),
            Error::<T>::RegistrationRoleChangeDenied
        );
        let admins: crate::pallet::InstitutionAdminsInputOf<T> = admins
            .try_into()
            .map_err(|_| Error::<T>::TooManyInstitutionAdmins)?;
        let admins = Self::normalize_institution_admins(admins);
        Self::ensure_admin_config(&admins)?;
        with_transaction(|| {
            let apply = || -> DispatchResult {
                Self::set_institution_admins(&cid, institution.institution_code, &admins)?;
                if !mutations.is_empty() || !assignments.is_empty() {
                    Self::apply_role_changes(
                        institution.institution_code,
                        cid.to_vec(),
                        mutations,
                        assignments,
                        None,
                        reference.to_vec(),
                        None,
                    )?;
                }
                if let Some(threshold) = initial_threshold {
                    crate::pallet::InstitutionGovernanceThresholds::<T>::insert(&cid, threshold);
                }
                Self::ensure_registration_configuration(&cid)?;
                Self::deposit_event(crate::pallet::Event::<T>::InstitutionAdminsRegistered {
                    cid_number: cid,
                    admins_len: admins.len() as u32,
                    submitter: who,
                });
                Ok(())
            };
            match apply() {
                Ok(()) => TransactionOutcome::Commit(Ok(())),
                Err(error) => TransactionOutcome::Rollback(Err(error)),
            }
        })
    }

    /// 阈值来自显式机构配置，按有效岗位票据校验；不由管理员钱包人数推导。
    fn ensure_registration_configuration(cid: &CidNumberOf<T>) -> DispatchResult {
        let institution = Institutions::<T>::get(cid).ok_or(Error::<T>::InstitutionNotFound)?;
        let threshold = crate::pallet::InstitutionGovernanceThresholds::<T>::get(cid)
            .ok_or(Error::<T>::MissingInitialConfiguration)?;
        let admins = T::InstitutionAdminQuery::institution_admins(
            institution.institution_code,
            cid.as_slice(),
        )
        .ok_or(Error::<T>::InvalidAssignmentResultAdmins)?
        .into_iter()
        .collect::<BTreeSet<_>>();
        let action = entitybase::BusinessActionId {
            module_tag: crate::MODULE_TAG.to_vec(),
            action_code: entitybase::business_action::ACTION_INSTITUTION_GOVERNANCE,
        };
        let mut proposer = false;
        let mut voters = 0u32;
        let mut role_count = 0u32;
        for (code, role) in
            InstitutionRoles::<T>::iter_prefix(cid).take(T::MaxAdmins::get() as usize + 1)
        {
            role_count = role_count.saturating_add(1);
            ensure!(
                role_count <= T::MaxAdmins::get(),
                Error::<T>::TooManyGovernanceChanges
            );
            let assignments = InstitutionRoleAssignments::<T>::get(cid, &code);
            ensure!(
                assignments
                    .iter()
                    .all(|assignment| admins.contains(&assignment.account_id)),
                Error::<T>::InvalidAssignmentResultAdmins
            );
            if role.role_status == InstitutionRoleStatus::Inactive {
                ensure!(
                    assignments.is_empty(),
                    Error::<T>::InactiveRoleHasAssignments
                );
                continue;
            }
            let active = assignments
                .iter()
                .filter(|assignment| Self::is_assignment_effective(&role, assignment))
                .count() as u32;
            let subject = RoleSubject {
                cid_number: cid.to_vec(),
                role_code: code.to_vec(),
            };
            if <Self as entitybase::InstitutionRoleAuthorizationQuery<T::AccountId>>::role_has_permission(&subject, &action, entitybase::RolePermissionOperation::Propose) {
                proposer |= active > 0;
            }
            if <Self as entitybase::InstitutionRoleAuthorizationQuery<T::AccountId>>::role_has_permission(&subject, &action, entitybase::RolePermissionOperation::Vote) {
                voters = voters.saturating_add(active);
            }
        }
        ensure!(
            proposer
                && voters >= 2
                && threshold > 0
                && threshold <= voters
                && u64::from(threshold).saturating_mul(2) > u64::from(voters),
            Error::<T>::InvalidThreshold
        );
        Ok(())
    }

    fn build_governance_assignments(
        cid_number: &CidNumberOf<T>,
        role_code: &RoleCodeOf,
        role: &InstitutionRoleOf<T>,
        targets: Vec<entitybase::InstitutionAssignmentTarget<T::AccountId>>,
        current_admin_set: &BTreeSet<T::AccountId>,
        registry_ref: Option<&[u8]>,
    ) -> Result<RoleAssignmentsOf<T>, sp_runtime::DispatchError> {
        ensure!(
            targets.len() as u32 <= T::MaxAdmins::get(),
            Error::<T>::TooManyInstitutionAdmins
        );
        let previous = if registry_ref.is_some() {
            InstitutionRoleAssignments::<T>::get(cid_number, role_code)
                .into_iter()
                .map(|assignment| (assignment.account_id.clone(), assignment))
                .collect::<BTreeMap<_, _>>()
        } else {
            BTreeMap::new()
        };
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
                match registry_ref {
                    Some(reference) => {
                        (target.assignment_source == InstitutionAssignmentSource::Registry
                            && target.assignment_source_ref.as_slice() == reference)
                            || previous
                                .get(&target.account_id)
                                .is_some_and(|stored| Self::same_assignment(stored, &target))
                    }
                    None => matches!(
                        target.assignment_source,
                        InstitutionAssignmentSource::PopularElection
                            | InstitutionAssignmentSource::MutualElection
                            | InstitutionAssignmentSource::NominationAppointment
                            | InstitutionAssignmentSource::InstitutionGovernance
                    ),
                },
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

    /// 存量任职逐字段相同时才保留来源，不能借旧引用改人或改任期。
    fn same_assignment(
        stored: &crate::institution::role::InstitutionAdminAssignmentOf<T>,
        target: &entitybase::InstitutionAssignmentTarget<T::AccountId>,
    ) -> bool {
        stored.account_id == target.account_id
            && stored.term_start == target.term_start
            && stored.term_end == target.term_end
            && stored.assignment_source == target.assignment_source
            && stored.assignment_status == target.assignment_status
            && stored.assignment_source_ref.as_slice() == target.assignment_source_ref.as_slice()
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
