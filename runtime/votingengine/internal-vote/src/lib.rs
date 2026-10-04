//! # 内部投票 pallet (internal-vote)
//!
//! 所有机构与个人多签共用的内部投票程序：机构按岗位票据，个人多签按
//! 独立管理员快照。
//!
//! 本模块负责内部投票模式准入、机构上下文、资格快照、计票和终态，不判断
//! 某个机构能否发起转账、销毁或密钥变更等具体业务。有效准入必须同时通过
//! 投票引擎的模式校验与调用方业务 pallet 的业务权限校验。
//!
//! 共用基础设施(Proposals 主 storage / 双层 ID / 反向索引 / 状态机骨架 / 快照 / 锁 / 清理)
//! 仍归 [`votingengine`] 引擎核心,本 pallet 通过 `Config: votingengine::Config` 直接访问。
//!
//! 本 pallet 自有:
//! - storage:`InternalVotesByTicket` / `InternalTallies` / `InternalThresholdSnapshot`
//! - event:`InternalVoteCast`
//! - error:`InvalidInternalCode` / `MissingThresholdSnapshot` / `InvalidThresholdSnapshot`
//! - extrinsic:`cast(proposal_id, ticket_claim, approve)`
//! - 业务函数:`do_create_internal_proposal*` / `do_internal_vote` / `do_finalize_internal_timeout`
//! - trait impl:`InternalVoteEngine`(供业务 pallet 创建提案)
//! - trait impl:`InternalProposalFinalizer`(votingengine 主 pallet finalize 路径反向调用)
//! - trait impl:`InternalCleanupHandler`(votingengine 主 pallet cleanup 状态机反向调用)

#![cfg_attr(not(feature = "std"), no_std)]

use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::{
    ensure,
    pallet_prelude::{BoundedVec, DecodeWithMemTracking, DispatchResult},
    storage::{with_transaction, TransactionOutcome},
};
use scale_info::TypeInfo;
use sp_runtime::traits::{SaturatedConversion, Saturating};
use sp_runtime::{Debug, DispatchError};

use primitives::count_const::VOTING_DURATION_BLOCKS;

use votingengine::{
    pallet::{AdminSnapshot, Proposals},
    types::{
        institution_code_from_cid_number, is_personal_code, is_valid_governance_code,
        AuthorizationSubject, CidNumber, InstitutionCode, InstitutionVoteTicket, ProposalSubject,
        ProposalSubjectCidNumbers, RoleCode, VotePlanOf, VotingEngineKind,
    },
    InternalAdminProvider, InternalProposalMutexKind, Proposal, PROPOSAL_KIND_INTERNAL,
    STAGE_INTERNAL, STATUS_EXECUTED, STATUS_EXECUTION_FAILED, STATUS_PASSED, STATUS_REJECTED,
};

pub mod weights;

mod cleanup;
mod proposal;
mod threshold;
mod vote;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarks;

pub use pallet::*;

#[cfg(test)]
mod tests;

/// 内部提案语义分类。
///
/// 这是投票引擎内部状态，不是业务模块自定义类型；用于在业务执行成功后
/// 激活/删除动态阈值，避免业务模块自己维护投票阈值。
#[derive(Encode, Decode, Clone, Copy, Debug, TypeInfo, MaxEncodedLen, PartialEq, Eq)]
pub enum InternalProposalRole {
    General,
    PersonalCreate,
    PersonalClose,
    PersonalAdminChange,
    InstitutionAdminChange,
}

/// 调用方声明本次使用个人管理员票，或某个机构岗位票。
#[derive(
    Encode, Decode, DecodeWithMemTracking, Clone, Debug, TypeInfo, MaxEncodedLen, PartialEq, Eq,
)]
pub enum InternalVoteTicketClaim {
    Personal,
    InstitutionRole(RoleCode),
}

/// 内部投票防重键。个人多签仍是一账户一票，机构按完整岗位票据记票。
#[derive(
    Encode, Decode, DecodeWithMemTracking, Clone, Debug, TypeInfo, MaxEncodedLen, PartialEq, Eq,
)]
pub enum InternalVoteTicket<AccountId> {
    Personal(AccountId),
    Institution(InstitutionVoteTicket<AccountId>),
}

#[derive(Encode, Decode, Clone, Copy, Debug, TypeInfo, MaxEncodedLen, PartialEq, Eq)]
pub struct PendingPersonalAdminChangeThreshold<AccountId> {
    pub personal_account_id: AccountId,
    pub new_admins_len: u32,
    pub new_threshold: u32,
}

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use frame_support::pallet_prelude::*;
    use frame_system::pallet_prelude::*;

    /// 重新创世直接使用最终 proposal_id 键控布局，不保留开发期旧存储迁移。
    pub const STORAGE_VERSION: StorageVersion = StorageVersion::new(0);

    #[pallet::config]
    pub trait Config: frame_system::Config + votingengine::Config {
        #[allow(deprecated)]
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
        /// 只读取岗位有效任职；业务权限已由业务 pallet 前置校验。
        type InstitutionRoleProvider: votingengine::InstitutionRoleProvider<Self::AccountId>;
        type WeightInfo: crate::weights::WeightInfo;
    }

    #[pallet::pallet]
    #[pallet::storage_version(STORAGE_VERSION)]
    pub struct Pallet<T>(_);

    /// 内部投票记录：个人多签按账户，机构按 CID + 岗位码 + 账户防重。
    #[pallet::storage]
    pub type InternalVotesByTicket<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        u64,
        Blake2_128Concat,
        InternalVoteTicket<T::AccountId>,
        bool,
        OptionQuery,
    >;

    #[pallet::storage]
    #[pallet::getter(fn internal_tally)]
    pub type InternalTallies<T: Config> =
        StorageMap<_, Blake2_128Concat, u64, votingengine::VoteCountU32, ValueQuery>;

    /// 内部投票阈值快照:提案创建时锁定阈值,投票期间不受账户状态变化影响。
    #[pallet::storage]
    pub type InternalThresholdSnapshot<T: Config> =
        StorageMap<_, Blake2_128Concat, u64, u32, OptionQuery>;

    /// 注册个人多签待激活动态阈值:proposal_id -> threshold。
    ///
    /// 注册提案发起时写入，提案执行成功后移动到 `ActivePersonalThresholds`。
    #[pallet::storage]
    pub type PendingPersonalThresholds<T: Config> =
        StorageMap<_, Blake2_128Concat, u64, u32, OptionQuery>;

    /// 个人多签已激活动态阈值：个人多签账户 -> threshold。
    #[pallet::storage]
    pub type ActivePersonalThresholds<T: Config> =
        StorageMap<_, Blake2_128Concat, T::AccountId, u32, OptionQuery>;

    /// 管理员变更提案待应用的新动态阈值。
    #[pallet::storage]
    pub type PendingPersonalAdminChangeThresholds<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        u64,
        PendingPersonalAdminChangeThreshold<T::AccountId>,
        OptionQuery,
    >;

    /// 内部提案语义分类。用于终态副作用，不交给业务模块判断。
    #[pallet::storage]
    pub type InternalProposalRoles<T: Config> =
        StorageMap<_, Blake2_128Concat, u64, InternalProposalRole, OptionQuery>;

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        /// 内部投票已投出一票。
        InternalVoteCast {
            proposal_id: u64,
            who: T::AccountId,
            /// 个人多签为 `None`；机构投票记录实际使用的岗位码。
            voter_role_code: Option<RoleCode>,
            approve: bool,
        },
    }

    #[pallet::error]
    pub enum Error<T> {
        /// 内部投票的机构类型不合法。
        InvalidInternalCode,
        /// 内部投票阈值快照缺失。
        MissingThresholdSnapshot,
        /// 个人多签全员阈值与管理员快照人数不匹配。
        InvalidThresholdSnapshot,
        /// 注册多签动态阈值不满足严格过半规则。
        InvalidDynamicThreshold,
        /// 动态阈值配置缺失。
        MissingDynamicThreshold,
    }

    use crate::weights::WeightInfo;

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        /// 内部投票。个人多签使用管理员账户；机构使用建案时冻结的岗位任职快照。
        #[pallet::call_index(0)]
        #[pallet::weight(<T as Config>::WeightInfo::cast())]
        pub fn cast(
            origin: OriginFor<T>,
            proposal_id: u64,
            ticket_claim: InternalVoteTicketClaim,
            approve: bool,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Self::do_internal_vote(who, proposal_id, ticket_claim, approve)
        }
    }
}
// 内部判定 helper
fn is_valid_institution_context(institution_code: InstitutionCode, cid_number: &[u8]) -> bool {
    !is_personal_code(&institution_code)
        && is_valid_governance_code(&institution_code)
        && institution_code_from_cid_number(core::str::from_utf8(cid_number).unwrap_or_default())
            == Some(institution_code)
}

fn is_personal_admin<T: Config>(personal_account_id: T::AccountId, who: &T::AccountId) -> bool {
    <T as votingengine::Config>::InternalAdminProvider::is_personal_admin(personal_account_id, who)
}

fn active_institution_threshold<T: Config>(
    institution_code: InstitutionCode,
    cid_number: &CidNumber,
) -> Option<u32> {
    <T as votingengine::Config>::InternalAdminProvider::institution_threshold(
        institution_code,
        cid_number.as_slice(),
    )
}
// 业务方法
// trait 实现
impl<T: Config> votingengine::InternalVoteEngine<T::AccountId> for Pallet<T> {
    fn create_institution_proposal_with_data(
        who: T::AccountId,
        institution_code: InstitutionCode,
        actor_cid_number: sp_std::vec::Vec<u8>,
        execution_account_id: Option<T::AccountId>,
        subject_cid_numbers: sp_std::vec::Vec<sp_std::vec::Vec<u8>>,
        vote_plan: votingengine::types::VotePlanOf<T::AccountId>,
        data: sp_std::vec::Vec<u8>,
    ) -> Result<u64, DispatchError> {
        with_transaction(|| {
            if sp_io::hashing::blake2_256(&data) != vote_plan.business_object_hash {
                return TransactionOutcome::Rollback(Err(
                    votingengine::Error::<T>::InvalidVotePlan.into(),
                ));
            }
            let proposal_id = match Self::do_create_institution_proposal(
                who.clone(),
                institution_code,
                actor_cid_number,
                execution_account_id,
                subject_cid_numbers,
                &vote_plan,
            ) {
                Ok(id) => id,
                Err(err) => return TransactionOutcome::Rollback(Err(err)),
            };
            if let Err(err) =
                votingengine::Pallet::<T>::bind_vote_plan(proposal_id, vote_plan.clone())
            {
                return TransactionOutcome::Rollback(Err(err));
            }
            match Self::register_data_and_auto_approve(
                who,
                proposal_id,
                vote_plan.proposal_owner.as_slice(),
                data,
            ) {
                Ok(id) => TransactionOutcome::Commit(Ok(id)),
                Err(err) => TransactionOutcome::Rollback(Err(err)),
            }
        })
    }

    fn create_personal_proposal_with_data(
        who: T::AccountId,
        personal_account_id: T::AccountId,
        module_tag: &[u8],
        data: sp_std::vec::Vec<u8>,
    ) -> Result<u64, DispatchError> {
        with_transaction(|| {
            let proposal_id =
                match Self::do_create_personal_proposal(who.clone(), personal_account_id) {
                    Ok(id) => id,
                    Err(err) => return TransactionOutcome::Rollback(Err(err)),
                };
            match Self::register_data_and_auto_approve(who, proposal_id, module_tag, data) {
                Ok(id) => TransactionOutcome::Commit(Ok(id)),
                Err(err) => TransactionOutcome::Rollback(Err(err)),
            }
        })
    }

    fn create_institution_admin_change_proposal_with_data(
        who: T::AccountId,
        institution_code: InstitutionCode,
        actor_cid_number: sp_std::vec::Vec<u8>,
        vote_plan: votingengine::types::VotePlanOf<T::AccountId>,
        data: sp_std::vec::Vec<u8>,
    ) -> Result<u64, DispatchError> {
        with_transaction(|| {
            if sp_io::hashing::blake2_256(&data) != vote_plan.business_object_hash {
                return TransactionOutcome::Rollback(Err(
                    votingengine::Error::<T>::InvalidVotePlan.into(),
                ));
            }
            let proposal_id = match Self::do_create_institution_proposal_with_mutex(
                who.clone(),
                institution_code,
                actor_cid_number,
                None,
                sp_std::vec::Vec::new(),
                InternalProposalMutexKind::AdminSetMutationExclusive,
                InternalProposalRole::InstitutionAdminChange,
                &vote_plan,
            ) {
                Ok(id) => id,
                Err(err) => return TransactionOutcome::Rollback(Err(err)),
            };
            if let Err(err) =
                votingengine::Pallet::<T>::bind_vote_plan(proposal_id, vote_plan.clone())
            {
                return TransactionOutcome::Rollback(Err(err));
            }
            match Self::register_data_and_auto_approve(
                who,
                proposal_id,
                vote_plan.proposal_owner.as_slice(),
                data,
            ) {
                Ok(id) => TransactionOutcome::Commit(Ok(id)),
                Err(err) => TransactionOutcome::Rollback(Err(err)),
            }
        })
    }

    fn create_personal_lifecycle_proposal_with_data(
        who: T::AccountId,
        personal_account_id: T::AccountId,
        module_tag: &[u8],
        data: sp_std::vec::Vec<u8>,
    ) -> Result<u64, DispatchError> {
        with_transaction(|| {
            let proposal_id =
                match Self::do_create_personal_lifecycle_proposal(who.clone(), personal_account_id)
                {
                    Ok(id) => id,
                    Err(err) => return TransactionOutcome::Rollback(Err(err)),
                };
            match Self::register_data_and_auto_approve(who, proposal_id, module_tag, data) {
                Ok(id) => TransactionOutcome::Commit(Ok(id)),
                Err(err) => TransactionOutcome::Rollback(Err(err)),
            }
        })
    }

    fn create_personal_account_create_proposal_with_data(
        who: T::AccountId,
        personal_account_id: T::AccountId,
        admins: sp_std::vec::Vec<T::AccountId>,
        dynamic_threshold: u32,
        module_tag: &[u8],
        data: sp_std::vec::Vec<u8>,
    ) -> Result<u64, DispatchError> {
        with_transaction(|| {
            let proposal_id = match Self::do_create_personal_account_create_proposal(
                who.clone(),
                personal_account_id,
                admins,
                dynamic_threshold,
            ) {
                Ok(id) => id,
                Err(err) => return TransactionOutcome::Rollback(Err(err)),
            };
            match Self::register_data_and_auto_approve(who, proposal_id, module_tag, data) {
                Ok(id) => TransactionOutcome::Commit(Ok(id)),
                Err(err) => TransactionOutcome::Rollback(Err(err)),
            }
        })
    }

    fn create_personal_admin_change_proposal_with_data(
        who: T::AccountId,
        personal_account_id: T::AccountId,
        new_admins_len: u32,
        new_threshold: u32,
        module_tag: &[u8],
        data: sp_std::vec::Vec<u8>,
    ) -> Result<u64, DispatchError> {
        with_transaction(|| {
            let proposal_id = match Self::do_create_personal_admin_change_proposal(
                who.clone(),
                personal_account_id,
                new_admins_len,
                new_threshold,
            ) {
                Ok(id) => id,
                Err(err) => return TransactionOutcome::Rollback(Err(err)),
            };
            match Self::register_data_and_auto_approve(who, proposal_id, module_tag, data) {
                Ok(id) => TransactionOutcome::Commit(Ok(id)),
                Err(err) => TransactionOutcome::Rollback(Err(err)),
            }
        })
    }

    fn active_institution_threshold(
        institution_code: InstitutionCode,
        cid_number: &[u8],
    ) -> Option<u32> {
        let cid_number = CidNumber::try_from(cid_number.to_vec()).ok()?;
        active_institution_threshold::<T>(institution_code, &cid_number)
    }

    fn active_personal_threshold(personal_account_id: T::AccountId) -> Option<u32> {
        ActivePersonalThresholds::<T>::get(personal_account_id)
    }

    fn configured_institution_threshold(
        _proposal_id: u64,
        institution_code: InstitutionCode,
        cid_number: &[u8],
    ) -> Option<u32> {
        Self::active_institution_threshold(institution_code, cid_number)
    }

    fn configured_personal_threshold(
        proposal_id: u64,
        personal_account_id: T::AccountId,
    ) -> Option<u32> {
        PendingPersonalThresholds::<T>::get(proposal_id)
            .or_else(|| ActivePersonalThresholds::<T>::get(personal_account_id))
    }
}

impl<T: Config>
    votingengine::traits::InternalProposalFinalizer<
        frame_system::pallet_prelude::BlockNumberFor<T>,
        T::AccountId,
    > for Pallet<T>
{
    fn finalize_internal_timeout(
        proposal: &Proposal<frame_system::pallet_prelude::BlockNumberFor<T>, T::AccountId>,
        proposal_id: u64,
    ) -> DispatchResult {
        Self::do_finalize_internal_timeout(proposal, proposal_id)
    }
}
