//! 联邦注册局管理员更换：建案和执行均限定为一个省专员岗位。

use crate::*;
use entitybase::InstitutionRoleQuery;
use primitives::{
    cid::code::FRG, count_const::FRG_PROVINCE_GROUP_ADMIN_COUNT,
    governance_skeleton::fixed_role_seats_by_identity,
};

fn is_province_role(cid: &[u8], role: &[u8]) -> bool {
    fixed_role_seats_by_identity(FRG, cid, role) == Some(FRG_PROVINCE_GROUP_ADMIN_COUNT)
}

/// 名册变更和省专员任职变更都必须进入同一个更换流程。
pub(crate) fn handles<T: Config>(
    code: InstitutionCode,
    cid: &CidNumberOf<T>,
    action: &InstitutionGovernanceActionOf<T>,
) -> bool {
    code == FRG
        && match action {
            InstitutionGovernanceAction::ReplaceAdmins { .. }
            | InstitutionGovernanceAction::ReplaceAdminsAndMutateRoles { .. } => true,
            InstitutionGovernanceAction::MutateRolesAndAssignments {
                assignment_changes, ..
            } => assignment_changes
                .iter()
                .any(|change| is_province_role(cid, &change.role_code)),
        }
}

struct Change<'a, AccountId> {
    admins: &'a [Admin<AccountId>],
    current: BTreeSet<AccountId>,
    target: BTreeSet<AccountId>,
}

fn validate<'a, T: Config>(
    cid: &CidNumberOf<T>,
    role: &RoleCodeOf,
    action: &'a InstitutionGovernanceActionOf<T>,
) -> Result<Change<'a, T::AccountId>, sp_runtime::DispatchError> {
    ensure!(is_province_role(cid, role), Error::<T>::PermissionDenied);
    let InstitutionGovernanceAction::ReplaceAdminsAndMutateRoles {
        admins,
        role_mutations,
        assignment_changes,
        legal_representative_change,
    } = action
    else {
        return Err(Error::<T>::InvalidInstitutionGovernanceAction.into());
    };
    let [assignment] = assignment_changes.as_slice() else {
        return Err(Error::<T>::InvalidInstitutionGovernanceAction.into());
    };
    ensure!(
        role_mutations.is_empty() && legal_representative_change.is_none(),
        Error::<T>::InvalidInstitutionGovernanceAction
    );
    ensure!(
        assignment.role_code.as_slice() == role.as_slice(),
        Error::<T>::PermissionDenied
    );

    let seats = FRG_PROVINCE_GROUP_ADMIN_COUNT as usize;
    let current: BTreeSet<_> =
        <Pallet<T> as InstitutionRoleQuery<T::AccountId>>::active_accounts_for_role(cid, role)
            .into_iter()
            .collect();
    let target: BTreeSet<_> = assignment
        .assignments
        .iter()
        .map(|member| member.account_id.clone())
        .collect();
    ensure!(
        current.len() == seats
            && InstitutionRoleAssignments::<T>::get(cid, role).len() == seats
            && target.len() == seats
            && assignment.assignments.len() == seats,
        Error::<T>::FixedRoleSeatsMismatch
    );
    ensure!(current != target, Error::<T>::GovernanceResultEmpty);
    for member in &assignment.assignments {
        ensure!(
            member.assignment_source == InstitutionAssignmentSource::InstitutionGovernance
                && member.assignment_status == InstitutionAssignmentStatus::Active,
            Error::<T>::InvalidAssignmentSource
        );
        ensure!(
            member.term_start == 0 && member.term_end == 0,
            Error::<T>::InvalidAssignmentTerm
        );
    }

    let mut expected: BTreeSet<_> = T::InstitutionAdminQuery::institution_admins(FRG, cid)
        .ok_or(Error::<T>::InvalidAssignmentResultAdmins)?
        .into_iter()
        .collect();
    let expected_len = adminbase::expected_fixed_governance_admins_len(FRG, cid)
        .ok_or(Error::<T>::InvalidInstitutionCode)? as usize;
    ensure!(
        expected.len() == expected_len && admins.len() == expected_len,
        Error::<T>::InvalidAdminsLen
    );
    for account in current.difference(&target) {
        expected.remove(account);
    }
    for account in target.difference(&current) {
        // 不能把其他省组已在册的管理员划入本省。
        ensure!(
            expected.insert(account.clone()),
            Error::<T>::InvalidAssignmentResultAdmins
        );
    }
    let submitted: BTreeSet<_> = admins
        .iter()
        .map(|admin| admin.account_id.clone())
        .collect();
    ensure!(
        submitted == expected,
        Error::<T>::InvalidAssignmentResultAdmins
    );

    Ok(Change {
        admins,
        current,
        target,
    })
}

pub(crate) fn vote_plan<T: Config>(
    who: &T::AccountId,
    cid: &CidNumberOf<T>,
    role: &RoleCodeOf,
    action: &InstitutionGovernanceActionOf<T>,
    data: &[u8],
) -> Result<VotePlanOf<T::AccountId>, sp_runtime::DispatchError> {
    validate::<T>(cid, role, action)?;
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
        .map_err(|_| votebase::Error::<T>::InvalidVotePlan)?;
    let subject = AuthorizationSubject::Institution(RoleSubject {
        cid_number: CidNumber::try_from(subject.cid_number)
            .map_err(|_| votebase::Error::<T>::InvalidVotePlan)?,
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
    .map_err(|_| votebase::Error::<T>::InvalidVotePlan.into())
}

pub(crate) fn execute<T: Config>(
    proposal_id: u64,
    cid: &CidNumberOf<T>,
    action: &InstitutionGovernanceActionOf<T>,
) -> DispatchResult {
    ensure!(
        votebase::Pallet::<T>::is_callback_execution_scope(proposal_id),
        Error::<T>::PermissionDenied
    );
    let plan = votebase::Pallet::<T>::proposal_vote_plan(proposal_id)
        .ok_or(votebase::Error::<T>::InvalidVotePlan)?;
    let AuthorizationSubject::Institution(ref subject) = plan.proposer_subject else {
        return Err(votebase::Error::<T>::InvalidVotePlan.into());
    };
    ensure!(
        subject.cid_number.as_slice() == cid.as_slice()
            && plan.voter_subjects.as_slice() == core::slice::from_ref(&plan.proposer_subject),
        votebase::Error::<T>::InvalidVotePlan
    );
    let change = validate::<T>(cid, &subject.role_code, action)?;
    let snapshot = votebase::VoterSnapshot::<T>::get(proposal_id, &plan.proposer_subject)
        .ok_or(votebase::Error::<T>::MissingVoterSnapshot)?;
    ensure!(
        snapshot.into_iter().collect::<BTreeSet<_>>() == change.current,
        Error::<T>::InvalidAssignmentResultAdmins
    );

    let mut assignments = InstitutionRoleAssignments::<T>::get(cid, &subject.role_code);
    let removed: BTreeSet<_> = change.current.difference(&change.target).collect();
    // 保留其他岗位的任职完整性，不能移除同时在其他岗位任职的人。
    for (role, members) in InstitutionRoleAssignments::<T>::iter_prefix(cid) {
        if role != subject.role_code {
            ensure!(
                !members
                    .iter()
                    .any(|member| removed.contains(&member.account_id)),
                Error::<T>::InvalidAssignmentResultAdmins
            );
        }
    }
    let mut incoming = change.target.difference(&change.current);
    let source_ref: institution::role::AssignmentSourceRefOf = proposal_id
        .to_le_bytes()
        .to_vec()
        .try_into()
        .map_err(|_| Error::<T>::InvalidAssignmentSource)?;
    for member in assignments.iter_mut() {
        if removed.contains(&member.account_id) {
            member.account_id = incoming
                .next()
                .ok_or(Error::<T>::InvalidAssignmentResultAdmins)?
                .clone();
            member.assignment_source = InstitutionAssignmentSource::InstitutionGovernance;
            member.assignment_source_ref = source_ref.clone();
        }
    }
    ensure!(
        incoming.next().is_none(),
        Error::<T>::InvalidAssignmentResultAdmins
    );

    // 调用方的现有执行事务涵盖名册与任职两次写入。
    T::AdminLifecycle::set_institution_admins(
        MODULE_TAG,
        cid.to_vec(),
        FRG,
        AdminAccountKind::PublicInstitution,
        change.admins.to_vec(),
    )?;
    InstitutionRoleAssignments::<T>::insert(cid, &subject.role_code, assignments);
    Pallet::<T>::deposit_event(Event::<T>::InstitutionGovernanceApplied {
        cid_number: cid.clone(),
        role_mutations: 0,
        assignment_changes: 1,
        admins_len: change.admins.len() as u32,
        legal_representative_updated: false,
        result_source_ref: source_ref,
    });
    Ok(())
}
