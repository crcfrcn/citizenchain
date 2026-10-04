//! 平台价格调整与统一投票引擎的唯一业务边界。
//!
//! 本模块只创建投票 action 并处理终态回调，不实现资格、快照、计票或状态推进。

use crate::{
    pallet::{Config, Error, Event, Pallet, PlatformPrice},
    MembershipLevel,
};
use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use entity_primitives::InstitutionRoleAuthorizationQuery;
use frame_support::ensure;
use scale_info::TypeInfo;
use sp_runtime::{Debug, DispatchResult};
use sp_std::vec::Vec;
use votingengine::{
    types::{
        AuthorizationSubject, BusinessActionId, CidNumber, RoleCode, RoleSubject, VotePlanOf,
        VotingEngineKind,
    },
    InternalVoteEngine,
};

#[derive(
    Clone, Encode, Decode, DecodeWithMemTracking, Eq, PartialEq, Debug, TypeInfo, MaxEncodedLen,
)]
pub struct PlatformPriceUpdateAction {
    pub actor_cid_number: votingengine::types::CidNumber,
    pub membership_level: MembershipLevel,
    pub new_price_fen: u128,
}

pub(crate) fn propose_price_change<T: Config>(
    who: T::AccountId,
    actor_cid_number: votingengine::types::CidNumber,
    proposer_role_code: votingengine::types::RoleCode,
    membership_level: MembershipLevel,
    new_price_fen: u128,
) -> DispatchResult {
    ensure!(new_price_fen > 0, Error::<T>::InvalidPlatformPrice);
    let actor_text = core::str::from_utf8(actor_cid_number.as_slice())
        .map_err(|_| Error::<T>::InvalidInstitution)?;
    let institution_code = votingengine::types::institution_code_from_cid_number(actor_text)
        .ok_or(Error::<T>::InvalidInstitution)?;
    // 平台机构永久固定为公民链基金会，直接以创世常量断言（机构码 SFGY + 准确 CID）。
    ensure!(
        primitives::cid::china::citizenchain::is_citizenchain_foundation_identity(
            institution_code,
            actor_cid_number.as_slice(),
        ),
        Error::<T>::NotPlatformInstitution
    );
    let action = PlatformPriceUpdateAction {
        actor_cid_number: actor_cid_number.clone(),
        membership_level,
        new_price_fen,
    };
    let mut encoded = Vec::from(crate::MODULE_TAG);
    encoded.extend_from_slice(&action.encode());
    let action_code = entity_primitives::business_action::ACTION_PLATFORM_PRICE;
    let action_id = BusinessActionId {
        module_tag: crate::MODULE_TAG.to_vec(),
        action_code,
    };
    let proposer = entity_primitives::RoleSubject {
        cid_number: actor_cid_number.to_vec(),
        role_code: proposer_role_code.to_vec(),
    };
    ensure!(
        T::InstitutionRoleAuthorization::is_authorized(
            &who,
            &proposer,
            &action_id,
            entity_primitives::RolePermissionOperation::Propose,
        ),
        Error::<T>::InvalidInstitution
    );
    let voter_subjects = T::InstitutionRoleAuthorization::role_subjects_with_permission(
        actor_cid_number.as_slice(),
        &action_id,
        entity_primitives::RolePermissionOperation::Vote,
    )
    .into_iter()
    .map(|role| {
        Ok(AuthorizationSubject::Institution(RoleSubject {
            cid_number: CidNumber::try_from(role.cid_number)
                .map_err(|_| sp_runtime::DispatchError::Other("InvalidVotePlan"))?,
            role_code: RoleCode::try_from(role.role_code)
                .map_err(|_| sp_runtime::DispatchError::Other("InvalidVotePlan"))?,
        }))
    })
    .collect::<Result<Vec<_>, sp_runtime::DispatchError>>()?;
    let owner: frame_support::BoundedVec<
        u8,
        frame_support::traits::ConstU32<{ entity_primitives::BUSINESS_MODULE_TAG_MAX_BYTES }>,
    > = crate::MODULE_TAG
        .to_vec()
        .try_into()
        .map_err(|_| sp_runtime::DispatchError::Other("InvalidVotePlan"))?;
    let vote_plan = VotePlanOf::<T::AccountId>::try_new(
        BusinessActionId {
            module_tag: owner.clone(),
            action_code,
        },
        owner,
        AuthorizationSubject::Institution(RoleSubject {
            cid_number: actor_cid_number.clone(),
            role_code: proposer_role_code,
        }),
        voter_subjects,
        VotingEngineKind::Internal,
        sp_io::hashing::blake2_256(&encoded),
    )
    .map_err(|_| sp_runtime::DispatchError::Other("InvalidVotePlan"))?;
    let proposal_id = T::InternalVoteEngine::create_institution_proposal_with_data(
        who,
        institution_code,
        actor_cid_number.to_vec(),
        None,
        Vec::from([actor_cid_number.to_vec()]),
        vote_plan,
        encoded,
    )?;
    Pallet::<T>::deposit_event(Event::<T>::PlatformPriceChangeProposed {
        proposal_id,
        actor_cid_number,
        membership_level,
        new_price_fen,
    });
    Ok(())
}

/// 平台调价终态执行器。业务模块只接收统一投票引擎广播的最终结果。
pub struct InternalVoteExecutor<T>(core::marker::PhantomData<T>);

impl<T> votingengine::InternalVoteResultCallback for InternalVoteExecutor<T>
where
    T: Config + votingengine::Config,
{
    fn on_internal_vote_finalized(
        proposal_id: u64,
        approved: bool,
    ) -> Result<votingengine::ProposalExecutionOutcome, sp_runtime::DispatchError> {
        use votingengine::{
            ProposalExecutionOutcome, PROPOSAL_KIND_INTERNAL, STAGE_INTERNAL, STATUS_PASSED,
        };
        let raw = match votingengine::Pallet::<T>::get_proposal_data(proposal_id) {
            Some(raw)
                if votingengine::Pallet::<T>::is_proposal_owner(proposal_id, crate::MODULE_TAG)
                    && raw.starts_with(crate::MODULE_TAG) =>
            {
                raw
            }
            _ => return Ok(ProposalExecutionOutcome::Ignored),
        };
        if !approved {
            return Ok(ProposalExecutionOutcome::Executed);
        }
        let action = PlatformPriceUpdateAction::decode(&mut &raw[crate::MODULE_TAG.len()..])
            .map_err(|_| Error::<T>::ProposalActionNotFound)?;
        let proposal = votingengine::Pallet::<T>::proposals(proposal_id)
            .ok_or(Error::<T>::ProposalActionNotFound)?;
        ensure!(
            votingengine::Pallet::<T>::is_callback_execution_scope(proposal_id)
                && proposal.kind == PROPOSAL_KIND_INTERNAL
                && proposal.stage == STAGE_INTERNAL
                && proposal.status == STATUS_PASSED
                && proposal.actor_cid_number.as_ref().map(|cid| cid.as_slice())
                    == Some(action.actor_cid_number.as_slice())
                // 平台机构永久固定为公民链基金会，以创世常量 CID 断言，不读可写存储。
                && action.actor_cid_number.as_slice()
                    == primitives::cid::china::citizenchain::CITIZENCHAIN_FOUNDATION
                        .cid_number
                        .as_bytes()
                && proposal.execution_account_id.is_none(),
            Error::<T>::ProposalNotPassed
        );
        ensure!(action.new_price_fen > 0, Error::<T>::InvalidPlatformPrice);
        let old_price_fen = PlatformPrice::<T>::get(action.membership_level);
        PlatformPrice::<T>::insert(action.membership_level, action.new_price_fen);
        Pallet::<T>::deposit_event(Event::<T>::PlatformPriceChanged {
            proposal_id,
            membership_level: action.membership_level,
            old_price_fen,
            new_price_fen: action.new_price_fen,
        });
        Ok(ProposalExecutionOutcome::Executed)
    }
}
