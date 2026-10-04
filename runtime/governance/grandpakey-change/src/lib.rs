//! # GRANDPA 验证密钥治理模块
//!
//! 本模块只管理 NRC/PRC 自有 GRANDPA authority 密钥，提供两条互斥路径：
//! - 正常更换：目标机构单个委员按 `CID + 委员岗位码 + account_id` 授权，旧、新
//!   GRANDPA 私钥对同一证明摘要签名后延迟生效，不进入投票。
//! - 紧急恢复：旧私钥丢失或无法签名时，由目标机构自己的委员岗位发起并在本机构
//!   内部投票；NRC、各 PRC 彼此独立，不组成联合投票，PRB 不拥有 GRANDPA authority。
//!
//! `pallet-grandpa` 真正切换 authority set 后，本模块才更新机构当前公钥映射并发出
//! 生效事件。节点据此在 finalized 确认后删除旧私钥；生效前旧、新私钥必须同时保留。

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use frame_support::{ensure, pallet_prelude::*, traits::StorageVersion, Blake2_128Concat};
use frame_system::pallet_prelude::*;
use primitives::cid::china::china_cb::CHINA_CB;
use scale_info::TypeInfo;
use votingengine::{
    types::{CidNumber, InstitutionCode, RoleCode, NRC, PRC},
    InternalVoteResultCallback, ProposalCancelDecision, ProposalExecutionOutcome,
    PROPOSAL_KIND_INTERNAL, STAGE_INTERNAL, STATUS_PASSED,
};

pub const MODULE_TAG: &[u8] = b"gra-key";

pub use pallet::*;
pub use proof::signing_digest as proof_signing_digest;
pub use proof::{GrandpaKeyChangeKind, GrandpaKeyProofPayload};

#[cfg(feature = "runtime-benchmarks")]
mod benchmarks;
mod proof;
mod recovery;
mod rotation;
pub mod weights;

const STORAGE_VERSION: StorageVersion = StorageVersion::new(0);

#[derive(
    Clone, Debug, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, TypeInfo, MaxEncodedLen,
)]
/// 紧急恢复内部投票绑定的业务动作；不保存私钥或重复保存签名。
pub struct EmergencyGrandpaKeyRecoveryAction<AccountId, BlockNumber> {
    pub actor_cid_number: CidNumber,
    pub actor_role_code: RoleCode,
    pub initiator_account_id: AccountId,
    pub old_public_key: [u8; 32],
    pub new_public_key: [u8; 32],
    pub proof_nonce: u64,
    pub proof_expires_at: BlockNumber,
}

#[derive(
    Clone, Debug, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, TypeInfo, MaxEncodedLen,
)]
/// 已由 `pallet-grandpa` 接受调度、等待 authority set 实际生效的变更。
pub struct PendingGrandpaKeyChangeState<AccountId, BlockNumber> {
    pub actor_cid_number: CidNumber,
    pub initiator_account_id: AccountId,
    pub old_public_key: [u8; 32],
    pub new_public_key: [u8; 32],
    pub proof_nonce: u64,
    pub change_kind: GrandpaKeyChangeKind,
    pub proposal_id: Option<u64>,
    pub expected_set_id: u64,
    pub scheduled_at: BlockNumber,
    pub activate_at: BlockNumber,
}

/// 只允许 NRC 与 PRC 使用本模块；PRB 没有 GRANDPA authority。
fn cid_org(actor_cid_number: &[u8]) -> Option<InstitutionCode> {
    let actor_text = core::str::from_utf8(actor_cid_number).ok()?;
    match votingengine::types::institution_code_from_cid_number(actor_text)? {
        NRC => Some(NRC),
        PRC => Some(PRC),
        _ => None,
    }
}

// 两条外部调用的参数数量由固定 SCALE 协议决定；不得把签名或证明字段藏入不透明字节。
#[allow(clippy::too_many_arguments)]
#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use crate::{proof, weights::WeightInfo};
    use sp_std::vec::Vec;
    use votingengine::InternalVoteEngine;

    #[pallet::config]
    pub trait Config: frame_system::Config + votingengine::Config + pallet_grandpa::Config {
        #[allow(deprecated)]
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

        #[pallet::constant]
        type GrandpaChangeDelay: Get<BlockNumberFor<Self>>;

        /// 紧急恢复只调用现有内部投票引擎创建提案，不在业务模块复制投票流程。
        type InternalVoteEngine: votingengine::InternalVoteEngine<Self::AccountId>;

        /// `CID + 岗位码 + account_id` 机构岗位授权唯一真源。
        type InstitutionRoleAuthorization: entity_primitives::InstitutionRoleAuthorizationQuery<
            Self::AccountId,
        >;

        type WeightInfo: crate::weights::WeightInfo;
    }

    #[pallet::pallet]
    #[pallet::storage_version(STORAGE_VERSION)]
    pub struct Pallet<T>(_);

    /// 各机构最后一次已经实际生效的 GRANDPA 公钥。
    #[pallet::storage]
    #[pallet::getter(fn current_grandpa_key)]
    pub type CurrentGrandpaKeys<T: Config> =
        StorageMap<_, Blake2_128Concat, CidNumber, [u8; 32], OptionQuery>;

    /// 已生效公钥到机构 CID 的唯一反向索引。
    #[pallet::storage]
    #[pallet::getter(fn key_owner)]
    pub type GrandpaKeyOwnerByKey<T: Config> =
        StorageMap<_, Blake2_128Concat, [u8; 32], CidNumber, OptionQuery>;

    /// 正在投票或等待生效的新公钥占用，防止并发提案复用同一公钥。
    #[pallet::storage]
    #[pallet::getter(fn reserved_key_owner)]
    pub type ReservedGrandpaKeys<T: Config> =
        StorageMap<_, Blake2_128Concat, [u8; 32], CidNumber, OptionQuery>;

    /// 每个机构下一份持钥证明必须使用的 nonce。
    #[pallet::storage]
    #[pallet::getter(fn next_proof_nonce)]
    pub type NextGrandpaKeyProofNonce<T: Config> =
        StorageMap<_, Blake2_128Concat, CidNumber, u64, ValueQuery>;

    /// 每个机构尚未终态的紧急恢复提案；正常更换不能越过它。
    #[pallet::storage]
    #[pallet::getter(fn active_emergency_recovery)]
    pub type ActiveEmergencyRecoveryByInstitution<T: Config> =
        StorageMap<_, Blake2_128Concat, CidNumber, u64, OptionQuery>;

    /// 全链唯一的已调度 GRANDPA 变更，与 `pallet-grandpa` 单 pending 限制一致。
    #[pallet::storage]
    #[pallet::getter(fn pending_grandpa_key_change)]
    pub type PendingGrandpaKeyChange<T: Config> =
        StorageValue<_, PendingGrandpaKeyChangeState<T::AccountId, BlockNumberFor<T>>, OptionQuery>;

    #[pallet::genesis_config]
    pub struct GenesisConfig<T: Config> {
        pub _phantom: core::marker::PhantomData<T>,
    }

    impl<T: Config> Default for GenesisConfig<T> {
        fn default() -> Self {
            Self {
                _phantom: Default::default(),
            }
        }
    }

    #[pallet::genesis_build]
    impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
        fn build(&self) {
            for node in CHINA_CB.iter() {
                let actor_cid_number: CidNumber = node
                    .cid_number
                    .as_bytes()
                    .to_vec()
                    .try_into()
                    .expect("genesis: CHINA_CB cid_number 超过长度上限");
                assert!(
                    !GrandpaKeyOwnerByKey::<T>::contains_key(node.grandpa_key),
                    "duplicated initial grandpa key in CHINA_CB"
                );
                CurrentGrandpaKeys::<T>::insert(actor_cid_number.clone(), node.grandpa_key);
                GrandpaKeyOwnerByKey::<T>::insert(node.grandpa_key, actor_cid_number);
            }
        }
    }

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
        fn on_initialize(_now: BlockNumberFor<T>) -> Weight {
            if Self::reconcile_pending_change() {
                T::DbWeight::get().reads_writes(7, 5)
            } else {
                T::DbWeight::get().reads(4)
            }
        }
    }

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        /// 已创建目标机构自己的委员内部投票紧急恢复提案。
        EmergencyRecoveryProposed {
            proposal_id: u64,
            institution_code: InstitutionCode,
            actor_cid_number: CidNumber,
            initiator_account_id: T::AccountId,
            old_public_key: [u8; 32],
            new_public_key: [u8; 32],
            proof_nonce: u64,
        },
        /// 正常更换已通过双密钥证明并调度延迟生效。
        RoutineRotationScheduled {
            actor_cid_number: CidNumber,
            initiator_account_id: T::AccountId,
            old_public_key: [u8; 32],
            new_public_key: [u8; 32],
            proof_nonce: u64,
            expected_set_id: u64,
            activate_at: BlockNumberFor<T>,
        },
        /// 紧急恢复内部投票通过，authority set 变更已调度。
        EmergencyRecoveryScheduled {
            proposal_id: u64,
            actor_cid_number: CidNumber,
            old_public_key: [u8; 32],
            new_public_key: [u8; 32],
            expected_set_id: u64,
            activate_at: BlockNumberFor<T>,
        },
        /// 新 authority 已在链上实际生效；节点可在该事件 finalized 后删除旧私钥。
        GrandpaKeyActivated {
            actor_cid_number: CidNumber,
            old_public_key: [u8; 32],
            new_public_key: [u8; 32],
            change_kind: GrandpaKeyChangeKind,
            expected_set_id: u64,
        },
        /// 紧急恢复提案已通过，但调度暂时失败，可由投票引擎重试。
        EmergencyRecoveryExecutionFailed { proposal_id: u64 },
        /// 被否决或确定不可执行的紧急恢复已释放公钥占用。
        EmergencyRecoveryClosed {
            proposal_id: u64,
            actor_cid_number: CidNumber,
        },
    }

    #[pallet::error]
    pub enum Error<T> {
        InvalidInstitution,
        UnauthorizedAdmin,
        ProposalActionNotFound,
        ProposalNotPassed,
        CurrentGrandpaKeyNotFound,
        NewKeyIsZero,
        InvalidEd25519Key,
        NewKeyUnchanged,
        NewKeyAlreadyUsed,
        NewKeyAlreadyReserved,
        OldAuthorityNotFound,
        GrandpaChangePending,
        EmergencyRecoveryAlreadyActive,
        EmergencyRecoveryNotActive,
        InvalidProofNonce,
        ProofExpired,
        InvalidOldKeySignature,
        InvalidNewKeySignature,
        ProofNonceOverflow,
        SetIdOverflow,
        ProposalStillExecutable,
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        /// 旧私钥不可用时，由目标机构委员发起本机构内部投票恢复。
        #[pallet::call_index(0)]
        #[pallet::weight(<T as Config>::WeightInfo::propose_emergency_grandpa_key_recovery())]
        pub fn propose_emergency_grandpa_key_recovery(
            origin: OriginFor<T>,
            actor_cid_number: CidNumber,
            actor_role_code: RoleCode,
            new_public_key: [u8; 32],
            proof_nonce: u64,
            proof_expires_at: BlockNumberFor<T>,
            new_public_key_signature: [u8; 64],
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            let institution_code =
                cid_org(actor_cid_number.as_slice()).ok_or(Error::<T>::InvalidInstitution)?;
            ensure!(
                ActiveEmergencyRecoveryByInstitution::<T>::get(actor_cid_number.clone()).is_none(),
                Error::<T>::EmergencyRecoveryAlreadyActive
            );
            Self::ensure_proof_window(&actor_cid_number, proof_nonce, proof_expires_at)?;
            let (old_public_key, _) =
                Self::current_key_and_next_authorities(&actor_cid_number, new_public_key, false)?;

            let proof_payload = proof::payload::<T>(
                actor_cid_number.clone(),
                actor_role_code.clone(),
                who.clone(),
                old_public_key,
                new_public_key,
                proof_nonce,
                proof_expires_at,
                GrandpaKeyChangeKind::EmergencyRecovery,
            );
            ensure!(
                proof::verify_signature(new_public_key, new_public_key_signature, &proof_payload),
                Error::<T>::InvalidNewKeySignature
            );

            let action = EmergencyGrandpaKeyRecoveryAction {
                actor_cid_number: actor_cid_number.clone(),
                actor_role_code: actor_role_code.clone(),
                initiator_account_id: who.clone(),
                old_public_key,
                new_public_key,
                proof_nonce,
                proof_expires_at,
            };
            let mut encoded_action = Vec::from(crate::MODULE_TAG);
            encoded_action.extend_from_slice(&action.encode());
            let vote_plan = Self::build_emergency_vote_plan(
                &who,
                actor_cid_number.as_slice(),
                actor_role_code.as_slice(),
                &encoded_action,
            )?;
            let proposal_id = T::InternalVoteEngine::create_institution_proposal_with_data(
                who.clone(),
                institution_code,
                actor_cid_number.to_vec(),
                None,
                Vec::from([actor_cid_number.to_vec()]),
                vote_plan,
                encoded_action,
            )?;

            ActiveEmergencyRecoveryByInstitution::<T>::insert(
                actor_cid_number.clone(),
                proposal_id,
            );
            ReservedGrandpaKeys::<T>::insert(new_public_key, actor_cid_number.clone());
            Self::consume_proof_nonce(&actor_cid_number, proof_nonce)?;
            Self::deposit_event(Event::<T>::EmergencyRecoveryProposed {
                proposal_id,
                institution_code,
                actor_cid_number,
                initiator_account_id: who,
                old_public_key,
                new_public_key,
                proof_nonce,
            });
            Ok(())
        }

        /// 旧、新 GRANDPA 私钥都可用时，由目标机构单个委员直接调度正常更换。
        #[pallet::call_index(1)]
        #[pallet::weight(<T as Config>::WeightInfo::schedule_grandpa_key_rotation())]
        pub fn schedule_grandpa_key_rotation(
            origin: OriginFor<T>,
            actor_cid_number: CidNumber,
            actor_role_code: RoleCode,
            new_public_key: [u8; 32],
            proof_nonce: u64,
            proof_expires_at: BlockNumberFor<T>,
            old_public_key_signature: [u8; 64],
            new_public_key_signature: [u8; 64],
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            cid_org(actor_cid_number.as_slice()).ok_or(Error::<T>::InvalidInstitution)?;
            ensure!(
                ActiveEmergencyRecoveryByInstitution::<T>::get(actor_cid_number.clone()).is_none(),
                Error::<T>::EmergencyRecoveryAlreadyActive
            );
            Self::authorize_routine_rotation(
                &who,
                actor_cid_number.as_slice(),
                actor_role_code.as_slice(),
            )?;
            Self::ensure_proof_window(&actor_cid_number, proof_nonce, proof_expires_at)?;
            let (old_public_key, next_authorities) =
                Self::current_key_and_next_authorities(&actor_cid_number, new_public_key, false)?;
            let proof_payload = proof::payload::<T>(
                actor_cid_number.clone(),
                actor_role_code,
                who.clone(),
                old_public_key,
                new_public_key,
                proof_nonce,
                proof_expires_at,
                GrandpaKeyChangeKind::RoutineRotation,
            );
            ensure!(
                proof::verify_signature(old_public_key, old_public_key_signature, &proof_payload),
                Error::<T>::InvalidOldKeySignature
            );
            ensure!(
                proof::verify_signature(new_public_key, new_public_key_signature, &proof_payload),
                Error::<T>::InvalidNewKeySignature
            );

            let pending = Self::schedule_replacement(
                actor_cid_number.clone(),
                who.clone(),
                old_public_key,
                new_public_key,
                proof_nonce,
                GrandpaKeyChangeKind::RoutineRotation,
                None,
                next_authorities,
            )?;
            Self::consume_proof_nonce(&actor_cid_number, proof_nonce)?;
            Self::deposit_event(Event::<T>::RoutineRotationScheduled {
                actor_cid_number,
                initiator_account_id: who,
                old_public_key,
                new_public_key,
                proof_nonce,
                expected_set_id: pending.expected_set_id,
                activate_at: pending.activate_at,
            });
            Ok(())
        }
    }

    impl<T: Config> Pallet<T> {
        fn ensure_proof_window(
            actor_cid_number: &CidNumber,
            proof_nonce: u64,
            proof_expires_at: BlockNumberFor<T>,
        ) -> Result<(), Error<T>> {
            ensure!(
                NextGrandpaKeyProofNonce::<T>::get(actor_cid_number.clone()) == proof_nonce,
                Error::<T>::InvalidProofNonce
            );
            ensure!(
                frame_system::Pallet::<T>::block_number() <= proof_expires_at,
                Error::<T>::ProofExpired
            );
            ensure!(
                proof_nonce.checked_add(1).is_some(),
                Error::<T>::ProofNonceOverflow
            );
            Ok(())
        }

        fn consume_proof_nonce(
            actor_cid_number: &CidNumber,
            proof_nonce: u64,
        ) -> Result<(), Error<T>> {
            let next = proof_nonce
                .checked_add(1)
                .ok_or(Error::<T>::ProofNonceOverflow)?;
            NextGrandpaKeyProofNonce::<T>::insert(actor_cid_number.clone(), next);
            Ok(())
        }

        pub(crate) fn cleanup_recovery(
            proposal_id: u64,
            action: &EmergencyGrandpaKeyRecoveryAction<T::AccountId, BlockNumberFor<T>>,
        ) {
            if ActiveEmergencyRecoveryByInstitution::<T>::get(action.actor_cid_number.clone())
                == Some(proposal_id)
            {
                ActiveEmergencyRecoveryByInstitution::<T>::remove(action.actor_cid_number.clone());
            }
            if ReservedGrandpaKeys::<T>::get(action.new_public_key)
                == Some(action.actor_cid_number.clone())
            {
                ReservedGrandpaKeys::<T>::remove(action.new_public_key);
            }
        }

        pub(crate) fn validate_emergency_action(
            proposal_id: u64,
            action: &EmergencyGrandpaKeyRecoveryAction<T::AccountId, BlockNumberFor<T>>,
        ) -> Result<Vec<(sp_consensus_grandpa::AuthorityId, u64)>, Error<T>> {
            ensure!(
                ActiveEmergencyRecoveryByInstitution::<T>::get(action.actor_cid_number.clone())
                    == Some(proposal_id),
                Error::<T>::EmergencyRecoveryNotActive
            );
            ensure!(
                ReservedGrandpaKeys::<T>::get(action.new_public_key)
                    == Some(action.actor_cid_number.clone()),
                Error::<T>::EmergencyRecoveryNotActive
            );
            let (current_public_key, next_authorities) = Self::current_key_and_next_authorities(
                &action.actor_cid_number,
                action.new_public_key,
                true,
            )?;
            ensure!(
                current_public_key == action.old_public_key,
                Error::<T>::OldAuthorityNotFound
            );
            use entity_primitives::{InstitutionRoleAuthorizationQuery, RolePermissionOperation};
            let subject = entity_primitives::RoleSubject {
                cid_number: action.actor_cid_number.to_vec(),
                role_code: action.actor_role_code.to_vec(),
            };
            let action_id = entity_primitives::BusinessActionId {
                module_tag: crate::MODULE_TAG.to_vec(),
                action_code:
                    entity_primitives::business_action::ACTION_GRANDPA_KEY_EMERGENCY_RECOVERY,
            };
            ensure!(
                T::InstitutionRoleAuthorization::is_authorized(
                    &action.initiator_account_id,
                    &subject,
                    &action_id,
                    RolePermissionOperation::Propose,
                ),
                Error::<T>::UnauthorizedAdmin
            );
            Ok(next_authorities)
        }

        pub(crate) fn try_execute_emergency_action(
            proposal_id: u64,
            action: EmergencyGrandpaKeyRecoveryAction<T::AccountId, BlockNumberFor<T>>,
        ) -> DispatchResult {
            let proposal = votingengine::Pallet::<T>::proposals(proposal_id)
                .ok_or(Error::<T>::ProposalActionNotFound)?;
            let institution_code = cid_org(action.actor_cid_number.as_slice())
                .ok_or(Error::<T>::InvalidInstitution)?;
            ensure!(
                votingengine::Pallet::<T>::is_callback_execution_scope(proposal_id)
                    && votingengine::Pallet::<T>::is_proposal_owner(proposal_id, crate::MODULE_TAG,)
                    && proposal.kind == PROPOSAL_KIND_INTERNAL
                    && proposal.stage == STAGE_INTERNAL
                    && proposal.status == STATUS_PASSED
                    && proposal.internal_code == Some(institution_code)
                    && proposal
                        .actor_cid_number
                        .as_ref()
                        .map(|value| value.as_slice())
                        == Some(action.actor_cid_number.as_slice())
                    && proposal.execution_account_id.is_none(),
                Error::<T>::ProposalNotPassed
            );

            let next_authorities = Self::validate_emergency_action(proposal_id, &action)?;
            let pending = Self::schedule_replacement(
                action.actor_cid_number.clone(),
                action.initiator_account_id,
                action.old_public_key,
                action.new_public_key,
                action.proof_nonce,
                GrandpaKeyChangeKind::EmergencyRecovery,
                Some(proposal_id),
                next_authorities,
            )?;
            ActiveEmergencyRecoveryByInstitution::<T>::remove(action.actor_cid_number.clone());
            Self::deposit_event(Event::<T>::EmergencyRecoveryScheduled {
                proposal_id,
                actor_cid_number: action.actor_cid_number,
                old_public_key: action.old_public_key,
                new_public_key: action.new_public_key,
                expected_set_id: pending.expected_set_id,
                activate_at: pending.activate_at,
            });
            Ok(())
        }
    }
}

/// 紧急恢复内部投票终态回调；投票本身全部由 votingengine/internal-vote 负责。
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
        let action = EmergencyGrandpaKeyRecoveryAction::<T::AccountId, BlockNumberFor<T>>::decode(
            &mut &raw[crate::MODULE_TAG.len()..],
        )
        .map_err(|_| pallet::Error::<T>::ProposalActionNotFound)?;

        if !approved {
            pallet::Pallet::<T>::cleanup_recovery(proposal_id, &action);
            pallet::Pallet::<T>::deposit_event(pallet::Event::<T>::EmergencyRecoveryClosed {
                proposal_id,
                actor_cid_number: action.actor_cid_number,
            });
            return Ok(ProposalExecutionOutcome::Executed);
        }

        match pallet::Pallet::<T>::try_execute_emergency_action(proposal_id, action.clone()) {
            Ok(()) => Ok(ProposalExecutionOutcome::Executed),
            Err(error) if error == pallet::Error::<T>::GrandpaChangePending.into() => {
                pallet::Pallet::<T>::deposit_event(
                    pallet::Event::<T>::EmergencyRecoveryExecutionFailed { proposal_id },
                );
                Ok(ProposalExecutionOutcome::RetryableFailed)
            }
            Err(_) => {
                pallet::Pallet::<T>::cleanup_recovery(proposal_id, &action);
                pallet::Pallet::<T>::deposit_event(
                    pallet::Event::<T>::EmergencyRecoveryExecutionFailed { proposal_id },
                );
                Ok(ProposalExecutionOutcome::FatalFailed)
            }
        }
    }

    fn can_cancel_passed_proposal(
        proposal_id: u64,
    ) -> Result<ProposalCancelDecision, sp_runtime::DispatchError> {
        let raw = match votingengine::Pallet::<T>::get_proposal_data(proposal_id) {
            Some(raw)
                if votingengine::Pallet::<T>::is_proposal_owner(proposal_id, crate::MODULE_TAG)
                    && raw.starts_with(crate::MODULE_TAG) =>
            {
                raw
            }
            _ => return Ok(ProposalCancelDecision::Ignored),
        };
        let action = EmergencyGrandpaKeyRecoveryAction::<T::AccountId, BlockNumberFor<T>>::decode(
            &mut &raw[crate::MODULE_TAG.len()..],
        )
        .map_err(|_| pallet::Error::<T>::ProposalActionNotFound)?;
        match pallet::Pallet::<T>::validate_emergency_action(proposal_id, &action) {
            Ok(_) => Err(pallet::Error::<T>::ProposalStillExecutable.into()),
            Err(pallet::Error::<T>::GrandpaChangePending) => {
                Err(pallet::Error::<T>::GrandpaChangePending.into())
            }
            Err(_) => {
                pallet::Pallet::<T>::cleanup_recovery(proposal_id, &action);
                pallet::Pallet::<T>::deposit_event(pallet::Event::<T>::EmergencyRecoveryClosed {
                    proposal_id,
                    actor_cid_number: action.actor_cid_number,
                });
                Ok(ProposalCancelDecision::Allow)
            }
        }
    }
}

#[cfg(test)]
mod tests;
