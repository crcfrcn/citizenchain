//! 普通公权机构原生政策：原选民批准、目标状态重验及事务内更新。

extern crate alloc;

use crate::weights::WeightInfo as _;
use crate::{
    institution::role::{ModuleTagOf, RoleCodeOf, RolePermissionsOf},
    pallet::*,
};
use alloc::{collections::BTreeSet, vec::Vec};
use codec::Encode;
use engingcore::InternalVoteEngine as _;
use entitybase::{
    business_action::{
        is_ordinary_institution, ACTION_INSTITUTION_CLOSE, ACTION_INSTITUTION_GOVERNANCE,
        ACTION_MULTISIG_TRANSFER, MODULE_MULTISIG,
    },
    institution_governance::{MAX_POLICY_PERMISSIONS, MAX_POLICY_ROLES},
    InstitutionCapabilityPolicy as _, InstitutionPolicyAction, InstitutionPolicyProposal,
    InstitutionRoleStatus, RoleBusinessPermission, RolePermissionOperation, RoleSubject,
};
use frame_support::{dispatch::DispatchResult, ensure, traits::Get};

impl<T: Config> Pallet<T> {
    /// 入池阶段只读compact长度，预算包括原状态、暂存目标及完整载荷。
    pub fn policy_state_weight(cid: &CidNumberOf<T>, bytes: u32) -> frame_support::weights::Weight {
        let (mut roles, mut permissions, mut assignments) = (0u32, 0u32, 0u32);
        for (code, _) in InstitutionRoles::<T>::iter_prefix(cid).take(MAX_POLICY_ROLES as usize + 1)
        {
            roles = roles.saturating_add(1);
            permissions = permissions.saturating_add(
                InstitutionRolePermissions::<T>::decode_len(cid, &code).unwrap_or(0) as u32,
            );
            assignments = assignments.saturating_add(
                InstitutionRoleAssignments::<T>::decode_len(cid, &code).unwrap_or(0) as u32,
            );
        }
        <T as Config>::WeightInfo::policy_state(
            roles,
            permissions,
            assignments,
            bytes.saturating_add(255) / 256,
        )
    }

    /// 三次扫描的耗时相加；相同存储键的证明共享，不重复累计同一证明。
    pub fn policy_weight(
        cid: &CidNumberOf<T>,
        action: &InstitutionPolicyAction,
    ) -> frame_support::weights::Weight {
        let state = Self::policy_state_weight(cid, action.encoded_size() as u32);
        <T as Config>::WeightInfo::propose_institution_governance().saturating_add(
            frame_support::weights::Weight::from_parts(
                state.ref_time().saturating_mul(3),
                state.proof_size(),
            ),
        )
    }

    /// 只允许普通机构修改由本机构真实分配、永久占用的动态岗位。
    pub(crate) fn ensure_policy_role(cid: &CidNumberOf<T>, role: &RoleCodeOf) -> DispatchResult {
        let info = Institutions::<T>::get(cid).ok_or(Error::<T>::InstitutionNotFound)?;
        ensure!(
            is_ordinary_institution(info.institution_code, cid),
            Error::<T>::InstitutionPolicyDenied
        );
        ensure!(
            role.starts_with(b"R_") && UsedRoleCodes::<T>::get(cid, role),
            Error::<T>::InstitutionPolicyDenied
        );
        let stored =
            InstitutionRoles::<T>::get(cid, role).ok_or(Error::<T>::AssignmentRoleNotFound)?;
        ensure!(
            stored.cid_number == *cid && stored.role_code == *role,
            Error::<T>::RoleCidMismatch
        );
        Ok(())
    }

    /// 当前原生内部业务共同使用一个阈值；链下岗位不进入本表或选民分母。
    pub(crate) fn ensure_configuration(cid: &CidNumberOf<T>) -> DispatchResult {
        let threshold = InstitutionGovernanceThresholds::<T>::get(cid)
            .ok_or(Error::<T>::MissingInitialConfiguration)?;
        let actions = [
            (crate::MODULE_TAG, ACTION_INSTITUTION_GOVERNANCE),
            (crate::MODULE_TAG, ACTION_INSTITUTION_CLOSE),
            (MODULE_MULTISIG, ACTION_MULTISIG_TRANSFER),
        ];
        let mut proposers = [false; 3];
        let mut voters = [0u32; 3];
        let mut configured = [true, false, false];
        let mut role_count = 0u32;
        let mut permission_count = 0u32;
        let mut assignment_count = 0u32;
        for (code, role) in
            InstitutionRoles::<T>::iter_prefix(cid).take(MAX_POLICY_ROLES as usize + 1)
        {
            role_count = role_count.saturating_add(1);
            ensure!(
                role_count <= MAX_POLICY_ROLES.min(T::MaxRoleEntries::get()),
                Error::<T>::TooManyGovernanceChanges
            );
            ensure!(
                role.cid_number == *cid && role.role_code == code,
                Error::<T>::RoleCidMismatch
            );
            let permissions = InstitutionRolePermissions::<T>::get(cid, &code);
            permission_count = permission_count.saturating_add(permissions.len() as u32);
            ensure!(
                permission_count <= MAX_POLICY_PERMISSIONS,
                Error::<T>::TooManyRolePermissions
            );
            let assignments = InstitutionRoleAssignments::<T>::get(cid, &code);
            assignment_count = assignment_count.saturating_add(assignments.len() as u32);
            ensure!(
                assignment_count <= T::MaxRoleEntries::get(),
                Error::<T>::TooManyInstitutionAssignments
            );
            ensure!(
                assignments.len() <= role.seats as usize,
                Error::<T>::FixedRoleSeatsMismatch
            );
            if role.role_status == InstitutionRoleStatus::Inactive {
                ensure!(
                    assignments.is_empty(),
                    Error::<T>::InactiveRoleHasAssignments
                );
                continue;
            }
            let mut persons = BTreeSet::new();
            let mut active = 0u32;
            for assignment in &assignments {
                ensure!(
                    assignment.cid_number == *cid && assignment.role_code == code,
                    Error::<T>::AssignmentCidMismatch
                );
                ensure!(
                    persons.insert(assignment.person_cid.clone()),
                    Error::<T>::DuplicateAssignment
                );
                if role.role_status == InstitutionRoleStatus::Active
                    && Self::is_assignment_effective(&role, assignment)
                {
                    // 与真正建案相同：缺绑定不得被过滤后降低分母。
                    ensure!(
                        Self::has_person_binding(&assignment.person_cid),
                        Error::<T>::InvalidAssignmentPerson
                    );
                    active = active.saturating_add(1);
                }
            }
            for permission in &permissions {
                ensure!(
                    permission.role_subject.cid_number == *cid
                        && permission.role_subject.role_code == code,
                    Error::<T>::InvalidRolePermission
                );
                for (index, (module, action)) in actions.iter().enumerate() {
                    if permission.business_action_id.module_tag.as_slice() == *module
                        && permission.business_action_id.action_code == *action
                    {
                        configured[index] = true;
                        match permission.operation {
                            RolePermissionOperation::Propose => proposers[index] |= active > 0,
                            RolePermissionOperation::Vote => {
                                voters[index] = voters[index].saturating_add(active)
                            }
                        }
                    }
                }
            }
        }
        for index in 0..actions.len() {
            if configured[index] {
                let count = voters[index];
                ensure!(
                    proposers[index]
                        && count >= 2
                        && threshold > 0
                        && threshold <= count
                        && u64::from(threshold) * 2 > u64::from(count),
                    Error::<T>::InvalidThreshold
                );
            }
        }
        Ok(())
    }

    /// 暂存目标后复用同一最终校验；调用者必须持有存储事务，失败全部回滚。
    fn apply_policy(cid: &CidNumberOf<T>, action: &InstitutionPolicyAction) -> DispatchResult {
        ensure!(
            action.encoded_size() <= <T as engingcore::Config>::MaxProposalDataLen::get() as usize,
            Error::<T>::TooManyGovernanceChanges
        );
        ensure!(
            !action.permission_changes.is_empty() || action.threshold.is_some(),
            Error::<T>::GovernanceResultEmpty
        );
        ensure!(
            action.permission_changes.len() <= MAX_POLICY_ROLES as usize,
            Error::<T>::TooManyGovernanceChanges
        );
        ensure!(
            InstitutionGovernanceThresholds::<T>::contains_key(cid),
            Error::<T>::MissingInitialConfiguration
        );
        let mut seen = BTreeSet::new();
        for change in &action.permission_changes {
            let role: RoleCodeOf = change
                .role_code
                .clone()
                .try_into()
                .map_err(|_| Error::<T>::InvalidRoleCode)?;
            Self::ensure_policy_role(cid, &role)?;
            ensure!(
                seen.insert(role.clone()),
                Error::<T>::DuplicateGovernanceRoleChange
            );
            ensure!(
                !change.permissions.is_empty(),
                Error::<T>::RolePermissionsEmpty
            );
            ensure!(
                change.permissions.len() <= MAX_POLICY_PERMISSIONS as usize,
                Error::<T>::TooManyRolePermissions
            );
            let mut unique = BTreeSet::new();
            let mut permissions = Vec::with_capacity(change.permissions.len());
            for spec in &change.permissions {
                ensure!(
                    !spec.business_action_id.module_tag.is_empty(),
                    Error::<T>::InvalidRolePermission
                );
                ensure!(
                    T::InstitutionCapabilityPolicy::allows(
                        cid,
                        &spec.business_action_id,
                        spec.operation
                    ),
                    Error::<T>::InstitutionCapabilityDenied
                );
                ensure!(
                    unique.insert((
                        spec.business_action_id.module_tag.clone(),
                        spec.business_action_id.action_code,
                        spec.operation as u8
                    )),
                    Error::<T>::DuplicateRolePermission
                );
                let module_tag: ModuleTagOf = spec
                    .business_action_id
                    .module_tag
                    .clone()
                    .try_into()
                    .map_err(|_| Error::<T>::InvalidRolePermission)?;
                permissions.push(RoleBusinessPermission {
                    role_subject: RoleSubject {
                        cid_number: cid.clone(),
                        role_code: role.clone(),
                    },
                    business_action_id: entitybase::BusinessActionId {
                        module_tag,
                        action_code: spec.business_action_id.action_code,
                    },
                    operation: spec.operation,
                });
            }
            let permissions: RolePermissionsOf<T> = permissions
                .try_into()
                .map_err(|_| Error::<T>::TooManyRolePermissions)?;
            InstitutionRolePermissions::<T>::insert(cid, role, permissions);
        }
        if let Some(threshold) = action.threshold {
            InstitutionGovernanceThresholds::<T>::insert(cid, threshold);
        }
        Self::ensure_configuration(cid)
    }

    /// 建案校验目标时始终回滚暂存，真正投票仍冻结原任职、原权限和原阈值。
    #[frame_support::transactional]
    pub(crate) fn do_propose_institution_policy(
        who: T::AccountId,
        cid: CidNumberOf<T>,
        action: InstitutionPolicyAction,
        proposer_role: RoleCodeOf,
        expected_state_hash: [u8; 32],
    ) -> DispatchResult {
        let info = Institutions::<T>::get(&cid).ok_or(Error::<T>::InstitutionNotFound)?;
        ensure!(
            is_ordinary_institution(info.institution_code, &cid),
            Error::<T>::InstitutionPolicyDenied
        );
        ensure!(
            Self::registration_state_hash(&cid)? == expected_state_hash,
            Error::<T>::StalePolicyState
        );
        frame_support::storage::with_transaction(|| {
            frame_support::storage::TransactionOutcome::Rollback(Self::apply_policy(&cid, &action))
        })?;
        let proposal = InstitutionPolicyProposal {
            institution_code: info.institution_code,
            cid_number: cid.to_vec(),
            action,
            expected_state_hash,
        };
        let mut data = Vec::from(crate::MODULE_TAG);
        data.push(ACTION_POLICY);
        data.extend_from_slice(&proposal.encode());
        let plan = Self::build_institution_vote_plan(
            &who,
            &cid,
            &proposer_role,
            ACTION_INSTITUTION_GOVERNANCE,
            &data,
        )?;
        let proposal_id = T::InternalVoteEngine::create_institution_proposal_with_data(
            who.clone(),
            info.institution_code,
            cid.to_vec(),
            None,
            Vec::new(),
            plan,
            data,
        )?;
        Self::deposit_event(Event::<T>::InstitutionPolicyProposed {
            proposal_id,
            cid_number: cid,
            proposer_account_id: who,
        });
        Ok(())
    }

    /// PASSED仅为排队；本入口成功提交后引擎才推进EXECUTED。
    pub(crate) fn execute_policy_proposal(
        proposal_id: u128,
        proposal: InstitutionPolicyProposal,
    ) -> DispatchResult {
        ensure!(
            engingcore::Pallet::<T>::is_callback_execution_scope(proposal_id)
                && engingcore::Pallet::<T>::is_proposal_owner(proposal_id, crate::MODULE_TAG),
            Error::<T>::PermissionDenied
        );
        let stored = engingcore::Pallet::<T>::proposals(proposal_id)
            .ok_or(Error::<T>::ProposalActionNotFound)?;
        let cid: CidNumberOf<T> = proposal
            .cid_number
            .try_into()
            .map_err(|_| Error::<T>::InvalidCidNumber)?;
        ensure!(
            stored.kind == engingcore::PROPOSAL_KIND_INTERNAL
                && stored.stage == engingcore::STAGE_INTERNAL
                && stored.status == engingcore::STATUS_PASSED
                && stored.internal_code == Some(proposal.institution_code)
                && stored
                    .actor_cid_number
                    .as_ref()
                    .is_some_and(|actor| actor.as_slice() == cid.as_slice())
                && stored.execution_account_id.is_none()
                && stored.subject_cid_numbers.is_empty(),
            Error::<T>::PermissionDenied
        );
        let info = Institutions::<T>::get(&cid).ok_or(Error::<T>::InstitutionNotFound)?;
        ensure!(
            info.institution_code == proposal.institution_code
                && is_ordinary_institution(info.institution_code, &cid),
            Error::<T>::InstitutionPolicyDenied
        );
        ensure!(
            Self::registration_state_hash(&cid)? == proposal.expected_state_hash,
            Error::<T>::StalePolicyState
        );
        Self::apply_policy(&cid, &proposal.action)?;
        Self::deposit_event(Event::<T>::InstitutionPolicyApplied {
            proposal_id,
            cid_number: cid.clone(),
            permission_changes: proposal.action.permission_changes.len() as u32,
            threshold: InstitutionGovernanceThresholds::<T>::get(&cid)
                .ok_or(Error::<T>::MissingInitialConfiguration)?,
        });
        Ok(())
    }
}
