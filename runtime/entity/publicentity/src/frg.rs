//! 联邦注册局管理员更换：建案和执行均限定为一个省专员岗位。

use crate::*;
use primitives::{
    cid::code::FRG, count_const::FRG_PROVINCE_GROUP_ADMIN_COUNT,
    governance_skeleton::fixed_role_seats_by_identity,
};

fn is_province_role(cid: &[u8], role: &[u8]) -> bool {
    fixed_role_seats_by_identity(FRG, cid, role) == Some(FRG_PROVINCE_GROUP_ADMIN_COUNT)
}

/// 钱包更换和纯人员任职分别校验，均限定本省五席岗位；旧完整名册动作不接纳。
pub(crate) fn handles<T: Config>(
    code: InstitutionCode,
    cid: &CidNumberOf<T>,
    action: &InstitutionGovernanceActionOf<T>,
) -> bool {
    code == FRG
        && match action {
            InstitutionGovernanceAction::RegisterAdminWalletChanges { .. } => false,
            InstitutionGovernanceAction::RotateAdminWallet { .. } => true,
            InstitutionGovernanceAction::ReplaceAdmins { .. }
            | InstitutionGovernanceAction::ReplaceAdminsAndMutateRoles { .. } => false,
            InstitutionGovernanceAction::MutateRolesAndAssignments {
                assignment_changes, ..
            } => assignment_changes
                .iter()
                .any(|change| is_province_role(cid, &change.role_code)),
        }
}

/// 人员治理只接受本省一个岗位的纯任职载荷，绝不写管理员钱包。
fn validate_personnel<T: Config>(
    cid: &CidNumberOf<T>,
    role: &RoleCodeOf,
    action: &InstitutionGovernanceActionOf<T>,
) -> DispatchResult {
    ensure!(is_province_role(cid, role), Error::<T>::PermissionDenied);
    let InstitutionGovernanceAction::MutateRolesAndAssignments {
        role_mutations,
        assignment_changes,
        legal_representative_change,
    } = action
    else {
        return Err(Error::<T>::InvalidInstitutionGovernanceAction.into());
    };
    let [change] = assignment_changes.as_slice() else {
        return Err(Error::<T>::InvalidInstitutionGovernanceAction.into());
    };
    ensure!(
        role_mutations.is_empty()
            && legal_representative_change.is_none()
            && change.role_code.as_slice() == role.as_slice(),
        Error::<T>::PermissionDenied
    );
    let seats = FRG_PROVINCE_GROUP_ADMIN_COUNT as usize;
    let current = InstitutionRoleAssignments::<T>::get(cid, role);
    let target: BTreeSet<_> = change
        .assignments
        .iter()
        .map(|a| a.account_id.clone())
        .collect();
    ensure!(
        current.len() == seats && target.len() == seats && change.assignments.len() == seats,
        Error::<T>::FixedRoleSeatsMismatch
    );
    ensure!(
        current
            .iter()
            .map(|a| a.account_id.clone())
            .collect::<BTreeSet<_>>()
            != target,
        Error::<T>::GovernanceResultEmpty
    );
    for a in &change.assignments {
        ensure!(
            a.assignment_source == InstitutionAssignmentSource::InstitutionGovernance
                && a.assignment_status == InstitutionAssignmentStatus::Active,
            Error::<T>::InvalidAssignmentSource
        );
        ensure!(
            a.term_start == 0 && a.term_end == 0,
            Error::<T>::InvalidAssignmentTerm
        );
    }
    // 不把其他省的任职人挪入本省；人员引用与钱包地址分开比较。
    for (other, members) in InstitutionRoleAssignments::<T>::iter_prefix(cid) {
        if other != *role && is_province_role(cid, &other) {
            ensure!(
                !members.iter().any(|a| target.contains(&a.account_id)),
                Error::<T>::PermissionDenied
            );
        }
    }
    Ok(())
}

pub(crate) fn vote_plan<T: Config>(
    who: &T::AccountId,
    cid: &CidNumberOf<T>,
    role: &RoleCodeOf,
    action: &InstitutionGovernanceActionOf<T>,
    data: &[u8],
) -> Result<VotePlanOf<T::AccountId>, sp_runtime::DispatchError> {
    if matches!(
        action,
        InstitutionGovernanceAction::RotateAdminWallet { .. }
    ) {
        ensure!(
            wallet_role::<T>(cid, action)?.as_slice() == role.as_slice(),
            Error::<T>::PermissionDenied
        );
    } else {
        validate_personnel::<T>(cid, role, action)?;
    }
    let action_code = entitybase::business_action::ACTION_INSTITUTION_GOVERNANCE;
    let permission = BusinessActionId {
        module_tag: MODULE_TAG.to_vec(),
        action_code,
    };
    let subject = entitybase::RoleSubject {
        cid_number: cid.to_vec(),
        role_code: role.to_vec(),
    };
    ensure!(
        Pallet::<T>::is_authorized(who, &subject, &permission, RolePermissionOperation::Propose)
            && Pallet::<T>::role_has_permission(
                &subject,
                &permission,
                RolePermissionOperation::Vote
            ),
        Error::<T>::PermissionDenied
    );
    let owner: ModuleTagOf = MODULE_TAG
        .to_vec()
        .try_into()
        .map_err(|_| engingcore::Error::<T>::InvalidVotePlan)?;
    let subject = AuthorizationSubject::Institution(RoleSubject {
        cid_number: CidNumber::try_from(subject.cid_number)
            .map_err(|_| engingcore::Error::<T>::InvalidVotePlan)?,
        role_code: role.clone(),
    });
    VotePlanOf::<T::AccountId>::try_new(
        BusinessActionId {
            module_tag: owner.clone(),
            action_code,
        },
        owner,
        subject.clone(),
        Vec::from([subject]),
        VotingEngineKind::Internal,
        sp_io::hashing::blake2_256(data),
    )
    .map_err(|_| engingcore::Error::<T>::InvalidVotePlan.into())
}

pub(crate) fn execute<T: Config>(
    proposal_id: u128,
    cid: &CidNumberOf<T>,
    action: &InstitutionGovernanceActionOf<T>,
) -> DispatchResult {
    ensure!(
        engingcore::Pallet::<T>::is_callback_execution_scope(proposal_id),
        Error::<T>::PermissionDenied
    );
    let plan = engingcore::Pallet::<T>::proposal_vote_plan(proposal_id)
        .ok_or(engingcore::Error::<T>::InvalidVotePlan)?;
    let AuthorizationSubject::Institution(ref subject) = plan.proposer_subject else {
        return Err(engingcore::Error::<T>::InvalidVotePlan.into());
    };
    ensure!(
        subject.cid_number.as_slice() == cid.as_slice()
            && plan.voter_subjects.as_slice() == core::slice::from_ref(&plan.proposer_subject),
        engingcore::Error::<T>::InvalidVotePlan
    );
    if let InstitutionGovernanceAction::RotateAdminWallet {
        change,
        expires_at,
        new_wallet_proof,
    } = action
    {
        ensure!(
            wallet_role::<T>(cid, action)?.as_slice() == subject.role_code.as_slice(),
            Error::<T>::PermissionDenied
        );
        ensure!(
            engingcore::Pallet::<T>::frozen_voter_identity(
                proposal_id,
                &plan.proposer_subject,
                &change.expected_wallet
            ) == Some((change.expected_wallet.clone(), change.expected_revision)),
            Error::<T>::PermissionDenied
        );
        Pallet::<T>::check_wallet_acceptance(
            cid,
            change,
            adminbase::InstitutionWalletChangeMode::Governance,
            *expires_at,
            new_wallet_proof,
        )?;
        return Pallet::<T>::apply_wallet_change(FRG, cid, change.clone());
    }
    validate_personnel::<T>(cid, &subject.role_code, action)?;
    let InstitutionGovernanceAction::MutateRolesAndAssignments {
        role_mutations,
        assignment_changes,
        legal_representative_change,
    } = action
    else {
        return Err(Error::<T>::InvalidInstitutionGovernanceAction.into());
    };
    Pallet::<T>::apply_institution_governance_result(entitybase::InstitutionGovernanceResult {
        institution_code: FRG,
        cid_number: cid.to_vec(),
        proposal_id,
        role_mutations: role_mutations.clone(),
        assignment_changes: assignment_changes.clone(),
        legal_representative_change: legal_representative_change.clone(),
        result_source_ref: proposal_id.to_le_bytes().to_vec(),
    })
}

fn wallet_role<T: Config>(
    cid: &CidNumberOf<T>,
    action: &InstitutionGovernanceActionOf<T>,
) -> Result<RoleCodeOf, sp_runtime::DispatchError> {
    let InstitutionGovernanceAction::RotateAdminWallet { change, .. } = action else {
        return Err(Error::<T>::PermissionDenied.into());
    };
    let reference =
        T::InstitutionAdminQuery::resolve_role_authorization(FRG, cid, &change.admin_account)
            .ok_or(Error::<T>::PermissionDenied)?;
    let mut found = None;
    for (role, definition) in InstitutionRoles::<T>::iter_prefix(cid) {
        if is_province_role(cid, &role)
            && definition.role_status == InstitutionRoleStatus::Active
            && InstitutionRoleAssignments::<T>::get(cid, &role)
                .iter()
                .any(|a| {
                    a.account_id == reference
                        && Pallet::<T>::is_assignment_effective(&definition, a)
                })
        {
            ensure!(found.is_none(), Error::<T>::PermissionDenied);
            found = Some(role);
        }
    }
    found.ok_or_else(|| Error::<T>::PermissionDenied.into())
}

/// 注册局钱包维护只核对目标原省组，不写入省专员人员任职。
pub(crate) fn ensure_registry_wallet_scope<T: Config>(
    cid: &CidNumberOf<T>,
    actor_role: &[u8],
    changes: &[adminbase::InstitutionAdminWalletChange<T::AccountId>],
) -> DispatchResult {
    if Institutions::<T>::get(cid).is_some_and(|i| i.institution_code == FRG) {
        ensure!(
            changes.len() <= FRG_PROVINCE_GROUP_ADMIN_COUNT as usize,
            Error::<T>::PermissionDenied
        );
        for change in changes {
            let action = InstitutionGovernanceAction::RotateAdminWallet {
                change: change.clone(),
                expires_at: 0,
                new_wallet_proof: Default::default(),
            };
            ensure!(
                wallet_role::<T>(cid, &action)?.as_slice() == actor_role,
                Error::<T>::PermissionDenied
            );
        }
    }
    Ok(())
}
