//! # 立法投票 pallet (legislation-vote)
//!
//! 立法机构专属投票模式(ADR-027,公民宪法第45/46条)。投票引擎「头等模式」:
//! `PROPOSAL_KIND_LEGISLATION`,共享核心 `votingengine`(Proposals/VotePlan/VoterSnapshot/
//! 状态机/公投快照验签/清理/反向索引),只本地保管计票账本。内部/联合/选举投票
//! sub-pallet 逻辑零改动。
//!
//! 阶段(ADR-027,当前五类提案 + 特别案公投 + 行政签署/三人会签/护宪终审):
//! - `STAGE_LEG_REPRESENTATIVE` 代表表决：单机构一段，多机构按声明顺序逐段推进。
//! - `STAGE_LEG_REFERENDUM` 强制公投:仅特别案(含核心修宪),内部全过后强制进入,公投通过即生效不签署。
//! - `STAGE_LEG_SIGN` 行政签署:非特别案内部全过后,行政机构法定代表人(市长/省长/总统)签署。
//!   市行政区无救济(否决=否决/30天超时=通过);省行政区/国家否决或超时 → 会签。
//! - `STAGE_LEG_OVERRIDE` 三人会签(省行政区/国家):立法院院长 + 参议长 + 众议长,全签=生效/任一否决或超时=否决。
//!
//! 计票口径:按建案时冻结的议员/委员岗位席位票据总数计算参与率和赞成率
//! (`votingengine::types` 的立法阈值纯函数)；投票期满 finalize 统一判定，
//! 结果已确定时可提前决。

#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(feature = "runtime-benchmarks")]
mod benchmarks;

pub mod cleanup;
pub mod legislation;
pub mod representative;
pub mod result;
pub mod rules;
pub mod types;
pub mod weights;

#[cfg(test)]
mod tests;

pub use pallet::*;
pub use types::{
    LegislationProcedureConfig, LegislationVoteEngine, RepresentativeBodies, RepresentativeRoute,
    RepresentativeVoteRule, VoteProcedure, MAX_REPRESENTATIVE_BODIES,
};

use frame_support::{
    ensure,
    pallet_prelude::DispatchResult,
    storage::{with_transaction, TransactionOutcome},
};
use sp_runtime::traits::{One, Saturating};
use sp_runtime::DispatchError;

use primitives::count_const::VOTING_DURATION_BLOCKS;

use votingengine::{
    pallet::{Proposals, ProposalsByExpiry},
    types::{InstitutionCode, ProposalSubjectCidNumbers, VotePlanOf},
    AuthorizationSubject, InternalProposalMutexKind, PopulationScope, Proposal,
    PROPOSAL_KIND_LEGISLATION, STAGE_LEG_CONSTITUTION_GUARD, STAGE_LEG_OVERRIDE,
    STAGE_LEG_REFERENDUM, STAGE_LEG_REPRESENTATIVE, STAGE_LEG_SIGN, STATUS_PASSED, STATUS_REJECTED,
    STATUS_VOTING,
};

use crate::rules::{representative_decided, representative_final_passed};

/// 法律全文大对象类型标记(写入 votingengine `ProposalObject`),与 legislation-yuan 对齐。
pub const PROPOSAL_OBJECT_KIND_LAW_TEXT: u8 = 2;

/// 护宪大法官法定人数(宪法第20条):7 人。
pub const CONSTITUTION_GUARD_MEMBERS: u32 = 7;

/// 修宪终审通过阈值(宪法第21条):7 人多数通过,即 4 名及以上护宪大法官赞成。
/// 口径单源在 `primitives::constitution::CONSTITUTION_GUARD_APPROVAL_THRESHOLD`(与节点守卫共用)。
pub const CONSTITUTION_GUARD_APPROVAL_THRESHOLD: usize =
    primitives::constitution::CONSTITUTION_GUARD_APPROVAL_THRESHOLD as usize;

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use crate::weights::WeightInfo;
    use frame_support::pallet_prelude::*;
    use frame_system::pallet_prelude::*;

    /// 重新创世直接使用代表表决与法律专属元数据分离的最终布局。
    pub const STORAGE_VERSION: StorageVersion = StorageVersion::new(0);

    /// 代表机构表决元数据。所有法律、任免、预算等立法机关表决共用这一份状态。
    #[derive(
        Encode, Decode, DecodeWithMemTracking, Clone, PartialEq, Eq, Debug, TypeInfo, MaxEncodedLen,
    )]
    pub struct RepresentativeMeta {
        /// 单机构或多机构顺序表决路线。
        pub route: RepresentativeRoute,
        /// 当前正在表决的机构索引。
        pub current_body: u32,
        /// 常规、重要或特别三种数学门槛。
        pub rule: RepresentativeVoteRule,
        /// 代表表决完成后直接终局，或继续法律专属程序。
        pub procedure: VoteProcedure,
    }

    /// 法律专属元数据。任免和预算提案不得创建本记录。
    #[derive(
        Encode, Decode, DecodeWithMemTracking, Clone, PartialEq, Eq, Debug, TypeInfo, MaxEncodedLen,
    )]
    pub struct LegislationMeta {
        /// 行政签署机构的 LR 岗位主体；任职在建案时冻结。
        pub executive: Option<crate::types::RepresentativeBody>,
        /// 省级、国家级三人会签的三个 LR 岗位主体；市级和特别案为空。
        pub override_signers: BoundedVec<crate::types::RepresentativeBody, ConstU32<3>>,
        /// 是否修宪(tier=宪法):为真时,现有流程通过后最后进护宪大法官终审(宪法第21条)。
        pub needs_guard: bool,
        /// 修宪终审的护宪大法官岗位主体；非修宪为空。
        pub guard: Option<crate::types::RepresentativeBody>,
    }

    #[pallet::config]
    pub trait Config: frame_system::Config + votingengine::Config {
        #[allow(deprecated)]
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
        /// 只提供岗位任职事实；业务权限由调用本引擎的业务模块校验。
        type InstitutionRoleProvider: votingengine::InstitutionRoleProvider<Self::AccountId>;
        type WeightInfo: crate::weights::WeightInfo;
    }

    #[pallet::pallet]
    #[pallet::storage_version(STORAGE_VERSION)]
    pub struct Pallet<T>(_);

    /// 代表机构表决元数据：proposal_id → RepresentativeMeta。
    #[pallet::storage]
    pub type RepresentativeMetas<T: Config> =
        StorageMap<_, Blake2_128Concat, u64, RepresentativeMeta, OptionQuery>;

    /// 法律专属元数据：只有 `VoteProcedure::Legislation` 提案存在。
    #[pallet::storage]
    pub type LegislationMetas<T: Config> =
        StorageMap<_, Blake2_128Concat, u64, LegislationMeta, OptionQuery>;

    /// 每个代表机构阶段独立计票：(proposal_id, body_index) → yes/no。
    #[pallet::storage]
    pub type RepresentativeTallies<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        u64,
        Blake2_128Concat,
        u32,
        votingengine::VoteCountU32,
        ValueQuery,
    >;

    /// 代表表决去重：(proposal_id, (body_index, 岗位票据)) → 赞成/反对。
    /// `body_index` 隔离顺序表决阶段，岗位票据保留 CID + 岗位码 + 账户三元权限。
    #[pallet::storage]
    pub type RepresentativeVotesByTicket<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        u64,
        Blake2_128Concat,
        (
            u32,
            votingengine::types::InstitutionVoteTicket<T::AccountId>,
        ),
        bool,
        OptionQuery,
    >;

    /// 公投计票(特别案)。
    #[pallet::storage]
    pub type LegReferendumTally<T: Config> =
        StorageMap<_, Blake2_128Concat, u64, votingengine::VoteCountU64, ValueQuery>;

    /// 立法公投票据：`(proposal_id, 永久公民 CID) → 完整公民主体 + 票值`。
    ///
    /// 按 CID 而不是账户去重，保证公民更换绑定账户后仍只能投一票。
    #[pallet::storage]
    pub type LegReferendumVotesByCid<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        u64,
        Blake2_128Concat,
        votingengine::types::CidNumber,
        votingengine::CitizenReferendumTicket<T::AccountId>,
        OptionQuery,
    >;

    /// 三人会签记录(省行政区/国家 STAGE_LEG_OVERRIDE):proposal_id → [(签署人, 是否赞成)],
    /// 去重 + 集齐 3 个不同身份赞成判通过。签署人 ∈ {院长, 参议长, 众议长} 法定代表人。
    #[pallet::storage]
    pub type LegOverrideSigns<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        u64,
        BoundedVec<(T::AccountId, bool), ConstU32<3>>,
        ValueQuery,
    >;

    /// 护宪大法官终审记录(仅修宪 STAGE_LEG_CONSTITUTION_GUARD):proposal_id → [(护宪大法官, 是否赞成)]。
    /// 去重 + 4 名及以上赞成判通过。成员集来自 `InternalAdminProvider::constitution_guard_members`。
    #[pallet::storage]
    pub type LegGuardSigns<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        u64,
        BoundedVec<(T::AccountId, bool), ConstU32<CONSTITUTION_GUARD_MEMBERS>>,
        ValueQuery,
    >;

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        /// 立法机关代表表决提案已创建。
        RepresentativeProposalCreated {
            proposal_id: u64,
            rule: RepresentativeVoteRule,
            bodies: u32,
            procedure: VoteProcedure,
        },
        /// 某议员/委员投出一票。
        RepresentativeVoteCast {
            proposal_id: u64,
            body_index: u32,
            who: T::AccountId,
            voter_role_code: votingengine::types::RoleCode,
            approve: bool,
        },
        /// 当前代表机构通过，推进至下一代表机构。
        RepresentativeBodyAdvanced { proposal_id: u64, next_body: u32 },
        /// 内部全过,推进至强制公投阶段。
        LegislationAdvancedToReferendum {
            proposal_id: u64,
            eligible_total: u64,
        },
        /// 一张公投票已投出。
        LegislationReferendumVoteCast {
            proposal_id: u64,
            voter_subject: votingengine::CitizenSubject<T::AccountId>,
            approve: bool,
        },
        /// 内部全过(非特别案),推进至行政签署阶段。
        LegislationAdvancedToSign { proposal_id: u64 },
        /// 行政首长(市长/省长/总统)已签署或否决。
        LegislationExecutiveSigned {
            proposal_id: u64,
            who: T::AccountId,
            approve: bool,
        },
        /// 退回立法院三人会签阶段(省行政区/国家)。
        LegislationAdvancedToOverride { proposal_id: u64 },
        /// 三人会签其一已签署或否决。
        LegislationOverrideSigned {
            proposal_id: u64,
            who: T::AccountId,
            approve: bool,
        },
        /// 修宪通过现有流程,推进至护宪大法官终审阶段。
        LegislationAdvancedToGuard { proposal_id: u64 },
        /// 护宪大法官其一已表决(修宪终审)。
        LegislationGuardVoted {
            proposal_id: u64,
            who: T::AccountId,
            approve: bool,
        },
    }

    #[pallet::error]
    pub enum Error<T> {
        /// 代表机构路线为空、重复、单机构误用顺序路线或超过上限。
        InvalidRepresentativeRoute,
        /// 特别门槛与直接终局程序组合非法。
        InvalidRepresentativeRule,
        /// 提案元数据缺失
        ProposalMetaMissing,
        /// 公投分母未设置
        CitizenEligibleTotalNotSet,
        /// 公民身份无公投资格
        CitizenNotEligible,
        /// 已投票人数达到创建时人口快照分母，拒绝分子超过 100%。
        ReferendumSnapshotExhausted,
        /// 提案不在该阶段(签署/会签 stage 校验)
        NotInExpectedStage,
        /// 签署人不是该机构法定代表人(行政签署)
        NotLegalRepresentative,
        /// 签署人不在三人会签合法身份集合(院长/参议长/众议长)
        NotOverrideSigner,
        /// 该身份已在本提案会签过
        AlreadySigned,
        /// 签署人不是护宪大法官
        NotConstitutionGuard,
        /// 护宪大法官成员数不是 7 人或成员重复
        InvalidGuardMembersLen,
        /// 机构账户无法解析到唯一 CID。
        InvalidInstitutionCid,
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        // call_index(0) 已永久废弃：特别案人口快照随提案创建在投票引擎事务内生成。

        /// 管理员按当前代表机构席位投票。
        #[pallet::call_index(1)]
        #[pallet::weight(<T as Config>::WeightInfo::cast_representative_vote())]
        pub fn cast_representative_vote(
            origin: OriginFor<T>,
            proposal_id: u64,
            voter_role_code: votingengine::types::RoleCode,
            approve: bool,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Self::do_cast_representative_vote(who, proposal_id, voter_role_code, approve)
        }

        /// 公民对特别案公投投票（校验完整公民主体，并按永久 CID 去重计票）。
        #[pallet::call_index(2)]
        #[pallet::weight(<T as Config>::WeightInfo::cast_referendum_vote())]
        pub fn cast_referendum_vote(
            origin: OriginFor<T>,
            proposal_id: u64,
            approve: bool,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Self::do_cast_referendum_vote(who, proposal_id, approve)
        }

        /// 行政首长(机构法定代表人:市长/省长/总统)对终审通过的非特别案签署或否决。
        /// 批准=生效;否决:市行政区=否决,省行政区/国家=退回三人会签。
        #[pallet::call_index(3)]
        #[pallet::weight(
            <T as Config>::WeightInfo::executive_sign().max(
                Weight::from_parts(38_000_000, 67_187)
                    .saturating_add(T::DbWeight::get().reads_writes(7, 5))
            )
        )]
        pub fn executive_sign(
            origin: OriginFor<T>,
            proposal_id: u64,
            approve: bool,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Self::do_executive_sign(who, proposal_id, approve)
        }

        /// 三人会签(省行政区/国家:立法院院长 + 参议长 + 众议长)签署或否决。
        /// 三人全批准=生效;任一否决=否决。
        #[pallet::call_index(4)]
        #[pallet::weight(
            <T as Config>::WeightInfo::override_sign().max(
                Weight::from_parts(45_000_000, 67_187)
                    .saturating_add(T::DbWeight::get().reads_writes(10, 2))
            )
        )]
        pub fn override_sign(
            origin: OriginFor<T>,
            proposal_id: u64,
            approve: bool,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Self::do_override_sign(who, proposal_id, approve)
        }

        /// 护宪大法官对修宪提案终审表决(宪法第21条):一人一票,4名及以上赞成→生效。
        #[pallet::call_index(5)]
        #[pallet::weight(
            <T as Config>::WeightInfo::guard_vote().max(
                Weight::from_parts(35_000_000, 30_000)
                    .saturating_add(T::DbWeight::get().reads_writes(5, 1))
            )
        )]
        pub fn guard_vote(origin: OriginFor<T>, proposal_id: u64, approve: bool) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Self::do_guard_vote(who, proposal_id, approve)
        }
    }
}
// 业务方法
// trait 实现(供 votingengine 核心 + 业务壳接入)
impl<T: Config> crate::LegislationVoteEngine<T::AccountId> for Pallet<T> {
    fn create_representative_vote(
        who: T::AccountId,
        actor_cid_number: votingengine::types::CidNumber,
        vote_plan: VotePlanOf<T::AccountId>,
        route: RepresentativeRoute,
        rule: RepresentativeVoteRule,
        subject_cid_numbers: ProposalSubjectCidNumbers,
        module_tag: &[u8],
        data: sp_runtime::sp_std::vec::Vec<u8>,
    ) -> Result<u64, DispatchError> {
        let first_body = Self::validate_representative_route(&route)?.1;
        with_transaction(|| {
            let id = match Self::do_create_representative_proposal(
                who.clone(),
                actor_cid_number,
                vote_plan,
                route,
                rule,
                VoteProcedure::RepresentativeOnly,
                subject_cid_numbers,
                None,
            ) {
                Ok(id) => id,
                Err(err) => return TransactionOutcome::Rollback(Err(err)),
            };
            let now = <frame_system::Pallet<T>>::block_number();
            if let Err(err) =
                <votingengine::Pallet<T>>::register_proposal_data(id, module_tag, data, now)
            {
                return TransactionOutcome::Rollback(Err(err));
            }
            if <votingengine::Pallet<T>>::is_subject_voter_in_snapshot(
                id,
                AuthorizationSubject::Institution(first_body.clone()),
                &who,
            ) {
                match Self::do_cast_representative_vote(who, id, first_body.role_code, true) {
                    Ok(()) => TransactionOutcome::Commit(Ok(id)),
                    Err(err) => TransactionOutcome::Rollback(Err(err)),
                }
            } else {
                TransactionOutcome::Commit(Ok(id))
            }
        })
    }

    fn create_legislation_vote(
        who: T::AccountId,
        actor_cid_number: votingengine::types::CidNumber,
        vote_plan: VotePlanOf<T::AccountId>,
        route: RepresentativeRoute,
        rule: RepresentativeVoteRule,
        procedure: LegislationProcedureConfig,
        module_tag: &[u8],
        data: sp_runtime::sp_std::vec::Vec<u8>,
        object_data: sp_runtime::sp_std::vec::Vec<u8>,
    ) -> Result<u64, DispatchError> {
        let first_body = Self::validate_representative_route(&route)?.1;
        with_transaction(|| {
            let id = match Self::do_create_representative_proposal(
                who.clone(),
                actor_cid_number,
                vote_plan,
                route,
                rule,
                VoteProcedure::Legislation,
                ProposalSubjectCidNumbers::new(),
                Some(pallet::LegislationMeta {
                    executive: procedure.executive,
                    override_signers: procedure.override_signers,
                    needs_guard: procedure.needs_guard,
                    guard: procedure.guard,
                }),
            ) {
                Ok(id) => id,
                Err(err) => return TransactionOutcome::Rollback(Err(err)),
            };
            let now = <frame_system::Pallet<T>>::block_number();
            if let Err(err) =
                <votingengine::Pallet<T>>::register_proposal_data(id, module_tag, data, now)
            {
                return TransactionOutcome::Rollback(Err(err));
            }
            if let Err(err) = <votingengine::Pallet<T>>::store_proposal_object(
                id,
                PROPOSAL_OBJECT_KIND_LAW_TEXT,
                object_data,
            ) {
                return TransactionOutcome::Rollback(Err(err));
            }
            // 发起人若属表决院(国家/省两院:发起院=众议会/教委会)则自动赞成一票;
            // 市行政区 市自治会/市教委会 委员提案时发起人不在表决院(市立法会),不自动投票。
            if <votingengine::Pallet<T>>::is_subject_voter_in_snapshot(
                id,
                AuthorizationSubject::Institution(first_body.clone()),
                &who,
            ) {
                match Self::do_cast_representative_vote(who, id, first_body.role_code, true) {
                    Ok(()) => TransactionOutcome::Commit(Ok(id)),
                    Err(err) => TransactionOutcome::Rollback(Err(err)),
                }
            } else {
                TransactionOutcome::Commit(Ok(id))
            }
        })
    }

    /// 读取某立法提案的强制公投结果 `(eligible, yes, no)`。
    /// 没有提案人口快照(即非特别案)或提案不存在 → `None`。
    /// 公投计票 `LegReferendumTally` 在提案 90 天清理前一直保留,故核心修宪写入(护宪终审同块)时可读到。
    fn referendum_result(proposal_id: u64) -> Option<(u64, u64, u64)> {
        let eligible = <votingengine::Pallet<T>>::population_eligible_total_of(proposal_id)?;
        if eligible == 0 {
            return None;
        }
        let tally = pallet::LegReferendumTally::<T>::get(proposal_id);
        Some((eligible, tally.yes, tally.no))
    }

    /// 读取某修宪提案的护宪大法官终审赞成票数;无终审记录(非修宪 / 未进终审)→ `None`。
    /// 记录 `LegGuardSigns` 在提案 90 天清理前保留,故写入版本(终审通过同块)时可读到。
    fn guard_review_result(proposal_id: u64) -> Option<u32> {
        let signs = pallet::LegGuardSigns::<T>::get(proposal_id);
        if signs.is_empty() {
            return None;
        }
        Some(signs.iter().filter(|(_, approve)| *approve).count() as u32)
    }
}

impl<T: Config>
    votingengine::traits::LegislationProposalFinalizer<
        frame_system::pallet_prelude::BlockNumberFor<T>,
        T::AccountId,
    > for Pallet<T>
{
    fn finalize_legislation_representative_timeout(
        proposal: &Proposal<frame_system::pallet_prelude::BlockNumberFor<T>, T::AccountId>,
        proposal_id: u64,
    ) -> DispatchResult {
        Self::do_finalize_representative_timeout(proposal, proposal_id)
    }

    fn finalize_legislation_referendum_timeout(
        proposal: &Proposal<frame_system::pallet_prelude::BlockNumberFor<T>, T::AccountId>,
        proposal_id: u64,
    ) -> DispatchResult {
        Self::do_finalize_referendum_timeout(proposal, proposal_id)
    }

    fn finalize_legislation_sign_timeout(
        proposal: &Proposal<frame_system::pallet_prelude::BlockNumberFor<T>, T::AccountId>,
        proposal_id: u64,
    ) -> DispatchResult {
        Self::do_finalize_sign_timeout(proposal, proposal_id)
    }

    fn finalize_legislation_override_timeout(
        proposal: &Proposal<frame_system::pallet_prelude::BlockNumberFor<T>, T::AccountId>,
        proposal_id: u64,
    ) -> DispatchResult {
        Self::do_finalize_override_timeout(proposal, proposal_id)
    }

    fn finalize_legislation_guard_timeout(
        proposal: &Proposal<frame_system::pallet_prelude::BlockNumberFor<T>, T::AccountId>,
        proposal_id: u64,
    ) -> DispatchResult {
        Self::do_finalize_guard_timeout(proposal, proposal_id)
    }
}
