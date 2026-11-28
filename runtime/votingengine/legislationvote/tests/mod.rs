//! 装配立法投票测试Runtime、身份范围及代表机构夹具。

#![cfg(test)]

//! 立法投票 sub-pallet 单测 mock runtime。
//!
//! System + VotingEngine + InternalVote(供 engingcore 必填 finalizer)+ LegislationVote。
//! engingcore::Config 通过 TrackHandlers 注册 LegislationVote，
//! LegislationVoteResultCallback 装 `()`(本 sub-pallet 单测只验投票机制,不验业务写法律)。
//! TestInternalVoteProvider 定义两院议员名册;公投公民资格从测试用 CitizenIdentityReader 返回。

use codec::{Decode, Encode};
use frame_support::{
    derive_impl,
    traits::{ConstU32, ConstU64, Hooks},
};
use frame_system as system;
use primitives::cid::{
    china::{china_jy::CHINA_JY, china_lf::CHINA_LF, china_sf::CHINA_SF, china_zf::CHINA_ZF},
    code::institution_code_from_cid_number,
};
use sp_runtime::{traits::IdentityLookup, AccountId32, BuildStorage};
use std::cell::RefCell;

type Block = frame_system::mocking::MockBlock<Test>;

#[frame_support::runtime]
mod runtime {
    #[runtime::runtime]
    #[runtime::derive(
        RuntimeCall,
        RuntimeEvent,
        RuntimeError,
        RuntimeOrigin,
        RuntimeFreezeReason,
        RuntimeHoldReason,
        RuntimeSlashReason,
        RuntimeLockId,
        RuntimeTask,
        RuntimeViewFunction
    )]
    pub struct Test;

    #[runtime::pallet_index(0)]
    pub type System = frame_system;

    #[runtime::pallet_index(1)]
    pub type VotingEngine = engingcore;

    #[runtime::pallet_index(99)]
    pub type InternalVote = internalvote;

    #[runtime::pallet_index(2)]
    pub type LegislationVote = super;
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl system::Config for Test {
    type Block = Block;
    type AccountId = AccountId32;
    type Lookup = IdentityLookup<Self::AccountId>;
}

// ───────── 测试机构 CID / 议员名册 ─────────
fn bounded_cid(cid_number: &str) -> engingcore::types::CidNumber {
    cid_number
        .as_bytes()
        .to_vec()
        .try_into()
        .expect("built-in institution CID should fit")
}

/// 业务发起机构与代表表决机构分离，权限统一按 actor CID 下的 admins 校验。
pub fn actor_cid_number() -> engingcore::types::CidNumber {
    bounded_cid(CHINA_LF[2].cid_number)
}

pub fn house1_cid() -> engingcore::types::CidNumber {
    bounded_cid(CHINA_LF[2].cid_number)
}
pub fn house2_cid() -> engingcore::types::CidNumber {
    bounded_cid(CHINA_LF[1].cid_number)
}
pub fn house3_cid() -> engingcore::types::CidNumber {
    bounded_cid(CHINA_JY[0].cid_number)
}
/// house1 议员 = 账户 [1..=10];house2 议员 = 账户 [11..=20]。
pub fn member(idx: u8) -> AccountId32 {
    AccountId32::new([idx; 32])
}

const DEFAULT_GUARD_MEMBER_IDS: [u8; 7] = [101, 102, 103, 104, 105, 106, 107];

thread_local! {
    static GUARD_MEMBER_IDS: RefCell<std::vec::Vec<u8>> =
        RefCell::new(DEFAULT_GUARD_MEMBER_IDS.to_vec());
}

pub fn set_guard_member_ids(ids: &[u8]) {
    GUARD_MEMBER_IDS.with(|members| {
        *members.borrow_mut() = ids.to_vec();
    });
}

// 签署机构(立法与宪法协议 修订):行政机构 + 立法院(两院级,供院长)。
pub fn exec_body_cid() -> engingcore::types::CidNumber {
    bounded_cid(CHINA_ZF[0].cid_number)
}
/// 行政首长(市长/省长/总统)= 行政机构法定代表人。
pub fn exec_rep() -> AccountId32 {
    AccountId32::new([81u8; 32])
}
pub fn leg_body_cid() -> engingcore::types::CidNumber {
    bounded_cid(CHINA_LF[0].cid_number)
}
/// 立法院院长 = 立法院法定代表人。
pub fn leg_rep() -> AccountId32 {
    AccountId32::new([71u8; 32])
}
pub fn guard_body_cid() -> engingcore::types::CidNumber {
    bounded_cid(
        CHINA_SF
            .iter()
            .find(|entry| institution_code_from_cid_number(entry.cid_number) == Some(*b"NJD\0"))
            .expect("NJD exists")
            .cid_number,
    )
}

const REPRESENTATIVE_ROLE: &[u8] = b"REPRESENTATIVE";
const LR_ROLE: &[u8] = primitives::institution_constraints::ROLE_CODE_LEGAL_REPRESENTATIVE;
const GUARD_ROLE: &[u8] = primitives::governance_skeleton::ROLE_CODE_CONSTITUTION_GUARD;

fn role_subject(
    cid_number: engingcore::types::CidNumber,
    role_code: &[u8],
) -> crate::types::RepresentativeBody {
    entitybase::RoleSubject {
        cid_number,
        role_code: role_code.to_vec().try_into().expect("test role fits"),
    }
}

pub fn house1() -> crate::types::RepresentativeBody {
    role_subject(house1_cid(), REPRESENTATIVE_ROLE)
}
pub fn exec_body() -> crate::types::RepresentativeBody {
    role_subject(exec_body_cid(), LR_ROLE)
}
pub fn leg_body() -> crate::types::RepresentativeBody {
    role_subject(leg_body_cid(), LR_ROLE)
}
pub fn guard_body() -> crate::types::RepresentativeBody {
    role_subject(guard_body_cid(), GUARD_ROLE)
}

pub struct TestCitizenIdentityReader;
pub struct TestInternalVoteProvider;
pub struct TestInstitutionRoleProvider;

impl engingcore::CitizenIdentityReader<AccountId32> for TestCitizenIdentityReader {
    fn person_cid(who: &AccountId32) -> Option<engingcore::CidNumber> {
        let cid = match sp_io::storage::get(&(b"fixture-wallet-person", who).encode()) {
            Some(value) => engingcore::CidNumber::decode(&mut &value[..]).ok()?,
            None => who.encode().try_into().ok()?,
        };
        (Self::person_account(&cid).as_ref() == Some(who)).then_some(cid)
    }

    fn person_account(cid: &engingcore::CidNumber) -> Option<AccountId32> {
        if let Some(value) = sp_io::storage::get(&(b"fixture-person-wallet", cid).encode()) {
            return AccountId32::decode(&mut &value[..]).ok();
        }
        let bytes: [u8; 32] = cid.as_slice().try_into().ok()?;
        let wallet = AccountId32::new(bytes);
        sp_io::storage::get(&(b"fixture-wallet-person", &wallet).encode())
            .is_none()
            .then_some(wallet)
    }

    fn voting_subject(
        who: &AccountId32,
        _scope: &engingcore::PopulationScope,
    ) -> Option<engingcore::CitizenSubject<AccountId32>> {
        Some(test_citizen_subject(who))
    }

    fn candidate_subject(
        who: &AccountId32,
        _scope: &engingcore::PopulationScope,
    ) -> Option<engingcore::CitizenSubject<AccountId32>> {
        Some(test_citizen_subject(who))
    }

    fn population_data(scope: &engingcore::PopulationScope) -> Option<engingcore::PopulationData> {
        Some(engingcore::PopulationData {
            scope: scope.clone(),
            eligible_total: 100,
            eligibility_revision: 1,
            eligibility_date: 20_000,
        })
    }

    fn voting_subject_at(
        who: &AccountId32,
        _population_data: &engingcore::PopulationData,
    ) -> Option<engingcore::CitizenSubject<AccountId32>> {
        Some(test_citizen_subject(who))
    }
}

/// 账户 201 模拟账户 100 更换后的新绑定账户，两者共用同一永久 CID。
fn test_citizen_subject(who: &AccountId32) -> engingcore::CitizenSubject<AccountId32> {
    let cid_source = if who == &member(201) {
        member(100)
    } else {
        who.clone()
    };
    engingcore::CitizenSubject {
        cid_number: <AccountId32 as AsRef<[u8]>>::as_ref(&cid_source)
            .to_vec()
            .try_into()
            .expect("account fits CID"),
        account_id: who.clone(),
    }
}

/// 按机构 CID 查询测试法定代表人。
impl engingcore::InternalVoteProvider<AccountId32> for TestInternalVoteProvider {
    /// 法定代表人:众议长=house1[member 1] / 参议长=house2[member 11] / 院长=leg_rep / 行政首长=exec_rep。
    fn legal_representative(cid_number: &[u8]) -> Option<AccountId32> {
        if cid_number == house1_cid().as_slice() {
            Some(member(1))
        } else if cid_number == house2_cid().as_slice() {
            Some(member(11))
        } else if cid_number == house3_cid().as_slice() {
            Some(member(1))
        } else if cid_number == leg_body_cid().as_slice() {
            Some(leg_rep())
        } else if cid_number == exec_body_cid().as_slice() {
            Some(exec_rep())
        } else {
            None
        }
    }
}

impl engingcore::InstitutionRoleProvider<AccountId32> for TestInstitutionRoleProvider {
    fn is_active_assignment(cid_number: &[u8], who: &AccountId32, role_code: &[u8]) -> bool {
        cid_number == actor_cid_number().as_slice()
            && role_code == REPRESENTATIVE_ROLE
            && (1u8..=10).map(member).any(|account| &account == who)
    }

    fn active_persons_for_role(cid: &[u8], role: &[u8]) -> Vec<Vec<u8>> {
        Self::fixture_wallets_for_role(cid, role).iter().map(codec::Encode::encode).collect()
    }

}
impl TestInstitutionRoleProvider {
    fn fixture_wallets_for_role(cid_number: &[u8], role_code: &[u8]) -> Vec<AccountId32> {
        if role_code == REPRESENTATIVE_ROLE {
            if cid_number == house1_cid().as_slice() || cid_number == house3_cid().as_slice() {
                return (1u8..=10).map(member).collect();
            }
            if cid_number == house2_cid().as_slice() {
                return (11u8..=20).map(member).collect();
            }
        }
        if role_code == LR_ROLE {
            if cid_number == house1_cid().as_slice() || cid_number == house3_cid().as_slice() {
                return vec![member(1)];
            }
            if cid_number == house2_cid().as_slice() {
                return vec![member(11)];
            }
            if cid_number == leg_body_cid().as_slice() {
                return vec![leg_rep()];
            }
            if cid_number == exec_body_cid().as_slice() {
                return vec![exec_rep()];
            }
        }
        if role_code == GUARD_ROLE && cid_number == guard_body_cid().as_slice() {
            return GUARD_MEMBER_IDS.with(|ids| ids.borrow().iter().copied().map(member).collect());
        }
        Vec::new()
    }
}


pub struct TestTimeProvider;
impl frame_support::traits::UnixTime for TestTimeProvider {
    fn now() -> core::time::Duration {
        core::time::Duration::from_secs(1_782_864_000)
    }
}

/// 测试业务回调:模拟业务壳认领立法提案并返回 Executed(真实 runtime 接 Legislation)。
pub struct TestLegislationCallback;
impl engingcore::LegislationVoteResultCallback for TestLegislationCallback {
    fn on_legislation_vote_finalized(
        _vote_proposal_id: u128,
        _approved: bool,
    ) -> Result<engingcore::ProposalExecutionOutcome, sp_runtime::DispatchError> {
        Ok(engingcore::ProposalExecutionOutcome::Executed)
    }
}

// 本模块验证业务状态转换；有界预算由engingcore专用测试与Runtime预算测试覆盖。
frame_support::parameter_types! { pub const LogicBudget: frame_support::weights::Weight = frame_support::weights::Weight::MAX; }

impl engingcore::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type MaxVoteNonceLength = ConstU32<64>;
    type MaxVoteSignatureLength = ConstU32<64>;
    type MaxAutoFinalizePerBlock = ConstU32<64>;
    type MaxAutoFinalizeWeightPerBlock = LogicBudget;
    type MaxExecutionWeightPerBlock = LogicBudget;
    type MaxCleanupWeightPerBlock = LogicBudget;
    type MaxProposalsPerExpiry = ConstU32<128>;
    type MaxInternalProposalMutexBindings = ConstU32<256>;
    type MaxActiveProposals = ConstU32<10>;
    type MaxCleanupStepsPerBlock = ConstU32<8>;
    type CleanupKeysPerStep = ConstU32<64>;
    type MaxProposalDataLen = ConstU32<1024>;
    type MaxProposalObjectLen = ConstU32<{ 64 * 1024 }>;
    type MaxModuleTagLen = ConstU32<32>;
    type MaxManualExecutionAttempts = ConstU32<3>;
    type ExecutionRetryGraceBlocks = ConstU64<216>;
    type MaxExecutionRetryDeadlinesPerBlock = ConstU32<128>;
    type MaxCleanupActivationsPerBlock = ConstU32<50>;
    type MaxPendingRetryExpirationsPerBlock = ConstU32<16>;
    type CitizenIdentityReader = TestCitizenIdentityReader;
    type JointVoteResultCallback = ();
    type InternalVoteResultCallback = ();
    type InternalVoteProvider = TestInternalVoteProvider;
    type MaxSnapshotVoters = ConstU32<64>;
    type TimeProvider = TestTimeProvider;
    type WeightInfo = ();
    type TrackHandlers = (InternalVote, (LegislationVote, ()));
    type LegislationVoteResultCallback = (TestLegislationCallback,);
    type ElectionVoteResultCallback = ();
}

impl internalvote::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type InstitutionRoleProvider = TestInstitutionRoleProvider;
    type WeightInfo = ();
}

impl crate::pallet::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type InstitutionRoleProvider = TestInstitutionRoleProvider;
    type WeightInfo = ();
}

pub fn new_test_ext() -> sp_io::TestExternalities {
    set_guard_member_ids(&DEFAULT_GUARD_MEMBER_IDS);
    let storage = frame_system::GenesisConfig::<Test>::default()
        .build_storage()
        .expect("test storage should build");
    let mut ext = sp_io::TestExternalities::new(storage);
    ext.execute_with(|| {
        System::set_block_number(1);
    });
    ext
}

// ───────── 测试 helper ─────────
pub type Lib = crate::pallet::Pallet<Test>;
pub use crate::pallet::RepresentativeMetas;
use crate::{RepresentativeBodies, RepresentativeRoute, RepresentativeVoteRule, VoteProcedure};

/// 创建立法提案并注册 ProposalData(设置 ProposalOwner,终态回调需要),不自动投票。
pub fn create(
    proposer_account_id: AccountId32,
    bodies: sp_runtime::sp_std::vec::Vec<engingcore::types::CidNumber>,
    rule: RepresentativeVoteRule,
) -> u128 {
    create_inner(proposer_account_id, bodies, rule, false)
}

/// 修宪提案(needs_guard=true):现有流程通过后进护宪大法官终审。
pub fn create_guard(
    proposer_account_id: AccountId32,
    bodies: sp_runtime::sp_std::vec::Vec<engingcore::types::CidNumber>,
    rule: RepresentativeVoteRule,
) -> u128 {
    try_create_inner(proposer_account_id, bodies, rule, true).expect("proposal created")
}

fn create_inner(
    proposer_account_id: AccountId32,
    bodies: sp_runtime::sp_std::vec::Vec<engingcore::types::CidNumber>,
    rule: RepresentativeVoteRule,
    needs_guard: bool,
) -> u128 {
    try_create_inner(proposer_account_id, bodies, rule, needs_guard).expect("proposal created")
}

pub fn try_create_guard(
    proposer_account_id: AccountId32,
    bodies: sp_runtime::sp_std::vec::Vec<engingcore::types::CidNumber>,
    rule: RepresentativeVoteRule,
) -> Result<u128, sp_runtime::DispatchError> {
    try_create_inner(proposer_account_id, bodies, rule, true)
}

fn try_create_inner(
    proposer_account_id: AccountId32,
    bodies: sp_runtime::sp_std::vec::Vec<engingcore::types::CidNumber>,
    rule: RepresentativeVoteRule,
    needs_guard: bool,
) -> Result<u128, sp_runtime::DispatchError> {
    let bounded: RepresentativeBodies = bodies
        .into_iter()
        .map(|cid| role_subject(cid, REPRESENTATIVE_ROLE))
        .collect::<Vec<_>>()
        .try_into()
        .expect("representative route bounded");
    let route = if bounded.len() == 1 {
        RepresentativeRoute::Single(bounded.first().cloned().expect("single body"))
    } else {
        RepresentativeRoute::Sequential(bounded)
    };
    let legislation_meta = legislation_meta(&route, rule, needs_guard);
    let vote_plan = test_vote_plan(&route, legislation_meta.as_ref());
    let pid = Lib::do_create_representative_proposal(
        proposer_account_id,
        actor_cid_number(),
        vote_plan,
        route,
        rule,
        VoteProcedure::Legislation,
        engingcore::types::ProposalSubjectCidNumbers::new(),
        legislation_meta,
    )?;
    let now = System::block_number();
    engingcore::Pallet::<Test>::register_proposal_data(
        pid,
        b"leg-yuan",
        sp_runtime::sp_std::vec![1u8],
        now,
    )?;
    Ok(pid)
}

pub fn legislation_meta(
    route: &RepresentativeRoute,
    rule: RepresentativeVoteRule,
    needs_guard: bool,
) -> Option<crate::pallet::LegislationMeta> {
    let executive = (rule != RepresentativeVoteRule::Special).then(exec_body);
    let mut override_signers = Vec::new();
    if rule != RepresentativeVoteRule::Special && route.len() >= 2 {
        override_signers.push(leg_body());
        for body in route.bodies().into_iter().take(2) {
            override_signers.push(role_subject(body.cid_number, LR_ROLE));
        }
    }
    Some(crate::pallet::LegislationMeta {
        executive,
        override_signers: override_signers.try_into().expect("three signers max"),
        needs_guard,
        guard: needs_guard.then(guard_body),
    })
}

pub fn test_vote_plan(
    route: &RepresentativeRoute,
    meta: Option<&crate::pallet::LegislationMeta>,
) -> engingcore::VotePlanOf<AccountId32> {
    test_vote_plan_with_owner(route, meta, b"leg-yuan")
}

pub fn test_vote_plan_with_owner(
    route: &RepresentativeRoute,
    meta: Option<&crate::pallet::LegislationMeta>,
    module_tag: &[u8],
) -> engingcore::VotePlanOf<AccountId32> {
    let owner: frame_support::BoundedVec<
        u8,
        ConstU32<{ entitybase::BUSINESS_MODULE_TAG_MAX_BYTES }>,
    > = module_tag.to_vec().try_into().expect("owner fits");
    let mut voters = route
        .bodies()
        .into_iter()
        .map(engingcore::AuthorizationSubject::Institution)
        .collect::<Vec<_>>();
    if let Some(meta) = meta {
        voters.extend(
            meta.executive
                .iter()
                .chain(meta.override_signers.iter())
                .chain(meta.guard.iter())
                .cloned()
                .map(engingcore::AuthorizationSubject::Institution),
        );
    }
    engingcore::VotePlanOf::try_new(
        entitybase::BusinessActionId {
            module_tag: owner.clone(),
            action_code: 0,
        },
        owner,
        engingcore::AuthorizationSubject::Institution(role_subject(
            actor_cid_number(),
            REPRESENTATIVE_ROLE,
        )),
        voters,
        engingcore::VotingEngineKind::Legislation,
        [3u8; 32],
    )
    .expect("test legislation plan")
}

/// 单院院序列 [house1]。
pub fn single_house() -> sp_runtime::sp_std::vec::Vec<engingcore::types::CidNumber> {
    sp_runtime::sp_std::vec![house1_cid()]
}
/// 两院院序列 [house1, house2]。
pub fn two_houses() -> sp_runtime::sp_std::vec::Vec<engingcore::types::CidNumber> {
    sp_runtime::sp_std::vec![house1_cid(), house2_cid()]
}

/// 两个管理员名册重叠的代表机构，用于验证同一账户按机构席位分别投票。
pub fn overlapping_bodies() -> sp_runtime::sp_std::vec::Vec<engingcore::types::CidNumber> {
    sp_runtime::sp_std::vec![house1_cid(), house3_cid()]
}

/// 当前提案状态(从核心读)。
pub fn status(pid: u128) -> u8 {
    engingcore::pallet::Proposals::<Test>::get(pid)
        .expect("proposal exists")
        .status
}
pub fn stage(pid: u128) -> u8 {
    engingcore::pallet::Proposals::<Test>::get(pid)
        .expect("proposal exists")
        .stage
}

/// 投一票(事务内,因 set_status_and_emit 需在事务中)。
pub fn cast(who: AccountId32, pid: u128, approve: bool) -> sp_runtime::DispatchResult {
    let meta = RepresentativeMetas::<Test>::get(pid).expect("representative meta exists");
    let voter_role_code = meta
        .route
        .body(meta.current_body)
        .expect("current representative body exists")
        .role_code;
    let result = frame_support::storage::with_transaction(
        || -> frame_support::storage::TransactionOutcome<sp_runtime::DispatchResult> {
            match Lib::do_cast_representative_vote(who, pid, voter_role_code, approve) {
                Ok(()) => frame_support::storage::TransactionOutcome::Commit(Ok(())),
                Err(e) => frame_support::storage::TransactionOutcome::Rollback(Err(e)),
            }
        },
    );
    if result.is_ok() {
        process_current_block();
    }
    result
}

pub fn representative_ticket(
    cid_number: engingcore::types::CidNumber,
    who: AccountId32,
) -> engingcore::InstitutionVoteTicket {
    engingcore::InstitutionVoteTicket {
        role_subject: role_subject(cid_number, REPRESENTATIVE_ROLE),
        person_cid: codec::Encode::encode(&who).try_into().unwrap(),
    }
}

/// 行政签署(事务内)。
pub fn exec_sign(who: AccountId32, pid: u128, approve: bool) -> sp_runtime::DispatchResult {
    let result = frame_support::storage::with_transaction(
        || -> frame_support::storage::TransactionOutcome<sp_runtime::DispatchResult> {
            match Lib::do_executive_sign(who, pid, approve) {
                Ok(()) => frame_support::storage::TransactionOutcome::Commit(Ok(())),
                Err(e) => frame_support::storage::TransactionOutcome::Rollback(Err(e)),
            }
        },
    );
    if result.is_ok() {
        process_current_block();
    }
    result
}

/// 三人会签(事务内)。
pub fn override_sign(who: AccountId32, pid: u128, approve: bool) -> sp_runtime::DispatchResult {
    let result = frame_support::storage::with_transaction(
        || -> frame_support::storage::TransactionOutcome<sp_runtime::DispatchResult> {
            match Lib::do_override_sign(who, pid, approve) {
                Ok(()) => frame_support::storage::TransactionOutcome::Commit(Ok(())),
                Err(e) => frame_support::storage::TransactionOutcome::Rollback(Err(e)),
            }
        },
    );
    if result.is_ok() {
        process_current_block();
    }
    result
}

/// 护宪大法官终审表决(事务内)。
pub fn guard_vote(who: AccountId32, pid: u128, approve: bool) -> sp_runtime::DispatchResult {
    let result = frame_support::storage::with_transaction(
        || -> frame_support::storage::TransactionOutcome<sp_runtime::DispatchResult> {
            match Lib::do_guard_vote(who, pid, approve) {
                Ok(()) => frame_support::storage::TransactionOutcome::Commit(Ok(())),
                Err(e) => frame_support::storage::TransactionOutcome::Rollback(Err(e)),
            }
        },
    );
    if result.is_ok() {
        process_current_block();
    }
    result
}

/// 执行当前区块维护钩子，让 PASSED 立法提案完成一次异步业务执行。
pub fn process_current_block() {
    let now = System::block_number();
    engingcore::Pallet::<Test>::on_initialize(now);
}

/// 推进到提案 end 之后并触发到期结算(用于签署/会签超时测试)。
pub fn run_to_expiry(pid: u128) {
    let end = engingcore::pallet::Proposals::<Test>::get(pid)
        .expect("proposal exists")
        .end;
    // 到期桶挂在 end+1;推进到 end+1 并触发 on_initialize 自动结算。
    System::set_block_number(end + 1);
    engingcore::Pallet::<Test>::on_initialize(end + 1);
}

mod cases;

/// 在夹具中替换人员 CID 的双向绑定，岗位继续保存同一人员 CID。
fn rebind_fixture_person(reference: &AccountId32, wallet: &AccountId32) {
    use engingcore::CitizenIdentityReader;

    let person_cid = TestCitizenIdentityReader::person_cid(reference).unwrap();
    sp_io::storage::clear(&(b"fixture-wallet-person", reference).encode());
    sp_io::storage::set(&(b"fixture-wallet-person", wallet).encode(), &person_cid.encode());
    sp_io::storage::set(&(b"fixture-person-wallet", &person_cid).encode(), &wallet.encode());
}
