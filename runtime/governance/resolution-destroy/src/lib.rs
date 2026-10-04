#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use alloc::vec::Vec;
use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::{ensure, pallet_prelude::*, traits::Currency};
use frame_system::pallet_prelude::*;
use primitives::{account_derive::RESERVED_NAME_FEE, fee_policy::OnchainFeeCharger};
use scale_info::TypeInfo;
use sp_runtime::traits::{CheckedAdd, Zero};

use votingengine::{
    types::{
        AuthorizationSubject, BusinessActionId, CidNumber, InstitutionCode, RoleCode, RoleSubject,
        VotePlanOf, VotingEngineKind, NRC, PRB, PRC,
    },
    InternalVoteResultCallback, ProposalExecutionOutcome, PROPOSAL_KIND_INTERNAL, STAGE_INTERNAL,
    STATUS_PASSED,
};

pub use pallet::*;
#[cfg(feature = "runtime-benchmarks")]
mod benchmarks;
pub mod weights;

/// 模块标识前缀，用于在 ProposalData 中区分不同业务模块，防止跨模块误解码。
pub const MODULE_TAG: &[u8] = b"res-dst";

/// 决议销毁是储备治理三档专属业务；内部投票引擎本身不承担这项业务限权。
fn can_propose_destroy(institution_code: InstitutionCode) -> bool {
    matches!(institution_code, NRC | PRC | PRB)
}

type BalanceOf<T> =
    <<T as pallet::Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub struct DestroyAction<AccountId, Balance> {
    /// 发起机构唯一 CID。
    pub actor_cid_number: CidNumber,
    /// 执行销毁的具体机构账户。
    pub institution_account_id: AccountId,
    /// 销毁数量
    pub amount: Balance,
}

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use crate::weights::WeightInfo;
    use entity_primitives::InstitutionMultisigQuery;
    use votingengine::InternalVoteEngine;

    #[pallet::config]
    pub trait Config: frame_system::Config + votingengine::Config {
        #[allow(deprecated)]
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

        type Currency: Currency<Self::AccountId>;

        /// 通过统一内部投票引擎创建提案，返回真实 proposal_id。
        type InternalVoteEngine: votingengine::InternalVoteEngine<Self::AccountId>;

        /// 机构岗位业务授权真源，由 runtime 路由到公权或私权机构目录。
        type InstitutionRoleAuthorization: entity_primitives::InstitutionRoleAuthorizationQuery<
            Self::AccountId,
        >;

        /// 机构账户归属的唯一查询出口；真实数据来自 entity 正反索引。
        type InstitutionQuery: entity_primitives::InstitutionMultisigQuery<Self::AccountId>;

        /// 投票通过后从 actor CID 费用账户收取金额手续费。
        type OnchainFeeCharger: primitives::fee_policy::OnchainFeeCharger<
            Self::AccountId,
            BalanceOf<Self>,
        >;

        /// 该 pallet 的可配置权重实现。
        type WeightInfo: crate::weights::WeightInfo;
    }

    #[pallet::pallet]
    pub struct Pallet<T>(_);

    // 提案数据、元数据、活跃提案列表均已移至 votingengine 统一管控。

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        /// 已发起销毁提案（并已在投票引擎创建内部提案）
        DestroyProposed {
            proposal_id: u64,
            institution_code: InstitutionCode,
            institution: T::AccountId,
            proposer: T::AccountId,
            amount: BalanceOf<T>,
        },
        /// 提交销毁投票
        DestroyVoteSubmitted {
            proposal_id: u64,
            who: T::AccountId,
            approve: bool,
        },
        /// 提案达到通过状态但自动执行失败（投票不回滚）
        DestroyExecutionFailed { proposal_id: u64 },
        /// 销毁执行完成
        DestroyExecuted {
            proposal_id: u64,
            institution: T::AccountId,
            fee_payer: T::AccountId,
            amount: BalanceOf<T>,
            fee: BalanceOf<T>,
        },
    }

    #[pallet::error]
    pub enum Error<T> {
        InvalidInstitution,
        InstitutionCodeMismatch,
        UnauthorizedAdmin,
        ZeroAmount,
        ProposalActionNotFound,
        ProposalNotPassed,
        InstitutionAccountDecodeFailed,
        InsufficientBalance,
        FeeAccountMissing,
        FeeWithdrawFailed,
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        /// 发起“决议销毁”内部投票提案。
        #[pallet::call_index(0)]
        #[pallet::weight(<T as pallet::Config>::WeightInfo::propose_destroy())]
        pub fn propose_destroy(
            origin: OriginFor<T>,
            actor_cid_number: CidNumber,
            proposer_role_code: RoleCode,
            institution_account_id: T::AccountId,
            amount: BalanceOf<T>,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;

            ensure!(amount > Zero::zero(), Error::<T>::ZeroAmount);
            let actual_org = T::InstitutionQuery::lookup_org(&institution_account_id)
                .ok_or(Error::<T>::InvalidInstitution)?;
            ensure!(
                can_propose_destroy(actual_org),
                Error::<T>::InvalidInstitution
            );
            ensure!(
                T::InstitutionQuery::lookup_cid(&institution_account_id).as_deref()
                    == Some(actor_cid_number.as_slice()),
                Error::<T>::InstitutionCodeMismatch
            );
            let action = DestroyAction {
                actor_cid_number: actor_cid_number.clone(),
                institution_account_id: institution_account_id.clone(),
                amount,
            };
            let mut encoded = Vec::from(crate::MODULE_TAG);
            encoded.extend_from_slice(&action.encode());
            let vote_plan = Self::build_vote_plan(
                &who,
                actor_cid_number.as_slice(),
                proposer_role_code.as_slice(),
                &encoded,
            )?;
            let proposal_id = T::InternalVoteEngine::create_institution_proposal_with_data(
                who.clone(),
                actual_org,
                actor_cid_number.to_vec(),
                Some(institution_account_id.clone()),
                Vec::from([actor_cid_number.to_vec()]),
                vote_plan,
                encoded,
            )?;

            Self::deposit_event(Event::<T>::DestroyProposed {
                proposal_id,
                institution_code: actual_org,
                institution: institution_account_id,
                proposer: who,
                amount,
            });
            Ok(())
        }

        // call_index = 1 永久保留空位,不复用。
    }

    impl<T: Config> Pallet<T> {
        fn build_vote_plan(
            who: &T::AccountId,
            cid_number: &[u8],
            proposer_role_code: &[u8],
            encoded: &[u8],
        ) -> Result<VotePlanOf<T::AccountId>, sp_runtime::DispatchError> {
            use entity_primitives::{InstitutionRoleAuthorizationQuery, RolePermissionOperation};

            let action_id = BusinessActionId {
                module_tag: crate::MODULE_TAG.to_vec(),
                action_code: entity_primitives::business_action::ACTION_RESOLUTION_DESTROY,
            };
            let proposer = entity_primitives::RoleSubject {
                cid_number: cid_number.to_vec(),
                role_code: proposer_role_code.to_vec(),
            };
            ensure!(
                T::InstitutionRoleAuthorization::is_authorized(
                    who,
                    &proposer,
                    &action_id,
                    RolePermissionOperation::Propose,
                ),
                Error::<T>::UnauthorizedAdmin
            );
            let voter_subjects = T::InstitutionRoleAuthorization::role_subjects_with_permission(
                cid_number,
                &action_id,
                RolePermissionOperation::Vote,
            )
            .into_iter()
            .map(|role| {
                Ok(AuthorizationSubject::Institution(RoleSubject {
                    cid_number: CidNumber::try_from(role.cid_number)
                        .map_err(|_| votingengine::Error::<T>::InvalidVotePlan)?,
                    role_code: RoleCode::try_from(role.role_code)
                        .map_err(|_| votingengine::Error::<T>::InvalidVotePlan)?,
                }))
            })
            .collect::<Result<Vec<_>, sp_runtime::DispatchError>>()?;
            let owner: frame_support::BoundedVec<
                u8,
                frame_support::traits::ConstU32<
                    { entity_primitives::BUSINESS_MODULE_TAG_MAX_BYTES },
                >,
            > = crate::MODULE_TAG
                .to_vec()
                .try_into()
                .map_err(|_| votingengine::Error::<T>::InvalidVotePlan)?;
            VotePlanOf::<T::AccountId>::try_new(
                BusinessActionId {
                    module_tag: owner.clone(),
                    action_code: entity_primitives::business_action::ACTION_RESOLUTION_DESTROY,
                },
                owner,
                AuthorizationSubject::Institution(RoleSubject {
                    cid_number: CidNumber::try_from(cid_number.to_vec())
                        .map_err(|_| votingengine::Error::<T>::InvalidVotePlan)?,
                    role_code: RoleCode::try_from(proposer_role_code.to_vec())
                        .map_err(|_| votingengine::Error::<T>::InvalidVotePlan)?,
                }),
                voter_subjects,
                VotingEngineKind::Internal,
                sp_io::hashing::blake2_256(encoded),
            )
            .map_err(|_| votingengine::Error::<T>::InvalidVotePlan.into())
        }

        pub(crate) fn try_execute_destroy_from_action(
            proposal_id: u64,
            action: DestroyAction<T::AccountId, BalanceOf<T>>,
        ) -> DispatchResult {
            let proposal = votingengine::Pallet::<T>::proposals(proposal_id)
                .ok_or(Error::<T>::ProposalActionNotFound)?;
            let actual_org = T::InstitutionQuery::lookup_org(&action.institution_account_id)
                .ok_or(Error::<T>::InvalidInstitution)?;
            let cid = T::InstitutionQuery::lookup_cid(&action.institution_account_id)
                .ok_or(Error::<T>::InvalidInstitution)?;
            // PASSED 是可执行/可重试态；每次自动执行和统一重试都重新绑定
            // owner、投票模式、机构码、机构账户和 CID，不能只信任业务载荷。
            ensure!(
                votingengine::Pallet::<T>::is_callback_execution_scope(proposal_id)
                    && votingengine::Pallet::<T>::is_proposal_owner(proposal_id, crate::MODULE_TAG,)
                    && proposal.kind == PROPOSAL_KIND_INTERNAL
                    && proposal.stage == STAGE_INTERNAL
                    && proposal.status == STATUS_PASSED
                    && proposal.internal_code == Some(actual_org)
                    && proposal
                        .actor_cid_number
                        .as_ref()
                        .map(|value| value.as_slice())
                        == Some(action.actor_cid_number.as_slice())
                    && proposal.execution_account_id == Some(action.institution_account_id.clone())
                    && cid.as_slice() == action.actor_cid_number.as_slice(),
                Error::<T>::ProposalNotPassed
            );
            ensure!(
                can_propose_destroy(actual_org),
                Error::<T>::InvalidInstitution
            );

            let free = T::Currency::free_balance(&action.institution_account_id);
            let ed = T::Currency::minimum_balance();
            // 销毁前必须预留 ED，确保机构账户不会因一次销毁被直接 reap。
            let required = action
                .amount
                .checked_add(&ed)
                .ok_or(Error::<T>::InsufficientBalance)?;
            ensure!(free >= required, Error::<T>::InsufficientBalance);

            let fee_account = T::InstitutionQuery::lookup_institution_account(
                action.actor_cid_number.as_slice(),
                RESERVED_NAME_FEE,
            )
            .ok_or(Error::<T>::FeeAccountMissing)?;

            // 金额手续费和本金销毁同属一次执行；任一失败时分账、事件和销毁全部回滚。
            let fee_result: Result<BalanceOf<T>, sp_runtime::DispatchError> =
                frame_support::storage::with_transaction(|| {
                    let fee = match T::OnchainFeeCharger::charge(&fee_account, action.amount) {
                        Ok(fee) => fee,
                        Err(_) => {
                            return frame_support::storage::TransactionOutcome::Rollback(Err(
                                Error::<T>::FeeWithdrawFailed.into(),
                            ))
                        }
                    };
                    let (negative_imbalance, remaining) =
                        T::Currency::slash(&action.institution_account_id, action.amount);
                    if !remaining.is_zero() {
                        return frame_support::storage::TransactionOutcome::Rollback(Err(
                            Error::<T>::InsufficientBalance.into(),
                        ));
                    }
                    drop(negative_imbalance);
                    frame_support::storage::TransactionOutcome::Commit(Ok(fee))
                });
            let fee = fee_result?;

            Self::deposit_event(Event::<T>::DestroyExecuted {
                proposal_id,
                institution: action.institution_account_id,
                fee_payer: fee_account,
                amount: action.amount,
                fee,
            });
            Ok(())
        }
    }
}

// ──── 投票终态回调:把已通过的销毁提案落地到链上 ────
//
// 投票统一由投票引擎承担,提案通过(或否决)经
// [`votingengine::InternalVoteResultCallback`] 广播回来。
// 本 Executor 按 `MODULE_TAG` 前缀认领本模块的提案,非己方返回 Ignored。
pub struct InternalVoteExecutor<T>(core::marker::PhantomData<T>);

impl<T: pallet::Config> InternalVoteResultCallback for InternalVoteExecutor<T> {
    fn on_internal_vote_finalized(
        proposal_id: u64,
        approved: bool,
    ) -> Result<ProposalExecutionOutcome, sp_runtime::DispatchError> {
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
        let action = DestroyAction::<T::AccountId, BalanceOf<T>>::decode(
            &mut &raw[crate::MODULE_TAG.len()..],
        )
        .map_err(|_| pallet::Error::<T>::ProposalActionNotFound)?;

        match pallet::Pallet::<T>::try_execute_destroy_from_action(proposal_id, action) {
            Ok(()) => Ok(ProposalExecutionOutcome::Executed),
            Err(_) => {
                pallet::Pallet::<T>::deposit_event(pallet::Event::<T>::DestroyExecutionFailed {
                    proposal_id,
                });
                Ok(ProposalExecutionOutcome::RetryableFailed)
            }
        }
    }
}

#[cfg(test)]
mod tests;
