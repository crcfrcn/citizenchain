use super::*;
use core::cell::RefCell;
use std::collections::BTreeMap;

use frame_support::{
    assert_noop, assert_ok, derive_impl, traits::ConstU32, traits::Hooks, BoundedVec,
};
use frame_system as system;

// 引擎核心 storage / 类型 / trait(住在 votingengine 主 crate)。
// `use super::*` 拉进 internal-vote 自有的 pallet items(Pallet/Event/Error/Config/InternalVotesByTicket/...);
// 这里追加 votingengine 的 storage 与 trait 名,让测试代码用短名引用。
use primitives::cid::code::PRS;
use votingengine::pallet::{
    AdminSnapshot, AutoFinalizeDeadLetters, AutoFinalizeRetryStates, CurrentProposalYear,
    ExecutionRetryDeadlines, NextProposalId, PendingCleanupQueue, PendingCleanupQueueHead,
    PendingCleanupQueueTail, PendingExecutionRetryExpirations, PendingExpiryBucket,
    PendingProposalCleanups, PendingTerminalCleanups, ProposalDisplayId,
    ProposalExecutionRetryStates, Proposals, ProposalsByCid, ProposalsByCode, ProposalsByExpiry,
    ProposalsByOwner, ProposalsByYear, ScheduledCleanupHead, ScheduledCleanupTail,
    ScheduledCleanups, YearProposalCounter,
};
use votingengine::types::{
    code_bytes, CidNumber, InstitutionCode, ProposalSubject, RoleSubject, NJD, NRC, PMUL, PRB, PRC,
};
// 个人多签按账户键测试；公权、私权法人按机构 CID 键测试。
const PERSONAL_CODE: InstitutionCode = PMUL;
const PUBLIC_CODE: InstitutionCode = code_bytes("CGOV");
const PRIVATE_CODE: InstitutionCode = code_bytes("SFLP");
// 六个永久国家单例中的总统府：用于验证“不读取 CID 动态阈值、按提案快照过半”。
const PERMANENT_SINGLETON_CODE: InstitutionCode = PRS;
// joint mode storage 在 joint-vote sub-pallet
use primitives::cid::china::china_cb::CHINA_CB;
use primitives::cid::china::china_ch::CHINA_CH;
use primitives::cid::china::china_sf::{CHINA_SF, NATIONAL_JUDICIAL_YUAN_ADMINS};
use sp_runtime::{
    traits::{Hash as _, IdentityLookup},
    AccountId32, BuildStorage, DispatchError,
};
use votingengine::traits::{
    InternalAdminProvider, InternalVoteEngine, InternalVoteResultCallback, JointVoteEngine,
    JointVoteResultCallback,
};
use votingengine::{
    PendingCleanupStage, Proposal, ProposalExecutionOutcome, VoteCountU32, VoteCountU64,
    PROPOSAL_KIND_JOINT, STAGE_INTERNAL, STAGE_JOINT, STAGE_REFERENDUM, STATUS_EXECUTED,
    STATUS_EXECUTION_FAILED, STATUS_PASSED, STATUS_REJECTED, STATUS_VOTING,
};

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
    pub type VotingEngine = votingengine;

    #[runtime::pallet_index(2)]
    pub type InternalVote = super;

    #[runtime::pallet_index(3)]
    pub type JointVote = joint_vote;
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl system::Config for Test {
    type Block = Block;
    type AccountId = AccountId32;
    type Lookup = IdentityLookup<Self::AccountId>;
}

impl votingengine::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type MaxVoteNonceLength = ConstU32<64>;
    type MaxVoteSignatureLength = ConstU32<64>;
    type MaxAutoFinalizePerBlock = ConstU32<64>;
    type MaxAutoFinalizeWeightPerBlock = votingengine::BlockWeightFraction<Test, 4>;
    type MaxExecutionWeightPerBlock = votingengine::BlockWeightFraction<Test, 4>;
    type MaxCleanupWeightPerBlock = votingengine::BlockWeightFraction<Test, 8>;
    type MaxProposalsPerExpiry = ConstU32<128>;
    type MaxInternalProposalMutexBindings = ConstU32<256>;
    type MaxActiveProposals = ConstU32<10>;
    type MaxCleanupStepsPerBlock = ConstU32<3>;
    type MaxCleanupActivationsPerBlock = ConstU32<50>;
    type CleanupKeysPerStep = ConstU32<2>;
    type MaxProposalDataLen = ConstU32<4096>;
    type MaxProposalObjectLen = ConstU32<10_240>;
    type MaxModuleTagLen = ConstU32<32>;
    type MaxManualExecutionAttempts = ConstU32<3>;
    type ExecutionRetryGraceBlocks = frame_support::traits::ConstU64<216>;
    type MaxExecutionRetryDeadlinesPerBlock = ConstU32<128>;
    type MaxPendingRetryExpirationsPerBlock = ConstU32<16>;
    type CitizenIdentityReader = TestCitizenIdentityReader;
    type JointVoteResultCallback = TestJointVoteResultCallback;
    type InternalVoteResultCallback = TestInternalVoteResultCallback;
    type InternalAdminProvider = TestInternalAdminProvider;
    type MaxAdminsPerInstitution = ConstU32<32>;
    type TimeProvider = TestTimeProvider;
    type WeightInfo = ();
    type TrackHandlers = (InternalVote, (JointVote, ()));
    type LegislationVoteResultCallback = ();
    type ElectionVoteResultCallback = ();
}

impl crate::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type InstitutionRoleProvider = TestInternalAdminProvider;
    type WeightInfo = ();
}

impl joint_vote::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type InstitutionRoleProvider = TestInternalAdminProvider;
    type WeightInfo = ();
}

thread_local! {
    static TEST_POPULATION_COUNT: RefCell<u64> = const { RefCell::new(100) };
}
thread_local! {
    static TEST_INSTITUTION_THRESHOLDS: RefCell<BTreeMap<Vec<u8>, u32>> =
        const { RefCell::new(BTreeMap::new()) };
}
thread_local! {
    static TEST_NOW_SECS: RefCell<u64> = const { RefCell::new(DEFAULT_TEST_NOW_SECS) };
}
thread_local! {
    static JOINT_CALLBACK_SHOULD_FAIL: RefCell<bool> = const { RefCell::new(false) };
}
thread_local! {
    static JOINT_CALLBACK_OVERRIDE_STATUS: RefCell<Option<u8>> = const { RefCell::new(None) };
}
// 内部投票终态回调测试桩。
// INTERNAL_CALLBACK_SHOULD_FAIL = true → 异步执行回调返回 Err，
//   提案保留 PASSED，等待执行队列按退避策略重试。
// INTERNAL_CALLBACK_LOG 记录每次被调用的 (proposal_id, approved),
//   用于验证回调是否触发 / 触发参数是否正确。
thread_local! {
    static INTERNAL_CALLBACK_SHOULD_FAIL: RefCell<bool> = const { RefCell::new(false) };
}
thread_local! {
    static INTERNAL_CALLBACK_LOG: RefCell<Vec<(u64, bool)>> = const { RefCell::new(Vec::new()) };
}
thread_local! {
    static INTERNAL_CALLBACK_OVERRIDE_STATUS: RefCell<Option<u8>> = const { RefCell::new(None) };
}
thread_local! {
    static INTERNAL_TERMINAL_CLEANUP_LOG: RefCell<Vec<u64>> = const { RefCell::new(Vec::new()) };
}
thread_local! {
    static INTERNAL_TERMINAL_CLEANUP_SHOULD_FAIL: RefCell<bool> = const { RefCell::new(false) };
}
// 换绑模拟：(名册规范账户, 该身份当前绑定的钱包)。
// None = 未换绑 → 投票人解析恒按账户（与生产创世期/无 CID 管理员语义一致）。
thread_local! {
    static REBIND: RefCell<Option<(AccountId32, AccountId32)>> = const { RefCell::new(None) };
}

/// 模拟某名册规范账户的公民 CID 换绑到新钱包：新钱包解析到该规范账户，旧钱包掉权。
pub fn set_rebind(canonical: AccountId32, current_wallet: AccountId32) {
    REBIND.with(|cell| *cell.borrow_mut() = Some((canonical, current_wallet)));
}

pub struct TestCitizenIdentityReader;
pub struct TestJointVoteResultCallback;
pub struct TestInternalVoteResultCallback;
pub struct TestInternalAdminProvider;

fn bounded_cid(raw: &[u8]) -> CidNumber {
    raw.to_vec()
        .try_into()
        .expect("test CID should fit runtime bound")
}

fn nrc_cid() -> CidNumber {
    bounded_cid(CHINA_CB[0].cid_number.as_bytes())
}

fn prc_cid() -> CidNumber {
    bounded_cid(CHINA_CB[1].cid_number.as_bytes())
}

fn prb_cid() -> CidNumber {
    bounded_cid(CHINA_CH[0].cid_number.as_bytes())
}

fn njd_cid() -> CidNumber {
    bounded_cid(CHINA_SF[0].cid_number.as_bytes())
}

fn public_cid() -> CidNumber {
    bounded_cid(b"GD001-CGOV0-123456789-2026")
}

fn private_cid() -> CidNumber {
    bounded_cid(b"GD001-SFLP0-123456789-2026")
}

fn permanent_singleton_cid() -> CidNumber {
    let singleton = primitives::institution_constraints::singleton_institutions()
        .into_iter()
        .find(|item| item.code == PERMANENT_SINGLETON_CODE)
        .expect("PRS singleton must exist");
    bounded_cid(singleton.cid_number.as_bytes())
}

fn pending_personal_account() -> AccountId32 {
    AccountId32::new([77u8; 32])
}

fn pending_personal_admin(index: usize) -> AccountId32 {
    match index {
        0 => AccountId32::new([91u8; 32]),
        1 => AccountId32::new([92u8; 32]),
        _ => AccountId32::new([93u8; 32]),
    }
}

fn personal_account_id() -> AccountId32 {
    AccountId32::new([78u8; 32])
}

fn personal_admin(index: usize) -> AccountId32 {
    match index {
        0 => AccountId32::new([81u8; 32]),
        1 => AccountId32::new([82u8; 32]),
        _ => AccountId32::new([83u8; 32]),
    }
}

fn test_institution_admin(index: usize) -> AccountId32 {
    AccountId32::new([84u8.saturating_add(index as u8); 32])
}

fn test_institution_execution_account() -> AccountId32 {
    AccountId32::new([88u8; 32])
}

fn permanent_singleton_admin(index: usize) -> AccountId32 {
    AccountId32::new([101u8.saturating_add(index as u8); 32])
}

fn set_personal_threshold(threshold: u32) {
    ActivePersonalThresholds::<Test>::insert(personal_account_id(), threshold);
}

fn set_institution_threshold(cid_number: CidNumber, threshold: u32) {
    TEST_INSTITUTION_THRESHOLDS.with(|thresholds| {
        thresholds
            .borrow_mut()
            .insert(cid_number.to_vec(), threshold);
    });
}

impl TestInternalAdminProvider {
    fn institution_admins(
        institution_code: InstitutionCode,
        cid_number: &[u8],
    ) -> Option<sp_std::vec::Vec<AccountId32>> {
        match institution_code {
            NRC => CHINA_CB
                .iter()
                .take(1)
                .find(|n| n.cid_number.as_bytes() == cid_number)
                .map(|n| n.admins.iter().copied().map(AccountId32::new).collect()),
            PRC => CHINA_CB
                .iter()
                .skip(1)
                .find(|n| n.cid_number.as_bytes() == cid_number)
                .map(|n| n.admins.iter().copied().map(AccountId32::new).collect()),
            PRB => CHINA_CH
                .iter()
                .find(|n| n.cid_number.as_bytes() == cid_number)
                .map(|n| n.admins.iter().copied().map(AccountId32::new).collect()),
            NJD => CHINA_SF
                .first()
                .filter(|n| n.cid_number.as_bytes() == cid_number)
                .map(|_| {
                    NATIONAL_JUDICIAL_YUAN_ADMINS
                        .iter()
                        .copied()
                        .map(AccountId32::new)
                        .collect()
                }),
            PERMANENT_SINGLETON_CODE if cid_number == permanent_singleton_cid().as_slice() => {
                Some((0..3).map(permanent_singleton_admin).collect())
            }
            PUBLIC_CODE if cid_number == public_cid().as_slice() => Some(sp_std::vec![
                test_institution_admin(0),
                test_institution_admin(1),
                test_institution_admin(2),
            ]),
            PRIVATE_CODE if cid_number == private_cid().as_slice() => Some(sp_std::vec![
                test_institution_admin(0),
                test_institution_admin(1),
                test_institution_admin(2),
            ]),
            _ => None,
        }
    }
}

impl InternalAdminProvider<AccountId32> for TestInternalAdminProvider {
    fn is_institution_admin(
        institution_code: InstitutionCode,
        cid_number: &[u8],
        who: &AccountId32,
    ) -> bool {
        let who_bytes = who.encode();
        if who_bytes.len() != 32 {
            return false;
        }
        let mut who_arr = [0u8; 32];
        who_arr.copy_from_slice(&who_bytes);

        match institution_code {
            NRC => CHINA_CB
                .iter()
                .take(1)
                .find(|n| n.cid_number.as_bytes() == cid_number)
                .map(|n| n.admins.contains(&who_arr))
                .unwrap_or(false),
            PRC => CHINA_CB
                .iter()
                .skip(1)
                .find(|n| n.cid_number.as_bytes() == cid_number)
                .map(|n| n.admins.contains(&who_arr))
                .unwrap_or(false),
            PRB => CHINA_CH
                .iter()
                .find(|n| n.cid_number.as_bytes() == cid_number)
                .map(|n| n.admins.contains(&who_arr))
                .unwrap_or(false),
            NJD => CHINA_SF
                .first()
                .filter(|n| n.cid_number.as_bytes() == cid_number)
                .map(|_| NATIONAL_JUDICIAL_YUAN_ADMINS.contains(&who_arr))
                .unwrap_or(false),
            PERMANENT_SINGLETON_CODE if cid_number == permanent_singleton_cid().as_slice() => {
                (0..3).any(|index| permanent_singleton_admin(index) == *who)
            }
            PUBLIC_CODE if cid_number == public_cid().as_slice() => {
                (0..3).any(|index| test_institution_admin(index) == *who)
            }
            PRIVATE_CODE if cid_number == private_cid().as_slice() => {
                (0..3).any(|index| test_institution_admin(index) == *who)
            }
            _ => false,
        }
    }

    /// 换绑感知的投票人解析：未设 REBIND 时恒按账户（现状语义，故现有用例不受影响）。
    fn resolve_institution_voter(_cid_number: &[u8], caller: &AccountId32) -> Option<AccountId32> {
        REBIND.with(|cell| match cell.borrow().as_ref() {
            // 新钱包 → 解析到名册规范账户（换绑不掉权）
            Some((canonical, current)) if caller == current => Some(canonical.clone()),
            // 旧钱包（= 规范账户本身）→ CID 已不绑定它 → 掉权
            Some((canonical, _)) if caller == canonical => None,
            // 其余管理员按账户
            _ => Some(caller.clone()),
        })
    }

    fn institution_threshold(_institution_code: InstitutionCode, cid_number: &[u8]) -> Option<u32> {
        TEST_INSTITUTION_THRESHOLDS.with(|thresholds| thresholds.borrow().get(cid_number).copied())
    }

    fn is_personal_admin(personal_account_id: AccountId32, who: &AccountId32) -> bool {
        personal_account_id == self::personal_account_id()
            && [personal_admin(0), personal_admin(1), personal_admin(2)]
                .iter()
                .any(|admin| admin == who)
    }

    fn get_personal_admins(personal_account_id: AccountId32) -> Option<Vec<AccountId32>> {
        if personal_account_id == self::personal_account_id() {
            Some(sp_std::vec![
                personal_admin(0),
                personal_admin(1),
                personal_admin(2),
            ])
        } else {
            None
        }
    }

    fn is_pending_personal_admin(personal_account_id: AccountId32, who: &AccountId32) -> bool {
        personal_account_id == pending_personal_account()
            && [pending_personal_admin(0), pending_personal_admin(1)]
                .iter()
                .any(|admin| admin == who)
    }

    fn get_pending_personal_admins(personal_account_id: AccountId32) -> Option<Vec<AccountId32>> {
        (personal_account_id == pending_personal_account())
            .then(|| sp_std::vec![pending_personal_admin(0), pending_personal_admin(1)])
    }
}

impl votingengine::InstitutionRoleProvider<AccountId32> for TestInternalAdminProvider {
    fn is_active_assignment(cid_number: &[u8], who: &AccountId32, role_code: &[u8]) -> bool {
        Self::active_accounts_for_role(cid_number, role_code).contains(who)
    }

    fn active_accounts_for_role(cid_number: &[u8], role_code: &[u8]) -> Vec<AccountId32> {
        let Ok(cid_text) = core::str::from_utf8(cid_number) else {
            return Vec::new();
        };
        let Some(code) = votingengine::types::institution_code_from_cid_number(cid_text) else {
            return Vec::new();
        };
        let expected_role = test_institution_role(code);
        let is_test_second_role = code == PUBLIC_CODE && role_code == b"TEST_SECOND_ROLE";
        if role_code != expected_role && !is_test_second_role {
            return Vec::new();
        }
        Self::institution_admins(code, cid_number).unwrap_or_default()
    }
}

fn test_institution_role(code: InstitutionCode) -> &'static [u8] {
    if matches!(code, NRC | PRC) {
        primitives::governance_skeleton::ROLE_CODE_COMMITTEE_MEMBER
    } else if code == PRB {
        primitives::governance_skeleton::ROLE_CODE_DIRECTOR
    } else {
        b"TEST_ROLE"
    }
}

const DEFAULT_TEST_NOW_SECS: u64 = 1_782_864_000;

/// 测试用时间提供器：默认返回 2026 年中，可由单测覆盖为指定 UTC 秒。
pub struct TestTimeProvider;
impl frame_support::traits::UnixTime for TestTimeProvider {
    fn now() -> core::time::Duration {
        TEST_NOW_SECS.with(|secs| core::time::Duration::from_secs(*secs.borrow()))
    }
}
impl votingengine::CitizenIdentityReader<AccountId32> for TestCitizenIdentityReader {
    fn voting_subject(
        who: &AccountId32,
        _scope: &votingengine::PopulationScope,
    ) -> Option<votingengine::CitizenSubject<AccountId32>> {
        (who == &nrc_admin(0)).then(|| test_citizen_subject(who))
    }

    fn candidate_subject(
        who: &AccountId32,
        _scope: &votingengine::PopulationScope,
    ) -> Option<votingengine::CitizenSubject<AccountId32>> {
        (who == &nrc_admin(0)).then(|| test_citizen_subject(who))
    }

    fn population_data(
        scope: &votingengine::PopulationScope,
    ) -> Option<votingengine::PopulationData> {
        Some(votingengine::PopulationData {
            scope: scope.clone(),
            eligible_total: TEST_POPULATION_COUNT.with(|count| *count.borrow()),
            eligibility_revision: 1,
            eligibility_date: 20_000,
        })
    }

    fn voting_subject_at(
        who: &AccountId32,
        _population_data: &votingengine::PopulationData,
    ) -> Option<votingengine::CitizenSubject<AccountId32>> {
        (who == &nrc_admin(0)).then(|| test_citizen_subject(who))
    }
}

fn test_citizen_subject(who: &AccountId32) -> votingengine::CitizenSubject<AccountId32> {
    votingengine::CitizenSubject {
        cid_number: <AccountId32 as AsRef<[u8]>>::as_ref(who)
            .to_vec()
            .try_into()
            .expect("account fits CID"),
        account_id: who.clone(),
    }
}

impl JointVoteResultCallback for TestJointVoteResultCallback {
    fn on_joint_vote_finalized(
        vote_proposal_id: u64,
        _approved: bool,
    ) -> Result<ProposalExecutionOutcome, DispatchError> {
        if JOINT_CALLBACK_SHOULD_FAIL.with(|flag| *flag.borrow()) {
            Err(DispatchError::Other("joint callback failed"))
        } else {
            if let Some(status) = JOINT_CALLBACK_OVERRIDE_STATUS.with(|value| *value.borrow()) {
                return Ok(match status {
                    STATUS_EXECUTED => ProposalExecutionOutcome::Executed,
                    STATUS_EXECUTION_FAILED => ProposalExecutionOutcome::FatalFailed,
                    STATUS_PASSED => ProposalExecutionOutcome::RetryableFailed,
                    _ => ProposalExecutionOutcome::Ignored,
                });
            }
            let _ = vote_proposal_id;
            Ok(ProposalExecutionOutcome::Executed)
        }
    }
}

impl InternalVoteResultCallback for TestInternalVoteResultCallback {
    fn on_internal_vote_finalized(
        proposal_id: u64,
        approved: bool,
    ) -> Result<ProposalExecutionOutcome, DispatchError> {
        // 先记日志；thread_local 不参与 storage transaction，仅用于确认回调调用次数。
        INTERNAL_CALLBACK_LOG.with(|log| log.borrow_mut().push((proposal_id, approved)));
        if INTERNAL_CALLBACK_SHOULD_FAIL.with(|flag| *flag.borrow()) {
            Err(DispatchError::Other("internal callback failed"))
        } else {
            if let Some(status) = INTERNAL_CALLBACK_OVERRIDE_STATUS.with(|value| *value.borrow()) {
                return Ok(match status {
                    STATUS_EXECUTED => ProposalExecutionOutcome::Executed,
                    STATUS_EXECUTION_FAILED => ProposalExecutionOutcome::FatalFailed,
                    STATUS_PASSED => ProposalExecutionOutcome::RetryableFailed,
                    _ => ProposalExecutionOutcome::Ignored,
                });
            }
            Ok(if approved {
                ProposalExecutionOutcome::RetryableFailed
            } else {
                ProposalExecutionOutcome::Executed
            })
        }
    }

    fn on_execution_failed_terminal(proposal_id: u64) -> DispatchResult {
        if INTERNAL_TERMINAL_CLEANUP_SHOULD_FAIL.with(|flag| *flag.borrow()) {
            return Err(DispatchError::Other("internal terminal cleanup failed"));
        }
        INTERNAL_TERMINAL_CLEANUP_LOG.with(|log| log.borrow_mut().push(proposal_id));
        Ok(())
    }
}

pub fn new_test_ext() -> sp_io::TestExternalities {
    let storage = frame_system::GenesisConfig::<Test>::default()
        .build_storage()
        .expect("frame system genesis storage should build");
    let mut ext = sp_io::TestExternalities::new(storage);
    ext.execute_with(|| {
        TEST_POPULATION_COUNT.with(|count| *count.borrow_mut() = 100);
        TEST_INSTITUTION_THRESHOLDS.with(|thresholds| thresholds.borrow_mut().clear());
        TEST_NOW_SECS.with(|secs| *secs.borrow_mut() = DEFAULT_TEST_NOW_SECS);
        JOINT_CALLBACK_SHOULD_FAIL.with(|flag| *flag.borrow_mut() = false);
        JOINT_CALLBACK_OVERRIDE_STATUS.with(|value| *value.borrow_mut() = None);
        INTERNAL_CALLBACK_SHOULD_FAIL.with(|flag| *flag.borrow_mut() = false);
        INTERNAL_CALLBACK_OVERRIDE_STATUS.with(|value| *value.borrow_mut() = None);
        INTERNAL_CALLBACK_LOG.with(|log| log.borrow_mut().clear());
        INTERNAL_TERMINAL_CLEANUP_LOG.with(|log| log.borrow_mut().clear());
        INTERNAL_TERMINAL_CLEANUP_SHOULD_FAIL.with(|flag| *flag.borrow_mut() = false);
        REBIND.with(|cell| *cell.borrow_mut() = None); // 默认未换绑 → 投票人解析按账户
        set_personal_threshold(3);
        set_institution_threshold(public_cid(), 3);
        set_institution_threshold(private_cid(), 3);
        set_institution_threshold(nrc_cid(), primitives::count_const::NRC_INTERNAL_THRESHOLD);
        set_institution_threshold(prc_cid(), primitives::count_const::PRC_INTERNAL_THRESHOLD);
        set_institution_threshold(prb_cid(), primitives::count_const::PRB_INTERNAL_THRESHOLD);
        set_institution_threshold(njd_cid(), primitives::count_const::NJD_INTERNAL_THRESHOLD);
        set_institution_threshold(permanent_singleton_cid(), 2);
        System::set_block_number(1);
    });
    ext
}

fn subject_cids_for(actor_cid_number: &CidNumber) -> Vec<Vec<u8>> {
    sp_std::vec![actor_cid_number.to_vec()]
}

fn internal_mutex_for(
    actor_cid_number: CidNumber,
) -> Option<votingengine::InternalProposalMutexState> {
    VotingEngine::internal_proposal_mutex(ProposalSubject::InstitutionCid(actor_cid_number))
}

fn personal_mutex_for(
    personal_account_id: AccountId32,
) -> Option<votingengine::InternalProposalMutexState> {
    VotingEngine::internal_proposal_mutex(ProposalSubject::PersonalAccount(personal_account_id))
}

fn nrc_admin(index: usize) -> AccountId32 {
    AccountId32::new(CHINA_CB[0].admins[index])
}

fn njd_admin(index: usize) -> AccountId32 {
    AccountId32::new(NATIONAL_JUDICIAL_YUAN_ADMINS[index])
}

fn all_prc_institutions() -> Vec<(CidNumber, AccountId32)> {
    CHINA_CB
        .iter()
        .skip(1)
        .map(|n| {
            (
                bounded_cid(n.cid_number.as_bytes()),
                AccountId32::new(n.admins[0]),
            )
        })
        .collect()
}

fn all_prb_institutions() -> Vec<(CidNumber, AccountId32)> {
    CHINA_CH
        .iter()
        .map(|n| {
            (
                bounded_cid(n.cid_number.as_bytes()),
                AccountId32::new(n.admins[0]),
            )
        })
        .collect()
}

fn prc_admin(index: usize) -> AccountId32 {
    AccountId32::new(CHINA_CB[1].admins[index])
}

fn prb_admin(index: usize) -> AccountId32 {
    AccountId32::new(CHINA_CH[0].admins[index])
}

fn institution_admins(cid_number: CidNumber) -> Vec<AccountId32> {
    CHINA_CB
        .iter()
        .find(|n| n.cid_number.as_bytes() == cid_number.as_slice())
        .map(|n| n.admins.iter().copied().map(AccountId32::new).collect())
        .or_else(|| {
            CHINA_CH
                .iter()
                .find(|n| n.cid_number.as_bytes() == cid_number.as_slice())
                .map(|n| n.admins.iter().copied().map(AccountId32::new).collect())
        })
        .expect("institution should have admins")
}

fn institution_threshold(cid_number: CidNumber) -> usize {
    if cid_number == nrc_cid() {
        return primitives::count_const::NRC_INTERNAL_THRESHOLD as usize;
    }
    if CHINA_CB
        .iter()
        .skip(1)
        .any(|n| n.cid_number.as_bytes() == cid_number.as_slice())
    {
        return primitives::count_const::PRC_INTERNAL_THRESHOLD as usize;
    }
    if CHINA_CH
        .iter()
        .any(|n| n.cid_number.as_bytes() == cid_number.as_slice())
    {
        return primitives::count_const::PRB_INTERNAL_THRESHOLD as usize;
    }
    panic!("unknown institution");
}

fn cast_joint_votes_until_finalized(proposal_id: u64, cid_number: CidNumber, approve: bool) {
    let admins = institution_admins(cid_number.clone());
    let threshold = institution_threshold(cid_number.clone());
    let required_votes = if approve {
        threshold
    } else {
        admins.len().saturating_sub(threshold).saturating_add(1)
    };
    for admin in admins.into_iter().take(required_votes) {
        assert_ok!(submit_joint_vote(
            admin,
            proposal_id,
            cid_number.clone(),
            approve
        ));
    }
    let now = System::block_number();
    <VotingEngine as Hooks<u64>>::on_initialize(now);
}

fn submit_joint_vote(
    who: AccountId32,
    proposal_id: u64,
    cid_number: CidNumber,
    approve: bool,
) -> DispatchResult {
    // 测试 helper 直调底层 do_joint_vote,绕过 extrinsic 签名包装。
    let role_code = if CHINA_CH
        .iter()
        .any(|entry| entry.cid_number.as_bytes() == cid_number.as_slice())
    {
        primitives::governance_skeleton::ROLE_CODE_DIRECTOR
    } else {
        primitives::governance_skeleton::ROLE_CODE_COMMITTEE_MEMBER
    }
    .to_vec()
    .try_into()
    .expect("测试岗位码合法");
    <joint_vote::Pallet<Test>>::do_joint_vote(who, proposal_id, cid_number, role_code, approve)
}

fn set_population_data(eligible_total: u64) {
    TEST_POPULATION_COUNT.with(|count| *count.borrow_mut() = eligible_total);
}

fn create_joint_proposal_for(
    who: AccountId32,
    actor_cid_number: CidNumber,
    eligible_total: u64,
) -> u64 {
    try_create_joint_proposal_for(who, actor_cid_number, eligible_total)
        .expect("joint proposal should be created")
}

fn try_create_joint_proposal_for(
    who: AccountId32,
    actor_cid_number: CidNumber,
    eligible_total: u64,
) -> Result<u64, DispatchError> {
    set_population_data(eligible_total);
    let data = b"joint-test".to_vec();
    let hash = <Test as frame_system::Config>::Hashing::hash(data.as_slice());
    let mut business_object_hash = [0u8; 32];
    business_object_hash.copy_from_slice(hash.as_ref());
    let actor_text = core::str::from_utf8(actor_cid_number.as_slice())
        .map_err(|_| DispatchError::Other("invalid actor CID"))?;
    let actor_code = votingengine::types::institution_code_from_cid_number(actor_text)
        .ok_or(DispatchError::Other("invalid actor CID"))?;
    let proposer_role_code = if matches!(actor_code, NRC | PRC) {
        primitives::governance_skeleton::ROLE_CODE_COMMITTEE_MEMBER
    } else {
        primitives::governance_skeleton::ROLE_CODE_DIRECTOR
    };
    let proposer_subject =
        votingengine::types::AuthorizationSubject::Institution(votingengine::types::RoleSubject {
            cid_number: actor_cid_number.clone(),
            role_code: proposer_role_code
                .to_vec()
                .try_into()
                .map_err(|_| DispatchError::Other("invalid proposer role"))?,
        });
    let voter_subjects = CHINA_CB
        .iter()
        .map(|entry| {
            votingengine::types::AuthorizationSubject::Institution(
                votingengine::types::RoleSubject {
                    cid_number: bounded_cid(entry.cid_number.as_bytes()),
                    role_code: primitives::governance_skeleton::ROLE_CODE_COMMITTEE_MEMBER
                        .to_vec()
                        .try_into()
                        .expect("bounded committee role"),
                },
            )
        })
        .chain(CHINA_CH.iter().map(|entry| {
            votingengine::types::AuthorizationSubject::Institution(
                votingengine::types::RoleSubject {
                    cid_number: bounded_cid(entry.cid_number.as_bytes()),
                    role_code: primitives::governance_skeleton::ROLE_CODE_DIRECTOR
                        .to_vec()
                        .try_into()
                        .expect("bounded director role"),
                },
            )
        }))
        .collect();
    let module_tag: BoundedVec<u8, ConstU32<32>> = b"joint-test"
        .to_vec()
        .try_into()
        .map_err(|_| DispatchError::Other("invalid module tag"))?;
    let vote_plan = votingengine::types::VotePlanOf::try_new(
        votingengine::types::BusinessActionId {
            module_tag: module_tag.clone(),
            action_code: 0,
        },
        module_tag,
        proposer_subject,
        voter_subjects,
        votingengine::types::VotingEngineKind::Joint,
        business_object_hash,
    )
    .map_err(|_| DispatchError::Other("invalid joint vote plan"))?;
    <JointVote as JointVoteEngine<AccountId32>>::create_joint_proposal_with_data(
        who,
        actor_cid_number.to_vec(),
        vote_plan,
        data,
    )
}

fn set_joint_callback_should_fail(should_fail: bool) {
    JOINT_CALLBACK_SHOULD_FAIL.with(|flag| *flag.borrow_mut() = should_fail);
}

fn set_joint_callback_override_status(status: Option<u8>) {
    JOINT_CALLBACK_OVERRIDE_STATUS.with(|value| *value.borrow_mut() = status);
}

fn set_internal_callback_override_status(status: Option<u8>) {
    INTERNAL_CALLBACK_OVERRIDE_STATUS.with(|value| *value.borrow_mut() = status);
}

fn set_internal_terminal_cleanup_should_fail(should_fail: bool) {
    INTERNAL_TERMINAL_CLEANUP_SHOULD_FAIL.with(|flag| *flag.borrow_mut() = should_fail);
}

fn set_test_now_secs(secs: u64) {
    TEST_NOW_SECS.with(|value| *value.borrow_mut() = secs);
}

fn create_internal_proposal_via_engine(
    who: AccountId32,
    institution_code: InstitutionCode,
    actor_cid_number: CidNumber,
) -> u64 {
    let data = b"payload".to_vec();
    let vote_plan = internal_vote_plan(&actor_cid_number, &data);
    <InternalVote as InternalVoteEngine<AccountId32>>::create_institution_proposal_with_data(
        who,
        institution_code,
        actor_cid_number.to_vec(),
        None,
        subject_cids_for(&actor_cid_number),
        vote_plan,
        data,
    )
    .expect("internal proposal should be created")
}

fn internal_vote_plan(
    actor_cid_number: &CidNumber,
    data: &[u8],
) -> votingengine::types::VotePlanOf<AccountId32> {
    internal_vote_plan_with_owner(actor_cid_number, b"test", data)
}

fn internal_vote_plan_with_owner(
    actor_cid_number: &CidNumber,
    module_tag: &[u8],
    data: &[u8],
) -> votingengine::types::VotePlanOf<AccountId32> {
    let actor_text = core::str::from_utf8(actor_cid_number.as_slice()).expect("valid CID");
    let code = votingengine::types::institution_code_from_cid_number(actor_text)
        .expect("known institution code");
    let role_code = test_institution_role(code);
    let owner: BoundedVec<u8, ConstU32<32>> = module_tag.to_vec().try_into().expect("owner");
    let role_subject = votingengine::types::RoleSubject {
        cid_number: actor_cid_number.clone(),
        role_code: role_code.to_vec().try_into().expect("role code"),
    };
    votingengine::types::VotePlanOf::try_new(
        votingengine::types::BusinessActionId {
            module_tag: owner.clone(),
            action_code: 0,
        },
        owner,
        votingengine::types::AuthorizationSubject::Institution(role_subject.clone()),
        vec![votingengine::types::AuthorizationSubject::Institution(
            role_subject,
        )],
        votingengine::types::VotingEngineKind::Internal,
        sp_io::hashing::blake2_256(data),
    )
    .expect("valid internal vote plan")
}

fn create_pending_personal_proposal_via_engine(
    who: AccountId32,
    personal_account_id: AccountId32,
) -> u64 {
    <InternalVote as InternalVoteEngine<AccountId32>>::create_personal_account_create_proposal_with_data(
        who,
        personal_account_id,
        sp_std::vec![pending_personal_admin(0), pending_personal_admin(1)],
        2,
        b"test",
        b"payload".to_vec(),
    )
    .expect("pending personal proposal should be created")
}

fn create_personal_proposal_via_engine(who: AccountId32, personal_account_id: AccountId32) -> u64 {
    <InternalVote as InternalVoteEngine<AccountId32>>::create_personal_proposal_with_data(
        who,
        personal_account_id,
        b"test",
        b"payload".to_vec(),
    )
    .expect("personal proposal should be created")
}

fn create_admin_set_mutation_proposal_via_engine(
    who: AccountId32,
    personal_account_id: AccountId32,
) -> u64 {
    <InternalVote as InternalVoteEngine<AccountId32>>::create_personal_admin_change_proposal_with_data(
        who,
        personal_account_id,
        3,
        2,
        b"test",
        b"payload".to_vec(),
    )
    .expect("admin-set mutation proposal should be created")
}

/// 测试辅助:走公开 `internal_vote` extrinsic 投票。
///
/// 内部投票只能通过公开 call，
/// 此函数包裹 `RuntimeOrigin::signed(who)` 让测试代码保持简洁。
fn cast_internal_vote_via_extrinsic(
    who: AccountId32,
    proposal_id: u64,
    approve: bool,
) -> DispatchResult {
    // 测试 helper 直调底层 do_internal_vote；保留 extrinsic dispatch 的事务语义。
    let proposal = VotingEngine::proposals(proposal_id).expect("测试提案存在");
    let ticket_claim = if let Some(actor_cid_number) = proposal.actor_cid_number {
        let actor_text = core::str::from_utf8(actor_cid_number.as_slice()).expect("测试 CID 合法");
        let code = votingengine::types::institution_code_from_cid_number(actor_text)
            .expect("测试机构码合法");
        InternalVoteTicketClaim::InstitutionRole(
            test_institution_role(code)
                .to_vec()
                .try_into()
                .expect("测试岗位码合法"),
        )
    } else {
        InternalVoteTicketClaim::Personal
    };
    let result = frame_support::storage::with_transaction(
        || -> frame_support::storage::TransactionOutcome<DispatchResult> {
            match Pallet::<Test>::do_internal_vote(who, proposal_id, ticket_claim, approve) {
                Ok(()) => frame_support::storage::TransactionOutcome::Commit(Ok(())),
                Err(e) => frame_support::storage::TransactionOutcome::Rollback(Err(e)),
            }
        },
    );
    if result.is_ok() {
        let now = System::block_number();
        <VotingEngine as Hooks<u64>>::on_initialize(now);
    }
    result
}

/// 执行当前区块的维护钩子，让 PASSED 提案进入一次异步业务执行尝试。
fn process_current_block() {
    let now = System::block_number();
    <VotingEngine as Hooks<u64>>::on_initialize(now);
}

fn insert_joint_referendum_proposal(proposal_id: u64, eligible_total: u64, end: u64) {
    Proposals::<Test>::insert(
        proposal_id,
        Proposal {
            kind: PROPOSAL_KIND_JOINT,
            stage: STAGE_REFERENDUM,
            status: STATUS_VOTING,
            internal_code: None,
            actor_cid_number: Some(
                CHINA_CB[0]
                    .cid_number
                    .as_bytes()
                    .to_vec()
                    .try_into()
                    .expect("NRC CID fits runtime bound"),
            ),
            execution_account_id: None,
            subject_cid_numbers: Default::default(),
            start: System::block_number(),
            end,
        },
    );
    votingengine::ProposalPopulationSnapshots::<Test>::insert(
        proposal_id,
        votingengine::ProposalPopulationSnapshot {
            population_data: votingengine::PopulationData {
                scope: votingengine::PopulationScope::Country,
                eligible_total,
                eligibility_revision: proposal_id,
                eligibility_date: 20_000,
            },
            created_at: System::block_number(),
        },
    );
}

fn full_retry_deadline_bucket(seed: u64) -> BoundedVec<u64, ConstU32<128>> {
    (0..128u64)
        .map(|index| 2_000_000 + seed.saturating_mul(128) + index)
        .collect::<Vec<_>>()
        .try_into()
        .expect("test retry deadline bucket should fit capacity")
}

fn exhaust_cleanup_sequence(_current_block: u64) {
    ScheduledCleanupTail::<Test>::put(u64::MAX);
}

fn reset_cleanup_sequence(_current_block: u64) {
    ScheduledCleanupTail::<Test>::put(0);
}

fn fill_retry_deadline_window(from: u64) {
    for offset in 0..100u64 {
        ExecutionRetryDeadlines::<Test>::insert(from + offset, full_retry_deadline_bucket(offset));
    }
}

fn clear_retry_deadline_window(from: u64) {
    for offset in 0..100u64 {
        ExecutionRetryDeadlines::<Test>::remove(from + offset);
    }
}

mod cases;
mod dual_id;
