//! 按机构岗位票据或个人管理员快照执行内部投票程序。

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

use engingcore::{
    pallet::{AdminSnapshot, Proposals},
    types::{
        institution_code_from_cid_number, is_personal_code, is_valid_governance_code,
        AuthorizationSubject, CidNumber, InstitutionCode, InstitutionVoteTicket, ProposalSubject,
        ProposalSubjectCidNumbers, RoleCode, VotePlanOf, VotingEngineKind,
    },
    InternalProposalMutexKind, InternalVoteProvider, Proposal, PROPOSAL_KIND_INTERNAL,
    STAGE_INTERNAL, STATUS_EXECUTED, STATUS_EXECUTION_FAILED, STATUS_PASSED, STATUS_REJECTED,
};

pub mod weights;

mod cleanup;
mod personal;
mod proposal;
mod threshold;
mod vote;

#[cfg(feature = "runtime-benchmarks")]
#[path = "../tests/benchmarks.rs"]
mod benchmarks;

pub use pallet::*;

#[cfg(test)]
#[path = "../tests/mod.rs"]
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
    /// 保留旧机构提案的编码值，仅用于识别历史记录。
    RetiredInstitutionChange,
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
    #[codec(index = 2)]
    Institution(InstitutionVoteTicket),
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

    /// 提案编号键控存储版本，在线迁移由 Runtime 统一装配。
    pub const STORAGE_VERSION: StorageVersion = StorageVersion::new(0);

    #[pallet::config]
    pub trait Config: frame_system::Config + engingcore::Config {
        #[allow(deprecated)]
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
        /// 只读取岗位有效任职；业务权限已由业务 pallet 前置校验。
        type InstitutionRoleProvider: engingcore::InstitutionRoleProvider<Self::AccountId>;
        type WeightInfo: crate::weights::WeightInfo;
    }

    #[pallet::pallet]
    #[pallet::storage_version(STORAGE_VERSION)]
    pub struct Pallet<T>(_);

    /// 内部投票记录：个人多签按账户去重，机构按机构 CID、岗位码和人员 CID 去重。
    #[pallet::storage]
    pub type InternalVotesByTicket<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        u128,
        Blake2_128Concat,
        InternalVoteTicket<T::AccountId>,
        bool,
        OptionQuery,
    >;

    #[pallet::storage]
    #[pallet::getter(fn internal_tally)]
    pub type InternalTallies<T: Config> =
        StorageMap<_, Blake2_128Concat, u128, engingcore::VoteCountU32, ValueQuery>;

    /// 内部投票阈值快照:提案创建时锁定阈值,投票期间不受账户状态变化影响。
    #[pallet::storage]
    pub type InternalThresholdSnapshot<T: Config> =
        StorageMap<_, Blake2_128Concat, u128, u32, OptionQuery>;

    /// 注册个人多签待激活动态阈值:proposal_id -> threshold。
    ///
    /// 注册提案发起时写入，提案执行成功后移动到 `ActivePersonalThresholds`。
    #[pallet::storage]
    pub type PendingPersonalThresholds<T: Config> =
        StorageMap<_, Blake2_128Concat, u128, u32, OptionQuery>;

    /// 个人多签已激活动态阈值：个人多签账户 -> threshold。
    #[pallet::storage]
    pub type ActivePersonalThresholds<T: Config> =
        StorageMap<_, Blake2_128Concat, T::AccountId, u32, OptionQuery>;

    /// 管理员变更提案待应用的新动态阈值。
    #[pallet::storage]
    pub type PendingPersonalAdminChangeThresholds<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        u128,
        PendingPersonalAdminChangeThreshold<T::AccountId>,
        OptionQuery,
    >;

    /// 内部提案语义分类。用于终态副作用，不交给业务模块判断。
    #[pallet::storage]
    pub type InternalProposalRoles<T: Config> =
        StorageMap<_, Blake2_128Concat, u128, InternalProposalRole, OptionQuery>;

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        /// 内部投票已投出一票。
        InternalVoteCast {
            proposal_id: u128,
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
        /// 动态阈值不满足所属主体的范围规则。
        InvalidDynamicThreshold,
        /// 动态阈值配置缺失。
        MissingDynamicThreshold,
    }

    use crate::weights::WeightInfo;

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        /// 内部投票。个人多签使用管理员账户；机构使用建案时冻结的岗位任职快照。
        #[pallet::call_index(0)]
        #[pallet::weight(<T as Config>::WeightInfo::cast().saturating_add(if matches!(ticket_claim,InternalVoteTicketClaim::InstitutionRole(_)){engingcore::Pallet::<T>::frozen_vote_weight()}else{Weight::zero()}))]
        pub fn cast(
            origin: OriginFor<T>,
            proposal_id: u128,
            ticket_claim: InternalVoteTicketClaim,
            approve: bool,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Self::do_internal_vote(who, proposal_id, ticket_claim, approve)
        }
    }
}
// 机构判定 helper

// 业务方法
// trait 实现
impl<T: Config> engingcore::InternalVoteEngine<T::AccountId> for Pallet<T> {
    fn validate_personal_threshold(admins_len: u32, threshold: u32) -> DispatchResult {
        Self::ensure_personal_threshold(admins_len, threshold)
    }

    fn create_institution_proposal_with_data(
        who: T::AccountId,
        institution_code: InstitutionCode,
        actor_cid_number: sp_std::vec::Vec<u8>,
        execution_account_id: Option<T::AccountId>,
        subject_cid_numbers: sp_std::vec::Vec<sp_std::vec::Vec<u8>>,
        vote_plan: engingcore::types::VotePlanOf<T::AccountId>,
        data: sp_std::vec::Vec<u8>,
    ) -> Result<u128, DispatchError> {
        Self::create_institution(
            who,
            institution_code,
            actor_cid_number,
            execution_account_id,
            subject_cid_numbers,
            vote_plan,
            data,
        )
    }

    fn create_personal_proposal_with_data(
        who: T::AccountId,
        personal_account_id: T::AccountId,
        module_tag: &[u8],
        data: sp_std::vec::Vec<u8>,
    ) -> Result<u128, DispatchError> {
        Self::create_personal(who, personal_account_id, module_tag, data)
    }

    fn create_personal_lifecycle_proposal_with_data(
        who: T::AccountId,
        personal_account_id: T::AccountId,
        module_tag: &[u8],
        data: sp_std::vec::Vec<u8>,
    ) -> Result<u128, DispatchError> {
        Self::create_personal_close(who, personal_account_id, module_tag, data)
    }

    fn create_personal_account_create_proposal_with_data(
        who: T::AccountId,
        personal_account_id: T::AccountId,
        admins: sp_std::vec::Vec<T::AccountId>,
        dynamic_threshold: u32,
        module_tag: &[u8],
        data: sp_std::vec::Vec<u8>,
    ) -> Result<u128, DispatchError> {
        Self::create_personal_account(
            who,
            personal_account_id,
            admins,
            dynamic_threshold,
            module_tag,
            data,
        )
    }

    fn create_personal_admin_change_proposal_with_data(
        who: T::AccountId,
        personal_account_id: T::AccountId,
        new_admins_len: u32,
        new_threshold: u32,
        module_tag: &[u8],
        data: sp_std::vec::Vec<u8>,
    ) -> Result<u128, DispatchError> {
        Self::change_personal_admins(
            who,
            personal_account_id,
            new_admins_len,
            new_threshold,
            module_tag,
            data,
        )
    }

    fn active_institution_threshold(
        institution_code: InstitutionCode,
        cid_number: &[u8],
    ) -> Option<u32> {
        Self::institution_threshold(institution_code, cid_number)
    }

    fn active_personal_threshold(personal_account_id: T::AccountId) -> Option<u32> {
        Self::personal_threshold(personal_account_id)
    }

    fn configured_personal_threshold(
        proposal_id: u128,
        personal_account_id: T::AccountId,
    ) -> Option<u32> {
        Self::configured_threshold(proposal_id, personal_account_id)
    }
}

impl<T: Config>
    engingcore::traits::InternalProposalFinalizer<
        frame_system::pallet_prelude::BlockNumberFor<T>,
        T::AccountId,
    > for Pallet<T>
{
    fn finalize_internal_timeout(
        proposal: &Proposal<frame_system::pallet_prelude::BlockNumberFor<T>, T::AccountId>,
        proposal_id: u128,
    ) -> DispatchResult {
        Self::do_finalize_internal_timeout(proposal, proposal_id)
    }
}

impl<T: Config> Pallet<T> {
    /// 统一20.0入口仅分流；目标实现必须再次核对提案真实主体。
    pub fn do_internal_vote(
        who: T::AccountId,
        proposal_id: u128,
        claim: InternalVoteTicketClaim,
        approve: bool,
    ) -> DispatchResult {
        match claim {
            InternalVoteTicketClaim::Personal => Self::vote_personal(who, proposal_id, approve),
            InternalVoteTicketClaim::InstitutionRole(role) => {
                Self::vote_institution(who, proposal_id, role, approve)
            }
        }
    }

    fn record_internal_vote(
        who: T::AccountId,
        proposal_id: u128,
        ticket: InternalVoteTicket<T::AccountId>,
        voter_role_code: Option<RoleCode>,
        approve: bool,
        eligible_total: u32,
    ) -> DispatchResult {
        ensure!(
            !InternalVotesByTicket::<T>::contains_key(proposal_id, &ticket),
            engingcore::Error::<T>::AlreadyVoted
        );

        InternalVotesByTicket::<T>::insert(proposal_id, ticket, approve);
        let tally = InternalTallies::<T>::mutate(proposal_id, |tally| {
            if approve {
                tally.yes = tally.yes.saturating_add(1);
            } else {
                tally.no = tally.no.saturating_add(1);
            }
            *tally
        });

        Self::deposit_event(Event::<T>::InternalVoteCast {
            proposal_id,
            who,
            voter_role_code,
            approve,
        });

        let threshold = InternalThresholdSnapshot::<T>::get(proposal_id)
            .ok_or(Error::<T>::MissingThresholdSnapshot)?;
        if tally.yes >= threshold {
            <engingcore::Pallet<T>>::set_status_and_emit(proposal_id, STATUS_PASSED)?;
        } else {
            let casted = tally.yes.saturating_add(tally.no);
            let remaining = eligible_total.saturating_sub(casted);
            if tally.yes.saturating_add(remaining) < threshold {
                <engingcore::Pallet<T>>::set_status_and_emit(proposal_id, STATUS_REJECTED)?;
            }
        }

        Ok(())
    }
    pub fn do_finalize_internal_timeout(
        proposal: &Proposal<frame_system::pallet_prelude::BlockNumberFor<T>, T::AccountId>,
        proposal_id: u128,
    ) -> DispatchResult {
        ensure!(
            proposal.stage == STAGE_INTERNAL,
            engingcore::Error::<T>::InvalidProposalStage
        );
        ensure!(
            proposal.status == engingcore::STATUS_VOTING,
            engingcore::Error::<T>::ProposalAlreadyFinalized
        );
        ensure!(
            <frame_system::Pallet<T>>::block_number() > proposal.end,
            engingcore::Error::<T>::VoteNotExpired
        );
        <engingcore::Pallet<T>>::set_status_and_emit(proposal_id, engingcore::STATUS_REJECTED)
    }
    pub(crate) fn internal_stage_duration() -> frame_system::pallet_prelude::BlockNumberFor<T> {
        (VOTING_DURATION_BLOCKS as u64).saturated_into()
    }
    fn allocate_and_lock(
        proposal: &Proposal<frame_system::pallet_prelude::BlockNumberFor<T>, T::AccountId>,
        mutex_kind: InternalProposalMutexKind,
    ) -> Result<u128, DispatchError> {
        let id = <engingcore::Pallet<T>>::allocate_proposal_id()?;
        engingcore::limit::try_add_active_proposals::<T>(proposal.subject_keys(), id)?;
        for subject in proposal.subject_keys() {
            <engingcore::Pallet<T>>::acquire_internal_proposal_mutex(id, subject, mutex_kind)?;
        }
        Ok(id)
    }
    fn finish_proposal_create(
        id: u128,
        proposal: Proposal<frame_system::pallet_prelude::BlockNumberFor<T>, T::AccountId>,
        end: frame_system::pallet_prelude::BlockNumberFor<T>,
        threshold: u32,
        role: InternalProposalRole,
    ) -> TransactionOutcome<Result<u128, DispatchError>> {
        InternalThresholdSnapshot::<T>::insert(id, threshold);
        InternalProposalRoles::<T>::insert(id, role);
        Proposals::<T>::insert(id, proposal);
        if let Err(err) = <engingcore::Pallet<T>>::schedule_proposal_expiry(id, end) {
            return TransactionOutcome::Rollback(Err(err));
        }
        <engingcore::Pallet<T>>::emit_proposal_created(
            id,
            PROPOSAL_KIND_INTERNAL,
            STAGE_INTERNAL,
            end,
        );
        TransactionOutcome::Commit(Ok(id))
    }
}
