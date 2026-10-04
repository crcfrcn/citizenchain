//! 决议发行提案与联合投票回调逻辑。

use crate::pallet::{
    AllocationOf, BalanceOf, Config, Error, Event, FinalizeOutcome, Pallet, ReasonOf,
    VotingProposalCount,
};
use codec::{Decode, Encode};
use entity_primitives::{
    AuthorizationSubject, BusinessActionId, InstitutionRoleAuthorizationQuery,
    RolePermissionOperation, RoleSubject,
};
use frame_support::{
    dispatch::DispatchResult,
    ensure,
    pallet_prelude::*,
    storage::{with_transaction, TransactionOutcome},
};
use primitives::{
    cid::{china::china_cb::CHINA_CB, china::china_ch::CHINA_CH},
    governance_skeleton::{ROLE_CODE_COMMITTEE_MEMBER, ROLE_CODE_DIRECTOR},
};
use sp_runtime::traits::{Hash as HashT, Zero};
use sp_runtime::DispatchError;
use sp_std::vec::Vec;
use votingengine::{
    JointVoteEngine, PROPOSAL_KIND_JOINT, STAGE_JOINT, STAGE_REFERENDUM, STATUS_PASSED,
    STATUS_REJECTED,
};

#[derive(
    Encode, Decode, DecodeWithMemTracking, Clone, Debug, TypeInfo, MaxEncodedLen, PartialEq, Eq,
)]
/// 单条收款分配项，包含收款账户和分配金额。
pub struct RecipientAmount<AccountId, Balance> {
    pub recipient_account_id: AccountId,
    pub amount: Balance,
}

/// 存入 votingengine ProposalData 的业务数据结构。
#[derive(Encode, Decode, Clone, Debug, PartialEq, Eq)]
pub struct IssuanceProposalData<AccountId, Balance> {
    pub actor_cid_number: votingengine::types::CidNumber,
    pub proposer_role_code: votingengine::types::RoleCode,
    pub proposer_account_id: AccountId,
    pub reason: Vec<u8>,
    pub total_amount: Balance,
    pub allocations: Vec<RecipientAmount<AccountId, Balance>>,
}

impl<T: Config> Pallet<T> {
    fn bounded_role_subject(
        cid_number: &[u8],
        role_code: &[u8],
    ) -> Result<
        entity_primitives::RoleSubject<
            votingengine::types::CidNumber,
            votingengine::types::RoleCode,
        >,
        DispatchError,
    > {
        Ok(entity_primitives::RoleSubject {
            cid_number: cid_number
                .to_vec()
                .try_into()
                .map_err(|_| Error::<T>::InvalidActorCid)?,
            role_code: role_code
                .to_vec()
                .try_into()
                .map_err(|_| Error::<T>::InvalidActorCid)?,
        })
    }

    /// 决议发行与协议升级使用完全相同的固定联合投票岗位集合。
    fn build_vote_plan(
        actor_cid_number: &votingengine::types::CidNumber,
        proposer_role_code: &votingengine::types::RoleCode,
        business_object_hash: [u8; 32],
    ) -> Result<votingengine::types::VotePlanOf<T::AccountId>, DispatchError> {
        let proposer_role =
            Self::bounded_role_subject(actor_cid_number.as_slice(), proposer_role_code.as_slice())?;
        let mut voters = Vec::new();
        for entry in CHINA_CB.iter() {
            voters.push(AuthorizationSubject::Institution(
                Self::bounded_role_subject(
                    entry.cid_number.as_bytes(),
                    ROLE_CODE_COMMITTEE_MEMBER,
                )?,
            ));
        }
        for entry in CHINA_CH.iter() {
            voters.push(AuthorizationSubject::Institution(
                Self::bounded_role_subject(entry.cid_number.as_bytes(), ROLE_CODE_DIRECTOR)?,
            ));
        }
        let module_tag: BoundedVec<
            u8,
            ConstU32<{ entity_primitives::BUSINESS_MODULE_TAG_MAX_BYTES }>,
        > = crate::MODULE_TAG
            .to_vec()
            .try_into()
            .map_err(|_| Error::<T>::JointVoteCreateFailed)?;
        votingengine::types::VotePlanOf::<T::AccountId>::try_new(
            BusinessActionId {
                module_tag: module_tag.clone(),
                action_code: entity_primitives::business_action::ACTION_RESOLUTION_ISSUANCE,
            },
            module_tag,
            AuthorizationSubject::Institution(proposer_role),
            voters,
            votingengine::types::VotingEngineKind::Joint,
            business_object_hash,
        )
        .map_err(|_| Error::<T>::JointVoteCreateFailed.into())
    }

    pub(crate) fn create_resolution_issuance_proposal(
        proposer_account_id: T::AccountId,
        actor_cid_number: votingengine::types::CidNumber,
        proposer_role_code: votingengine::types::RoleCode,
        reason: ReasonOf<T>,
        total_amount: BalanceOf<T>,
        allocations: AllocationOf<T>,
    ) -> DispatchResult {
        ensure!(!reason.is_empty(), Error::<T>::EmptyReason);
        let actor_text = core::str::from_utf8(actor_cid_number.as_slice())
            .map_err(|_| Error::<T>::InvalidActorCid)?;
        let actor_code = votingengine::types::institution_code_from_cid_number(actor_text)
            .ok_or(Error::<T>::InvalidActorCid)?;
        ensure!(
            matches!(
                actor_code,
                votingengine::types::NRC | votingengine::types::PRC
            ),
            Error::<T>::InvalidActorCid
        );
        let proposer_role = RoleSubject {
            cid_number: actor_cid_number.to_vec(),
            role_code: proposer_role_code.to_vec(),
        };
        let business_action = BusinessActionId {
            module_tag: crate::MODULE_TAG.to_vec(),
            action_code: entity_primitives::business_action::ACTION_RESOLUTION_ISSUANCE,
        };
        ensure!(
            T::InstitutionRoleAuthorization::is_authorized(
                &proposer_account_id,
                &proposer_role,
                &business_action,
                RolePermissionOperation::Propose,
            ),
            Error::<T>::UnauthorizedActorRole
        );
        Self::validate_proposal_allocations(&total_amount, allocations.as_slice())?;

        // 联合投票提案创建、业务数据写入和计数递增必须原子提交；
        // 任一步失败都不能留下孤儿提案或错误的 VotingProposalCount。
        with_transaction(|| {
            let data = IssuanceProposalData {
                actor_cid_number: actor_cid_number.clone(),
                proposer_role_code: proposer_role_code.clone(),
                proposer_account_id: proposer_account_id.clone(),
                reason: reason.to_vec(),
                total_amount,
                allocations: allocations.to_vec(),
            };
            let mut encoded = Vec::from(crate::MODULE_TAG);
            encoded.extend_from_slice(&data.encode());
            let encoded_hash = T::Hashing::hash(encoded.as_slice());
            let mut business_object_hash = [0u8; 32];
            business_object_hash.copy_from_slice(encoded_hash.as_ref());
            let vote_plan = match Self::build_vote_plan(
                &actor_cid_number,
                &proposer_role_code,
                business_object_hash,
            ) {
                Ok(plan) => plan,
                Err(err) => return TransactionOutcome::Rollback(Err(err)),
            };
            let proposal_id = match T::JointVoteEngine::create_joint_proposal_with_data(
                proposer_account_id.clone(),
                actor_cid_number.to_vec(),
                vote_plan,
                encoded,
            ) {
                Ok(id) => id,
                Err(_) => {
                    return TransactionOutcome::Rollback(Err(
                        Error::<T>::JointVoteCreateFailed.into()
                    ))
                }
            };

            if let Err(err) = Self::increment_voting_proposal_count() {
                return TransactionOutcome::Rollback(Err(err));
            }

            Self::deposit_event(Event::<T>::ResolutionIssuanceProposed {
                proposal_id,
                actor_cid_number,
                proposer_role_code,
                proposer_account_id,
                total_amount,
                allocation_count: allocations.len() as u32,
            });
            TransactionOutcome::Commit(Ok(()))
        })
    }

    /// 从投票引擎 ProposalData 中读取并解码本模块的业务数据。
    pub fn load_proposal_data(
        proposal_id: u64,
    ) -> Option<IssuanceProposalData<T::AccountId, BalanceOf<T>>> {
        let raw = votingengine::Pallet::<T>::get_proposal_data(proposal_id)?;
        Self::decode_tagged_data(&raw)
    }

    /// 判断指定提案是否属于本模块。
    pub fn owns_proposal(proposal_id: u64) -> bool {
        votingengine::Pallet::<T>::is_proposal_owner(proposal_id, crate::MODULE_TAG)
    }

    fn decode_tagged_data(raw: &[u8]) -> Option<IssuanceProposalData<T::AccountId, BalanceOf<T>>> {
        let tag = crate::MODULE_TAG;
        if raw.len() < tag.len() || &raw[..tag.len()] != tag {
            return None;
        }
        let mut input = &raw[tag.len()..];
        let decoded = IssuanceProposalData::decode(&mut input).ok()?;
        input.is_empty().then_some(decoded)
    }

    pub(crate) fn apply_joint_vote_result(
        proposal_id: u64,
        approved: bool,
    ) -> Result<FinalizeOutcome, DispatchError> {
        // 联合投票终结、发行执行和计数递减必须在同一事务里提交；
        // votingengine 负责在外层终态转换后统一登记提案清理。
        with_transaction(|| {
            if let Err(err) = Self::ensure_vote_engine_callback_context(proposal_id, approved) {
                return TransactionOutcome::Rollback(Err(err));
            }
            let data = match Self::load_proposal_data(proposal_id) {
                Some(data) => data,
                None => {
                    return TransactionOutcome::Rollback(Err(Error::<T>::ProposalNotFound.into()))
                }
            };

            if approved {
                let execute_reason: ReasonOf<T> = match data.reason.clone().try_into() {
                    Ok(v) => v,
                    Err(_) => {
                        // reason 原本由 ReasonOf<T> 写入 ProposalData；
                        // 如果回读时超限，说明链上业务数据异常，而不是提案不存在。
                        return TransactionOutcome::Rollback(Err(Error::<T>::ReasonTooLong.into()));
                    }
                };
                let execute_allocations: AllocationOf<T> = match data.allocations.clone().try_into()
                {
                    Ok(v) => v,
                    Err(_) => {
                        return TransactionOutcome::Rollback(Err(
                            Error::<T>::InvalidAllocationCount.into(),
                        ))
                    }
                };

                if Self::execute_approved_issuance(
                    proposal_id,
                    &execute_reason,
                    data.total_amount,
                    &execute_allocations,
                )
                .is_ok()
                {
                    if let Err(err) = Self::decrement_voting_proposal_count() {
                        return TransactionOutcome::Rollback(Err(err));
                    }
                    Self::deposit_event(Event::<T>::JointVoteFinalized {
                        proposal_id,
                        approved: true,
                    });
                    Self::deposit_event(Event::<T>::IssuanceExecutionTriggered {
                        proposal_id,
                        total_amount: data.total_amount,
                    });
                    return TransactionOutcome::Commit(Ok(
                        FinalizeOutcome::ApprovedExecutionSucceeded,
                    ));
                }

                // 执行失败不保留重试分支；交由回调返回值写入失败终态。
                if let Err(err) = Self::decrement_voting_proposal_count() {
                    return TransactionOutcome::Rollback(Err(err));
                }
                Self::deposit_event(Event::<T>::JointVoteFinalized {
                    proposal_id,
                    approved: true,
                });
                Self::deposit_event(Event::<T>::IssuanceExecutionFailed { proposal_id });
                return TransactionOutcome::Commit(Ok(FinalizeOutcome::ApprovedExecutionFailed));
            }

            if let Err(err) = Self::decrement_voting_proposal_count() {
                return TransactionOutcome::Rollback(Err(err));
            }
            Self::deposit_event(Event::<T>::JointVoteFinalized {
                proposal_id,
                approved: false,
            });
            TransactionOutcome::Commit(Ok(FinalizeOutcome::Rejected))
        })
    }

    fn ensure_vote_engine_callback_context(proposal_id: u64, approved: bool) -> DispatchResult {
        // 决议发行只接受 votingengine 在终态转换事务内发起的回调，
        // 不再提供任何 Root 或外部来源可直接触发的手工 finalize 路径。
        ensure!(
            votingengine::pallet::CallbackExecutionScopes::<T>::contains_key(proposal_id),
            Error::<T>::ProposalNotFinalizable
        );
        let proposal = votingengine::Pallet::<T>::proposals(proposal_id)
            .ok_or(Error::<T>::ProposalNotFound)?;
        ensure!(
            votingengine::Pallet::<T>::is_proposal_owner(proposal_id, crate::MODULE_TAG),
            Error::<T>::ProposalNotFinalizable
        );
        // 联合机构全票通过时停留在 STAGE_JOINT；非全票转公投后，合法通过
        // 状态是 STAGE_REFERENDUM。业务回调必须同时接受这两条法定终局路径。
        ensure!(
            proposal.kind == PROPOSAL_KIND_JOINT
                && matches!(proposal.stage, STAGE_JOINT | STAGE_REFERENDUM),
            Error::<T>::ProposalNotFinalizable
        );
        let expected_status = if approved {
            STATUS_PASSED
        } else {
            STATUS_REJECTED
        };
        ensure!(
            proposal.status == expected_status,
            Error::<T>::ProposalNotFinalizable
        );
        Ok(())
    }

    pub(crate) fn increment_voting_proposal_count() -> DispatchResult {
        VotingProposalCount::<T>::try_mutate(|count| -> DispatchResult {
            *count = count
                .checked_add(1)
                .ok_or(Error::<T>::VotingProposalCountOverflow)?;
            Ok(())
        })
    }

    pub(crate) fn decrement_voting_proposal_count() -> DispatchResult {
        VotingProposalCount::<T>::try_mutate(|count| -> DispatchResult {
            *count = count
                .checked_sub(1)
                .ok_or(Error::<T>::VotingProposalCountUnderflow)?;
            Ok(())
        })
    }

    pub(crate) fn ensure_nonzero_total(total_amount: &BalanceOf<T>) -> DispatchResult {
        ensure!(!total_amount.is_zero(), Error::<T>::ZeroAmount);
        Ok(())
    }
}
