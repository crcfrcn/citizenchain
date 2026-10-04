//! 代表机构表决机制。

pub mod sequential;
pub mod single;
pub mod tally;
use crate::*;
use votingengine::InstitutionRoleProvider as _;

impl<T: Config> Pallet<T> {
    pub(crate) fn stage_duration() -> frame_system::pallet_prelude::BlockNumberFor<T> {
        use sp_runtime::traits::SaturatedConversion;
        (VOTING_DURATION_BLOCKS as u64).saturated_into()
    }

    /// 机构码只允许由 CID 解析，调用载荷不得另带一份可冲突的身份字段。
    pub(crate) fn institution_code_for_cid(
        cid_number: &votingengine::types::CidNumber,
    ) -> Result<InstitutionCode, DispatchError> {
        let text = core::str::from_utf8(cid_number.as_slice())
            .map_err(|_| Error::<T>::InvalidInstitutionCid)?;
        votingengine::types::institution_code_from_cid_number(text)
            .ok_or_else(|| Error::<T>::InvalidInstitutionCid.into())
    }

    /// 特别案人口作用域只由发起机构 CID 推导，调用载荷不得再携带第二份 scope。
    pub(crate) fn population_scope_for_actor(
        actor_code: InstitutionCode,
        actor_cid_number: &votingengine::types::CidNumber,
    ) -> Result<PopulationScope, DispatchError> {
        match actor_code {
            code if code == *b"NRP\0" || code == *b"NED\0" => Ok(PopulationScope::Country),
            code if code == *b"PRP\0" => {
                let (province_code, _) =
                    primitives::cid::number::cid_scope_codes(actor_cid_number.as_slice())
                        .map_err(|_| Error::<T>::InvalidInstitutionCid)?;
                let province_code = province_code
                    .to_vec()
                    .try_into()
                    .map_err(|_| Error::<T>::InvalidInstitutionCid)?;
                Ok(PopulationScope::Province(province_code))
            }
            code if code == *b"CSLF" || code == *b"CEDU" || code == *b"CLEG" => {
                let (province_code, city_code) =
                    primitives::cid::number::cid_scope_codes(actor_cid_number.as_slice())
                        .map_err(|_| Error::<T>::InvalidInstitutionCid)?;
                let province_code = province_code
                    .to_vec()
                    .try_into()
                    .map_err(|_| Error::<T>::InvalidInstitutionCid)?;
                let city_code = city_code
                    .to_vec()
                    .try_into()
                    .map_err(|_| Error::<T>::InvalidInstitutionCid)?;
                Ok(PopulationScope::City(province_code, city_code))
            }
            _ => Err(Error::<T>::InvalidInstitutionCid.into()),
        }
    }

    pub(crate) fn resolve_subject_cid_numbers(
        actor_cid_number: &votingengine::types::CidNumber,
        route: &RepresentativeRoute,
        additional_subjects: ProposalSubjectCidNumbers,
        additional_institutions: &[crate::types::RepresentativeBody],
    ) -> Result<ProposalSubjectCidNumbers, DispatchError> {
        let mut raw: sp_runtime::sp_std::vec::Vec<sp_runtime::sp_std::vec::Vec<u8>> =
            additional_subjects
                .into_iter()
                .map(|cid| cid.into_inner())
                .collect();
        raw.push(actor_cid_number.to_vec());
        for body in route.bodies() {
            raw.push(body.cid_number.to_vec());
        }
        for subject in additional_institutions {
            raw.push(subject.cid_number.to_vec());
        }
        <votingengine::Pallet<T>>::bound_subject_cid_numbers(raw)
    }

    /// 校验路线并返回首个表决机构。路线中的机构不得重复。
    pub(crate) fn validate_representative_route(
        route: &RepresentativeRoute,
    ) -> Result<(InstitutionCode, crate::types::RepresentativeBody), DispatchError> {
        let bodies = route.bodies();
        ensure!(!bodies.is_empty(), Error::<T>::InvalidRepresentativeRoute);
        match route {
            RepresentativeRoute::Single(_) => {}
            RepresentativeRoute::Sequential(sequence) => ensure!(
                sequence.len() >= 2 && sequence.len() <= MAX_REPRESENTATIVE_BODIES as usize,
                Error::<T>::InvalidRepresentativeRoute
            ),
        }
        for (index, body) in bodies.iter().enumerate() {
            ensure!(
                !bodies[..index].iter().any(|existing| existing == body),
                Error::<T>::InvalidRepresentativeRoute
            );
        }
        for body in &bodies {
            Self::institution_code_for_cid(&body.cid_number)?;
        }
        let first_body = bodies[0].clone();
        Ok((
            Self::institution_code_for_cid(&first_body.cid_number)?,
            first_body,
        ))
    }
}

impl<T: Config> Pallet<T> {
    /// 创建通用代表机构表决提案。业务模块只提供路线、门槛、受影响主体和 owner 数据。
    #[allow(clippy::too_many_arguments)]
    pub fn do_create_representative_proposal(
        who: T::AccountId,
        actor_cid_number: votingengine::types::CidNumber,
        vote_plan: VotePlanOf<T::AccountId>,
        route: RepresentativeRoute,
        rule: RepresentativeVoteRule,
        procedure: VoteProcedure,
        additional_subjects: ProposalSubjectCidNumbers,
        legislation_meta: Option<pallet::LegislationMeta>,
    ) -> Result<u64, DispatchError> {
        let (first_code, _first_body) = Self::validate_representative_route(&route)?;
        let actor_code = Self::institution_code_for_cid(&actor_cid_number)?;
        ensure!(
            vote_plan.voting_engine == votingengine::VotingEngineKind::Legislation,
            votingengine::Error::<T>::InvalidVotePlan
        );
        let proposer_role = match &vote_plan.proposer_subject {
            AuthorizationSubject::Institution(subject) => subject,
            AuthorizationSubject::PersonalMultisig(_) => {
                return Err(votingengine::Error::<T>::InvalidVotePlan.into())
            }
        };
        ensure!(
            proposer_role.cid_number == actor_cid_number
                && T::InstitutionRoleProvider::is_active_assignment(
                    actor_cid_number.as_slice(),
                    &who,
                    proposer_role.role_code.as_slice(),
                ),
            votingengine::Error::<T>::NoPermission
        );
        ensure!(
            !(procedure == VoteProcedure::RepresentativeOnly
                && rule == RepresentativeVoteRule::Special),
            Error::<T>::InvalidRepresentativeRule
        );
        ensure!(
            (procedure == VoteProcedure::Legislation) == legislation_meta.is_some(),
            Error::<T>::ProposalMetaMissing
        );

        let mut additional_institutions = sp_runtime::sp_std::vec::Vec::new();
        if let Some(meta) = legislation_meta.as_ref() {
            if let Some(executive) = meta.executive.as_ref() {
                additional_institutions.push(executive.clone());
            }
            additional_institutions.extend(meta.override_signers.iter().cloned());
            additional_institutions.extend(meta.guard.iter().cloned());
        }
        let mut expected_voters = route
            .bodies()
            .into_iter()
            .map(AuthorizationSubject::Institution)
            .collect::<sp_runtime::sp_std::vec::Vec<_>>();
        expected_voters.extend(
            additional_institutions
                .iter()
                .cloned()
                .map(AuthorizationSubject::Institution),
        );
        for (index, subject) in expected_voters.iter().enumerate() {
            ensure!(
                !expected_voters[..index].contains(subject),
                votingengine::Error::<T>::InvalidVotePlan
            );
        }
        ensure!(
            expected_voters.len() == vote_plan.voter_subjects.len()
                && expected_voters
                    .iter()
                    .all(|subject| vote_plan.voter_subjects.contains(subject)),
            votingengine::Error::<T>::InvalidVotePlan
        );
        let subject_cid_numbers = Self::resolve_subject_cid_numbers(
            &actor_cid_number,
            &route,
            additional_subjects,
            &additional_institutions,
        )?;

        let population_scope = if rule == RepresentativeVoteRule::Special {
            Some(Self::population_scope_for_actor(
                actor_code,
                &actor_cid_number,
            )?)
        } else {
            None
        };
        let now = <frame_system::Pallet<T>>::block_number();
        let end = now.saturating_add(Self::stage_duration());

        with_transaction(|| {
            let proposal = Proposal {
                kind: PROPOSAL_KIND_LEGISLATION,
                stage: STAGE_LEG_REPRESENTATIVE,
                status: STATUS_VOTING,
                internal_code: Some(first_code),
                actor_cid_number: Some(actor_cid_number),
                execution_account_id: None,
                subject_cid_numbers,
                start: now,
                end,
            };
            let id = match <votingengine::Pallet<T>>::allocate_proposal_id() {
                Ok(id) => id,
                Err(err) => return TransactionOutcome::Rollback(Err(err)),
            };
            if let Err(err) =
                votingengine::limit::try_add_active_proposals::<T>(proposal.subject_keys(), id)
            {
                return TransactionOutcome::Rollback(Err(err));
            }
            // 立法提案可能关联多机构,互斥锁以所有关联 CID 为主体占用。
            for subject in proposal.subject_keys() {
                if let Err(err) = <votingengine::Pallet<T>>::acquire_internal_proposal_mutex(
                    id,
                    subject,
                    InternalProposalMutexKind::Regular,
                ) {
                    return TransactionOutcome::Rollback(Err(err));
                }
            }
            // 所有代表、签署和护宪岗位都在建案时冻结；后续换届不得改写既有资格。
            for subject in vote_plan.voter_subjects.iter() {
                let role_subject = match subject {
                    AuthorizationSubject::Institution(role_subject) => role_subject,
                    AuthorizationSubject::PersonalMultisig(_) => {
                        return TransactionOutcome::Rollback(Err(
                            votingengine::Error::<T>::InvalidVotePlan.into(),
                        ))
                    }
                };
                let voters = T::InstitutionRoleProvider::active_accounts_for_role(
                    role_subject.cid_number.as_slice(),
                    role_subject.role_code.as_slice(),
                );
                if legislation_meta
                    .as_ref()
                    .and_then(|meta| meta.guard.as_ref())
                    == Some(role_subject)
                {
                    // 护宪终审是固定七人制；人数错误或账户重复必须在建案冻结资格时失败。
                    let unique = voters
                        .iter()
                        .enumerate()
                        .all(|(index, voter)| !voters[..index].contains(voter));
                    if voters.len() != CONSTITUTION_GUARD_MEMBERS as usize || !unique {
                        return TransactionOutcome::Rollback(Err(
                            Error::<T>::InvalidGuardMembersLen.into(),
                        ));
                    }
                }
                if let Err(err) =
                    <votingengine::Pallet<T>>::snapshot_role_voters(id, subject.clone(), voters)
                {
                    return TransactionOutcome::Rollback(Err(err));
                }
            }
            pallet::RepresentativeMetas::<T>::insert(
                id,
                pallet::RepresentativeMeta {
                    route,
                    current_body: 0,
                    rule,
                    procedure,
                },
            );
            if let Some(meta) = legislation_meta {
                pallet::LegislationMetas::<T>::insert(id, meta);
            }
            Proposals::<T>::insert(id, proposal);
            if let Err(err) = <votingengine::Pallet<T>>::bind_vote_plan(id, vote_plan) {
                return TransactionOutcome::Rollback(Err(err));
            }
            // 只有特别案创建人口快照。投票引擎读取 citizen-identity 人口数据后
            // 在本 proposal_id 下生成快照，后续失败由外层事务整体回滚。
            if let Some(scope) = population_scope.as_ref() {
                match <votingengine::Pallet<T>>::create_population_snapshot(id, scope) {
                    Ok(0) => {
                        return TransactionOutcome::Rollback(Err(
                            Error::<T>::CitizenEligibleTotalNotSet.into(),
                        ))
                    }
                    Ok(_) => {}
                    Err(err) => return TransactionOutcome::Rollback(Err(err)),
                }
            }
            if let Err(err) = <votingengine::Pallet<T>>::schedule_proposal_expiry(id, end) {
                return TransactionOutcome::Rollback(Err(err));
            }
            <votingengine::Pallet<T>>::emit_proposal_created(
                id,
                PROPOSAL_KIND_LEGISLATION,
                STAGE_LEG_REPRESENTATIVE,
                end,
            );
            Self::deposit_event(pallet::Event::<T>::RepresentativeProposalCreated {
                proposal_id: id,
                rule,
                bodies: pallet::RepresentativeMetas::<T>::get(id)
                    .map(|meta| meta.route.len() as u32)
                    .unwrap_or_default(),
                procedure,
            });
            TransactionOutcome::Commit(Ok(id))
        })
    }

    /// 对当前代表机构投票；同一账户可在不同机构阶段分别依法投票。
    pub fn do_cast_representative_vote(
        who: T::AccountId,
        proposal_id: u64,
        voter_role_code: votingengine::types::RoleCode,
        approve: bool,
    ) -> DispatchResult {
        let proposal = <votingengine::Pallet<T>>::ensure_open_proposal(proposal_id)?;
        ensure!(
            proposal.kind == PROPOSAL_KIND_LEGISLATION,
            votingengine::Error::<T>::InvalidProposalKind
        );
        ensure!(
            proposal.stage == STAGE_LEG_REPRESENTATIVE,
            votingengine::Error::<T>::InvalidProposalStage
        );
        let meta = pallet::RepresentativeMetas::<T>::get(proposal_id)
            .ok_or(Error::<T>::ProposalMetaMissing)?;
        let representative_body = meta
            .route
            .body(meta.current_body)
            .ok_or(Error::<T>::InvalidRepresentativeRoute)?;
        ensure!(
            representative_body.role_code == voter_role_code,
            votingengine::Error::<T>::NoPermission
        );
        let representative_subject = AuthorizationSubject::Institution(representative_body.clone());
        // 票据按【规范账户】防重：换绑前后归并为同一张票，杜绝新旧钱包各投一票。
        let voter_account_id =
            <votingengine::Pallet<T>>::resolve_subject_voter(&representative_subject, &who)
                .unwrap_or_else(|| who.clone());
        let ticket = votingengine::types::InstitutionVoteTicket {
            role_subject: representative_body,
            voter_account_id,
        };
        let vote_key = (meta.current_body, ticket);
        ensure!(
            !pallet::RepresentativeVotesByTicket::<T>::contains_key(proposal_id, &vote_key),
            votingengine::Error::<T>::AlreadyVoted
        );
        ensure!(
            <votingengine::Pallet<T>>::is_subject_voter_in_snapshot(
                proposal_id,
                representative_subject.clone(),
                &who,
            ),
            votingengine::Error::<T>::NoPermission
        );

        pallet::RepresentativeVotesByTicket::<T>::insert(proposal_id, vote_key, approve);
        let tally =
            pallet::RepresentativeTallies::<T>::mutate(proposal_id, meta.current_body, |t| {
                if approve {
                    t.yes = t.yes.saturating_add(1);
                } else {
                    t.no = t.no.saturating_add(1);
                }
                *t
            });
        Self::deposit_event(pallet::Event::<T>::RepresentativeVoteCast {
            proposal_id,
            body_index: meta.current_body,
            who,
            voter_role_code,
            approve,
        });

        let voters_len =
            <votingengine::Pallet<T>>::subject_voters_len(proposal_id, representative_subject)
                .ok_or(votingengine::Error::<T>::MissingVoterSnapshot)?;
        match representative_decided(meta.rule, voters_len, tally.yes, tally.no) {
            Some(true) => match meta.route {
                RepresentativeRoute::Single(_) => {
                    Self::finish_single_representative_vote(proposal_id)
                }
                RepresentativeRoute::Sequential(_) => {
                    Self::advance_sequential_representative_vote(proposal_id)
                }
            },
            Some(false) => {
                <votingengine::Pallet<T>>::set_status_and_emit(proposal_id, STATUS_REJECTED)
            }
            None => Ok(()),
        }
    }

    /// 顺序路线推进至下一个代表机构；全部完成后进入配置的后续程序。
    pub(crate) fn advance_representative_body_or_finish(proposal_id: u64) -> DispatchResult {
        let meta = pallet::RepresentativeMetas::<T>::get(proposal_id)
            .ok_or(Error::<T>::ProposalMetaMissing)?;
        let next = meta.current_body.saturating_add(1);
        if (next as usize) < meta.route.len() {
            let next_body = meta
                .route
                .body(next)
                .ok_or(Error::<T>::InvalidRepresentativeRoute)?;
            let next_code = Self::institution_code_for_cid(&next_body.cid_number)?;
            let now = <frame_system::Pallet<T>>::block_number();
            let end = now.saturating_add(Self::stage_duration());
            with_transaction(|| {
                // 各机构计票按 body_index 永久隔离到提案清理，不删除前一阶段审计记录。
                pallet::RepresentativeMetas::<T>::mutate(proposal_id, |maybe| {
                    if let Some(m) = maybe {
                        m.current_body = next;
                    }
                });
                let old_end =
                    match Proposals::<T>::try_mutate(
                        proposal_id,
                        |maybe| -> Result<
                            frame_system::pallet_prelude::BlockNumberFor<T>,
                            DispatchError,
                        > {
                            let p = maybe
                                .as_mut()
                                .ok_or(votingengine::Error::<T>::ProposalNotFound)?;
                            let old = p.end;
                            p.internal_code = Some(next_code);
                            p.start = now;
                            p.end = end;
                            Ok(old)
                        },
                    ) {
                        Ok(v) => v,
                        Err(err) => return TransactionOutcome::Rollback(Err(err)),
                    };
                let old_expiry = old_end.saturating_add(One::one());
                ProposalsByExpiry::<T>::mutate(old_expiry, |ids| ids.retain(|&i| i != proposal_id));
                if let Err(err) =
                    <votingengine::Pallet<T>>::schedule_proposal_expiry(proposal_id, end)
                {
                    return TransactionOutcome::Rollback(Err(err));
                }
                Self::deposit_event(pallet::Event::<T>::RepresentativeBodyAdvanced {
                    proposal_id,
                    next_body: next,
                });
                TransactionOutcome::Commit(Ok(()))
            })
        } else {
            Self::finish_representative_route(proposal_id)
        }
    }

    /// 所有代表机构通过后按强类型程序进入终局或法律专属阶段。
    pub(crate) fn finish_representative_route(proposal_id: u64) -> DispatchResult {
        let meta = pallet::RepresentativeMetas::<T>::get(proposal_id)
            .ok_or(Error::<T>::ProposalMetaMissing)?;
        match meta.procedure {
            VoteProcedure::RepresentativeOnly => {
                <votingengine::Pallet<T>>::set_status_and_emit(proposal_id, STATUS_PASSED)
            }
            VoteProcedure::Legislation if meta.rule == RepresentativeVoteRule::Special => {
                Self::advance_to_referendum(proposal_id)
            }
            VoteProcedure::Legislation => Self::advance_to_sign(proposal_id),
        }
    }
}

impl<T: Config> Pallet<T> {
    /// 当前代表机构阶段超时结算：按强类型门槛计票，通过则推进，否则否决。
    pub fn do_finalize_representative_timeout(
        proposal: &Proposal<frame_system::pallet_prelude::BlockNumberFor<T>, T::AccountId>,
        proposal_id: u64,
    ) -> DispatchResult {
        ensure!(
            proposal.stage == STAGE_LEG_REPRESENTATIVE,
            votingengine::Error::<T>::InvalidProposalStage
        );
        ensure!(
            proposal.status == STATUS_VOTING,
            votingengine::Error::<T>::ProposalAlreadyFinalized
        );
        ensure!(
            <frame_system::Pallet<T>>::block_number() > proposal.end,
            votingengine::Error::<T>::VoteNotExpired
        );
        let meta = pallet::RepresentativeMetas::<T>::get(proposal_id)
            .ok_or(Error::<T>::ProposalMetaMissing)?;
        let representative_body = meta
            .route
            .body(meta.current_body)
            .ok_or(Error::<T>::InvalidRepresentativeRoute)?;
        let representative_subject = AuthorizationSubject::Institution(representative_body);
        let voters_len =
            <votingengine::Pallet<T>>::subject_voters_len(proposal_id, representative_subject)
                .ok_or(votingengine::Error::<T>::MissingVoterSnapshot)?;
        let tally = pallet::RepresentativeTallies::<T>::get(proposal_id, meta.current_body);
        if representative_final_passed(meta.rule, voters_len, tally.yes, tally.no) {
            match meta.route {
                RepresentativeRoute::Single(_) => {
                    Self::finish_single_representative_vote(proposal_id)
                }
                RepresentativeRoute::Sequential(_) => {
                    Self::advance_sequential_representative_vote(proposal_id)
                }
            }
        } else {
            <votingengine::Pallet<T>>::set_status_and_emit(proposal_id, STATUS_REJECTED)
        }
    }
}
