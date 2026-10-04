#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(feature = "runtime-benchmarks")]
mod benchmarks;
pub mod weights;

use frame_support::pallet_prelude::DispatchResult;
pub use pallet::*;
use votingengine::JointVoteResultCallback;

/// 模块标识前缀，用于在 ProposalData 中区分不同业务模块，防止跨模块误解码。
pub const MODULE_TAG: &[u8] = b"rt-upg";

pub trait RuntimeCodeExecutor {
    /// 由 Runtime 原子暂存 PoW 参数并执行 set_code。
    fn execute_runtime_code(
        code: &[u8],
        pow_params: pow_difficulty::PowDifficultyParams,
        activate_at: u32,
    ) -> DispatchResult;
}

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use entity_primitives::{
        AuthorizationSubject, BusinessActionId, InstitutionRoleAuthorizationQuery,
        RolePermissionOperation, RoleSubject,
    };
    use frame_support::pallet_prelude::*;
    use frame_system::pallet_prelude::*;
    use genesis_pallet::DeveloperUpgradeCheck;
    use primitives::{
        cid::{china::china_cb::CHINA_CB, china::china_ch::CHINA_CH},
        governance_skeleton::{ROLE_CODE_COMMITTEE_MEMBER, ROLE_CODE_DIRECTOR},
    };
    use sp_runtime::{
        traits::{Hash, SaturatedConversion},
        DispatchError,
    };
    use votingengine::JointVoteEngine;

    pub type ReasonOf<T> = BoundedVec<u8, <T as Config>::MaxReasonLen>;
    pub type CodeOf<T> = BoundedVec<u8, <T as Config>::MaxRuntimeCodeSize>;
    pub const PROPOSAL_OBJECT_KIND_RUNTIME_WASM: u8 = 1;

    #[derive(
        Encode,
        Decode,
        DecodeWithMemTracking,
        Clone,
        Copy,
        Debug,
        TypeInfo,
        MaxEncodedLen,
        PartialEq,
        Eq,
    )]
    pub enum UpgradeExecutionPath {
        JointVote,
        DeveloperDirect,
    }

    #[derive(
        Encode, Decode, DecodeWithMemTracking, Clone, Debug, TypeInfo, MaxEncodedLen, PartialEq, Eq,
    )]
    #[scale_info(skip_type_params(T))]
    pub struct RuntimeUpgradeAudit<T: Config> {
        pub proposal_id: Option<u64>,
        pub execution_path: UpgradeExecutionPath,
        pub code_hash: T::Hash,
        pub old_pow_params_hash: T::Hash,
        pub new_pow_params_hash: T::Hash,
        pub executed_at: u32,
        pub activate_at: u32,
        pub developer: Option<T::AccountId>,
    }

    /// 提案摘要数据：序列化后存入 votingengine 的 ProposalData。
    /// 大对象 wasm code 单独写入 votingengine 的 ProposalObject。
    #[derive(
        Encode, Decode, DecodeWithMemTracking, Clone, Debug, TypeInfo, MaxEncodedLen, PartialEq, Eq,
    )]
    #[scale_info(skip_type_params(T))]
    pub struct Proposal<T: Config> {
        pub actor_cid_number: votingengine::types::CidNumber,
        /// 提案发起岗位码；与机构 CID、签名账户共同形成完整授权主体。
        pub actor_role_code: votingengine::types::RoleCode,
        /// 提案发起人（国家储委会或省储委会对应岗位的任职管理员）
        pub proposer_account_id: T::AccountId,
        /// 升级理由
        pub reason: ReasonOf<T>,
        /// 代码哈希，便于事件与链下审计对齐
        pub code_hash: T::Hash,
        /// 提案创建时生效参数的哈希，防止投票期间参数基线被替换。
        pub expected_pow_params_hash: T::Hash,
        /// 与 runtime code 一起表决的完整 PoW 参数。
        pub new_pow_params: pow_difficulty::PowDifficultyParams,
    }

    use crate::weights::WeightInfo;

    #[pallet::config]
    pub trait Config: frame_system::Config + votingengine::Config + pow_difficulty::Config {
        #[allow(deprecated)]
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

        /// 只完成签名来源校验；协议升级业务权限随后按机构 CID + 委员岗位校验。
        type ProposeOrigin: EnsureOrigin<Self::RuntimeOrigin, Success = Self::AccountId>;

        /// 开发期直接升级权限：只允许国家储委会管理员绕过投票执行 set_code。
        type DeveloperUpgradeOrigin: EnsureOrigin<Self::RuntimeOrigin, Success = Self::AccountId>;

        type JointVoteEngine: JointVoteEngine<Self::AccountId>;
        /// 协议升级业务按“机构 CID + 委员岗位”校验提案权限。
        type InstitutionRoleAuthorization: InstitutionRoleAuthorizationQuery<Self::AccountId>;
        type RuntimeCodeExecutor: RuntimeCodeExecutor;

        /// 开发者直升 runtime 开关检查（由 genesis_pallet-pallet 注入）。
        type DeveloperUpgradeCheck: genesis_pallet::DeveloperUpgradeCheck;

        #[pallet::constant]
        type MaxReasonLen: Get<u32>;

        #[pallet::constant]
        type MaxRuntimeCodeSize: Get<u32>;

        type WeightInfo: crate::weights::WeightInfo;
    }

    #[pallet::pallet]
    pub struct Pallet<T>(_);

    /// 最近一次成功升级的永久审计；NodeGuard 用它把 :code 与 PoW 参数原子绑定。
    #[pallet::storage]
    pub type LastRuntimeUpgradeAudit<T: Config> =
        StorageValue<_, RuntimeUpgradeAudit<T>, OptionQuery>;

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        RuntimeUpgradeProposed {
            proposal_id: u64,
            actor_cid_number: votingengine::types::CidNumber,
            actor_role_code: votingengine::types::RoleCode,
            proposer_account_id: T::AccountId,
            code_hash: T::Hash,
            pow_params_hash: T::Hash,
        },
        JointVoteFinalized {
            proposal_id: u64,
            approved: bool,
        },
        RuntimeUpgradeExecuted {
            proposal_id: u64,
            code_hash: T::Hash,
        },
        RuntimeUpgradeExecutionFailed {
            proposal_id: u64,
            code_hash: T::Hash,
        },
        /// 开发期直接升级成功（不走投票）。
        DeveloperDirectUpgradeExecuted {
            who: T::AccountId,
            code_hash: T::Hash,
        },
    }

    #[pallet::error]
    pub enum Error<T> {
        EmptyReason,
        EmptyRuntimeCode,
        InvalidActorCid,
        UnauthorizedActorAdmin,
        /// 发起人没有目标机构委员岗位的协议升级提案权限。
        UnauthorizedActorRole,
        ProposalNotFound,
        ProposalNotVoting,
        JointVoteCreateFailed,
        RuntimeCodeMissing,
        /// 开发者直升已关闭（链已进入运行期）。
        DeveloperUpgradeDisabled,
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        /// NRC/PRC 委员岗位任职人发起 runtime 升级提案，升级流程走联合投票。
        /// 本模块只提交协议升级业务内容；人口快照、联合签名、
        /// 投票资格和计票流程全部由 votingengine 负责。
        #[pallet::call_index(0)]
        #[pallet::weight(<T as Config>::WeightInfo::propose_runtime_upgrade())]
        pub fn propose_runtime_upgrade(
            origin: OriginFor<T>,
            actor_cid_number: votingengine::types::CidNumber,
            actor_role_code: votingengine::types::RoleCode,
            reason: ReasonOf<T>,
            code: CodeOf<T>,
            new_pow_params: pow_difficulty::PowDifficultyParams,
        ) -> DispatchResult {
            let proposer_account_id = T::ProposeOrigin::ensure_origin(origin)?;

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
                role_code: actor_role_code.to_vec(),
            };
            let business_action = BusinessActionId {
                module_tag: crate::MODULE_TAG.to_vec(),
                action_code: entity_primitives::business_action::ACTION_RUNTIME_UPGRADE,
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

            ensure!(!reason.is_empty(), Error::<T>::EmptyReason);
            ensure!(!code.is_empty(), Error::<T>::EmptyRuntimeCode);
            new_pow_params
                .validate()
                .map_err(|_| DispatchError::Other("invalid pow difficulty params"))?;

            let code_vec = code.into_inner();
            let code_hash = T::Hashing::hash(code_vec.as_slice());
            let mut business_object_hash = [0u8; 32];
            business_object_hash.copy_from_slice(code_hash.as_ref());
            let expected_pow_params_hash =
                T::Hashing::hash_of(&pow_difficulty::ActiveParams::<T>::get());
            let pow_params_hash = T::Hashing::hash_of(&new_pow_params);
            let proposal = Proposal::<T> {
                actor_cid_number: actor_cid_number.clone(),
                actor_role_code: actor_role_code.clone(),
                proposer_account_id: proposer_account_id.clone(),
                reason,
                code_hash,
                expected_pow_params_hash,
                new_pow_params,
            };
            let mut encoded = sp_runtime::sp_std::vec::Vec::from(crate::MODULE_TAG);
            encoded.extend_from_slice(&proposal.encode());
            let vote_plan = Self::build_vote_plan(&actor_cid_number, business_object_hash)?;
            let proposal_id = T::JointVoteEngine::create_joint_proposal_with_data_and_object(
                proposer_account_id.clone(),
                actor_cid_number.to_vec(),
                vote_plan,
                encoded,
                PROPOSAL_OBJECT_KIND_RUNTIME_WASM,
                code_vec,
            )
            .map_err(|_| Error::<T>::JointVoteCreateFailed)?;

            Self::deposit_event(Event::<T>::RuntimeUpgradeProposed {
                proposal_id,
                actor_cid_number,
                actor_role_code,
                proposer_account_id,
                code_hash,
                pow_params_hash,
            });
            Ok(())
        }

        /// 开发期快捷通道：仅国家储委会委员岗位任职人可直接 set_code，不走投票。
        /// 仅在 genesis_pallet-pallet 的 DeveloperUpgradeEnabled 为 true 时可用。
        /// 链进入运行期后此调用永久失效，升级必须走 propose_runtime_upgrade 联合投票。
        #[pallet::call_index(2)]
        #[pallet::weight(
            <<T as frame_system::Config>::SystemWeightInfo as frame_system::weights::WeightInfo>::set_code()
        )]
        pub fn developer_direct_upgrade(
            origin: OriginFor<T>,
            actor_cid_number: votingengine::types::CidNumber,
            actor_role_code: votingengine::types::RoleCode,
            code: CodeOf<T>,
            new_pow_params: pow_difficulty::PowDifficultyParams,
        ) -> DispatchResult {
            let who = T::DeveloperUpgradeOrigin::ensure_origin(origin)?;
            let actor_text = core::str::from_utf8(actor_cid_number.as_slice())
                .map_err(|_| Error::<T>::InvalidActorCid)?;
            let actor_code = votingengine::types::institution_code_from_cid_number(actor_text)
                .ok_or(Error::<T>::InvalidActorCid)?;
            ensure!(
                actor_code == votingengine::types::NRC,
                Error::<T>::InvalidActorCid
            );
            let role_subject = RoleSubject {
                cid_number: actor_cid_number.to_vec(),
                role_code: actor_role_code.to_vec(),
            };
            ensure!(
                T::InstitutionRoleAuthorization::is_authorized(
                    &who,
                    &role_subject,
                    &BusinessActionId {
                        module_tag: crate::MODULE_TAG.to_vec(),
                        action_code: entity_primitives::business_action::ACTION_RUNTIME_UPGRADE,
                    },
                    RolePermissionOperation::Propose,
                ),
                Error::<T>::UnauthorizedActorRole
            );
            ensure!(
                T::DeveloperUpgradeCheck::is_enabled(),
                Error::<T>::DeveloperUpgradeDisabled
            );
            ensure!(!code.is_empty(), Error::<T>::EmptyRuntimeCode);
            new_pow_params
                .validate()
                .map_err(|_| DispatchError::Other("invalid pow difficulty params"))?;
            let code_hash = T::Hashing::hash(code.as_slice());
            Self::execute_upgrade_bundle(
                code.as_slice(),
                new_pow_params,
                None,
                UpgradeExecutionPath::DeveloperDirect,
                Some(who.clone()),
            )?;
            Self::deposit_event(Event::<T>::DeveloperDirectUpgradeExecuted { who, code_hash });
            Ok(())
        }

        // call_index = 1 保持空缺：联合投票终结只能由 votingengine 回调进入，
        // 不再暴露 Root 手工回放 extrinsic，避免形成第二条执行入口。
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

        /// 协议升级固定使用 NRC/PRC 委员与 PRB 董事组成的联合投票计划。
        fn build_vote_plan(
            actor_cid_number: &votingengine::types::CidNumber,
            business_object_hash: [u8; 32],
        ) -> Result<votingengine::types::VotePlanOf<T::AccountId>, DispatchError> {
            let proposer_role = Self::bounded_role_subject(
                actor_cid_number.as_slice(),
                ROLE_CODE_COMMITTEE_MEMBER,
            )?;
            let mut voters = sp_runtime::sp_std::vec::Vec::new();
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
                    action_code: entity_primitives::business_action::ACTION_RUNTIME_UPGRADE,
                },
                module_tag,
                AuthorizationSubject::Institution(proposer_role),
                voters,
                votingengine::types::VotingEngineKind::Joint,
                business_object_hash,
            )
            .map_err(|_| Error::<T>::JointVoteCreateFailed.into())
        }

        #[frame_support::transactional]
        fn execute_upgrade_bundle(
            code: &[u8],
            new_pow_params: pow_difficulty::PowDifficultyParams,
            proposal_id: Option<u64>,
            execution_path: UpgradeExecutionPath,
            developer: Option<T::AccountId>,
        ) -> DispatchResult {
            let active = pow_difficulty::ActiveParams::<T>::get();
            let old_pow_params_hash = T::Hashing::hash_of(&active);
            let new_pow_params_hash = T::Hashing::hash_of(&new_pow_params);
            let executed_at: u32 = frame_system::Pallet::<T>::block_number().saturated_into();
            let activate_at = executed_at
                .checked_add(1)
                .ok_or(DispatchError::Other("pow params activation overflow"))?;

            T::RuntimeCodeExecutor::execute_runtime_code(code, new_pow_params, activate_at)?;
            LastRuntimeUpgradeAudit::<T>::put(RuntimeUpgradeAudit::<T> {
                proposal_id,
                execution_path,
                code_hash: T::Hashing::hash(code),
                old_pow_params_hash,
                new_pow_params_hash,
                executed_at,
                activate_at,
                developer,
            });
            Ok(())
        }

        /// 快速判断 proposal_id 是否属于本模块（通过 ProposalOwner 匹配）。
        pub fn owns_proposal(proposal_id: u64) -> bool {
            votingengine::Pallet::<T>::is_proposal_owner(proposal_id, crate::MODULE_TAG)
        }

        /// 从投票引擎 ProposalData 中读取并解码本模块的提案摘要。
        /// 先校验 MODULE_TAG 前缀，防止跨模块误解码。
        pub(crate) fn load_proposal(proposal_id: u64) -> Result<Proposal<T>, DispatchError> {
            let raw = votingengine::Pallet::<T>::get_proposal_data(proposal_id)
                .ok_or(Error::<T>::ProposalNotFound)?;
            let tag = crate::MODULE_TAG;
            if raw.len() < tag.len() || &raw[..tag.len()] != tag {
                return Err(Error::<T>::ProposalNotFound.into());
            }
            Proposal::<T>::decode(&mut &raw[tag.len()..])
                .map_err(|_| Error::<T>::ProposalNotFound.into())
        }

        fn load_runtime_code(proposal_id: u64) -> Result<CodeOf<T>, DispatchError> {
            let meta = votingengine::Pallet::<T>::get_proposal_object_meta(proposal_id)
                .ok_or(Error::<T>::RuntimeCodeMissing)?;
            ensure!(
                meta.kind == PROPOSAL_OBJECT_KIND_RUNTIME_WASM,
                Error::<T>::RuntimeCodeMissing
            );
            let raw = votingengine::Pallet::<T>::get_proposal_object(proposal_id)
                .ok_or(Error::<T>::RuntimeCodeMissing)?;
            raw.try_into()
                .map_err(|_| Error::<T>::RuntimeCodeMissing.into())
        }

        /// 联合投票结果回调（由 votingengine 的 set_status_and_emit 在事务内调用）。
        ///
        /// 状态处理模式与 votingengine 对齐：
        /// - approved + 执行成功 → 返回 `Executed`，由投票引擎写 STATUS_EXECUTED。
        /// - approved + 执行失败 → 返回 `FatalFailed`，由投票引擎写 STATUS_EXECUTION_FAILED。
        /// - rejected → 返回 `Executed`，投票引擎保留 STATUS_REJECTED。
        pub(crate) fn apply_joint_vote_result(
            proposal_id: u64,
            approved: bool,
        ) -> Result<votingengine::ProposalExecutionOutcome, DispatchError> {
            let proposal = Self::load_proposal(proposal_id)?;
            let engine_proposal = votingengine::Pallet::<T>::proposals(proposal_id)
                .ok_or(Error::<T>::ProposalNotFound)?;
            ensure!(
                votingengine::Pallet::<T>::is_callback_execution_scope(proposal_id)
                    && votingengine::Pallet::<T>::is_proposal_owner(proposal_id, crate::MODULE_TAG,)
                    && engine_proposal.kind == votingengine::PROPOSAL_KIND_JOINT
                    && matches!(
                        engine_proposal.stage,
                        votingengine::STAGE_JOINT | votingengine::STAGE_REFERENDUM
                    ),
                Error::<T>::ProposalNotVoting
            );
            let expected_status = if approved {
                votingengine::STATUS_PASSED
            } else {
                votingengine::STATUS_REJECTED
            };
            ensure!(
                engine_proposal.status == expected_status,
                Error::<T>::ProposalNotVoting
            );

            if approved {
                let code_to_execute = Self::load_runtime_code(proposal_id)?;
                ensure!(
                    T::Hashing::hash(code_to_execute.as_slice()) == proposal.code_hash,
                    Error::<T>::RuntimeCodeMissing
                );
                let current_params_hash =
                    T::Hashing::hash_of(&pow_difficulty::ActiveParams::<T>::get());
                ensure!(
                    current_params_hash == proposal.expected_pow_params_hash,
                    DispatchError::Other("pow params changed while upgrade vote was open")
                );
                let exec_ok = Self::execute_upgrade_bundle(
                    code_to_execute.as_slice(),
                    proposal.new_pow_params,
                    Some(proposal_id),
                    UpgradeExecutionPath::JointVote,
                    None,
                )
                .is_ok();

                Self::deposit_event(Event::<T>::JointVoteFinalized {
                    proposal_id,
                    approved: true,
                });
                if exec_ok {
                    Self::deposit_event(Event::<T>::RuntimeUpgradeExecuted {
                        proposal_id,
                        code_hash: proposal.code_hash,
                    });
                } else {
                    Self::deposit_event(Event::<T>::RuntimeUpgradeExecutionFailed {
                        proposal_id,
                        code_hash: proposal.code_hash,
                    });
                }
                Ok(if exec_ok {
                    votingengine::ProposalExecutionOutcome::Executed
                } else {
                    votingengine::ProposalExecutionOutcome::FatalFailed
                })
            } else {
                Self::deposit_event(Event::<T>::JointVoteFinalized {
                    proposal_id,
                    approved: false,
                });
                Ok(votingengine::ProposalExecutionOutcome::Executed)
            }
        }
    }
}

impl<T: pallet::Config> JointVoteResultCallback for pallet::Pallet<T> {
    fn on_joint_vote_finalized(
        vote_proposal_id: u64,
        approved: bool,
    ) -> Result<votingengine::ProposalExecutionOutcome, sp_runtime::DispatchError> {
        // 统一使用 voting engine 的 proposal_id，不再需要反查映射。
        pallet::Pallet::<T>::apply_joint_vote_result(vote_proposal_id, approved)
    }
}

#[cfg(test)]
mod tests;
