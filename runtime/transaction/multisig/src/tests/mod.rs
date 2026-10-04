#![cfg(test)]

use super::*;
use frame_support::{
    assert_noop, assert_ok, derive_impl,
    traits::{ConstU128, ConstU32, Currency, ExistenceRequirement, Hooks, WithdrawReasons},
};
use frame_system as system;
use primitives::cid::china::china_ch::CHINA_CH;
use primitives::cid::china::china_sf::CHINA_SF;
use primitives::cid::china::china_zf::CHINA_ZF;
use sp_core::{sr25519, Pair as PairT};
use sp_runtime::{traits::IdentityLookup, AccountId32, BuildStorage};
use votingengine::types::{
    code_bytes, institution_code_from_cid_number, InstitutionCode, FRG, NJD, PRC,
};
use votingengine::InstitutionRoleProvider as _;
use votingengine::{STATUS_EXECUTED, STATUS_REJECTED, STATUS_VOTING};

// 测试用机构码:个人多签 / 私权法人,均属"注册多签动态账户"。
const PERSONAL_CODE: InstitutionCode = PMUL;
const PRIVATE_CODE: InstitutionCode = code_bytes("SFLP");

type Balance = u128;
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
    pub type Balances = pallet_balances;

    #[runtime::pallet_index(2)]
    pub type VotingEngine = votingengine;

    #[runtime::pallet_index(99)]
    pub type InternalVote = internal_vote;

    #[runtime::pallet_index(4)]
    pub type MultisigTransfer = super;

    #[runtime::pallet_index(7)]
    pub type PersonalManage = personal_manage;

    #[runtime::pallet_index(8)]
    pub type PersonalAdmins = personal_admins;
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl system::Config for Test {
    type Block = Block;
    type AccountId = AccountId32;
    type Lookup = IdentityLookup<Self::AccountId>;
    type AccountData = pallet_balances::AccountData<Balance>;
}

impl pallet_balances::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type Balance = Balance;
    type DustRemoval = ();
    type ExistentialDeposit = ConstU128<1>;
    type AccountStore = System;
    type MaxLocks = ConstU32<0>;
    type MaxReserves = ();
    type ReserveIdentifier = [u8; 8];
    type FreezeIdentifier = RuntimeFreezeReason;
    type MaxFreezes = ConstU32<0>;
    type RuntimeHoldReason = RuntimeHoldReason;
    type RuntimeFreezeReason = RuntimeFreezeReason;
    type DoneSlashHandler = ();
    type WeightInfo = ();
}

pub struct TestAccountValidator;
impl entity_primitives::AccountValidator<AccountId32> for TestAccountValidator {
    fn is_valid(address: &AccountId32) -> bool {
        address != &AccountId32::new([0u8; 32])
    }
}

pub struct TestReservedAccountChecker;
impl entity_primitives::ReservedAccountGuard<AccountId32> for TestReservedAccountChecker {
    fn is_reserved(address: &AccountId32) -> bool {
        *address == AccountId32::new([0xAA; 32])
    }
}

pub struct TestCitizenIdentityReader;
impl votingengine::CitizenIdentityReader<AccountId32> for TestCitizenIdentityReader {
    fn voting_subject(
        who: &AccountId32,
        _scope: &votingengine::PopulationScope,
    ) -> Option<votingengine::CitizenSubject<AccountId32>> {
        Some(test_citizen_subject(who))
    }

    fn candidate_subject(
        who: &AccountId32,
        _scope: &votingengine::PopulationScope,
    ) -> Option<votingengine::CitizenSubject<AccountId32>> {
        Some(test_citizen_subject(who))
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

pub struct TestInternalAdminProvider;
impl votingengine::InternalAdminProvider<AccountId32> for TestInternalAdminProvider {
    fn is_institution_admin(
        institution_code: InstitutionCode,
        cid_number: &[u8],
        who: &AccountId32,
    ) -> bool {
        get_institution_admins(institution_code, cid_number)
            .map(|admins| admins.iter().any(|admin| admin == who))
            .unwrap_or(false)
    }

    fn institution_threshold(institution_code: InstitutionCode, cid_number: &[u8]) -> Option<u32> {
        primitives::cid::code::fixed_governance_pass_threshold(&institution_code).or_else(|| {
            INSTITUTION_THRESHOLDS.with(|thresholds| {
                thresholds
                    .borrow()
                    .get(&(institution_code, cid_number.to_vec()))
                    .copied()
            })
        })
    }

    fn is_personal_admin(personal_account_id: AccountId32, who: &AccountId32) -> bool {
        <personal_admins::Pallet<Test> as admin_primitives::AdminAccountQuery<AccountId32>>::is_active_account_admin(
            PMUL,
            personal_account_id,
            who,
        )
    }

    fn get_personal_admins(personal_account_id: AccountId32) -> Option<Vec<AccountId32>> {
        <personal_admins::Pallet<Test> as admin_primitives::AdminAccountQuery<AccountId32>>::active_account_admins(
            PMUL,
            personal_account_id,
        )
    }
}

pub struct TestInstitutionRoleProvider;

fn test_role_code(institution_code: InstitutionCode) -> &'static [u8] {
    if institution_code == PRB {
        primitives::governance_skeleton::ROLE_CODE_DIRECTOR
    } else {
        primitives::governance_skeleton::ROLE_CODE_COMMITTEE_MEMBER
    }
}

fn propose_transfer(
    origin: RuntimeOrigin,
    actor_cid_number: Option<CidNumber>,
    funding_account_id: AccountId32,
    beneficiary_account_id: AccountId32,
    amount: Balance,
    remark: BoundedVec<u8, ConstU32<256>>,
) -> sp_runtime::DispatchResult {
    let proposer_role_code = actor_cid_number.as_ref().map(|cid_number| {
        let cid_text = core::str::from_utf8(cid_number.as_slice()).expect("valid test CID");
        let code = votingengine::types::institution_code_from_cid_number(cid_text)
            .expect("known test institution");
        test_role_code(code)
            .to_vec()
            .try_into()
            .expect("test role fits")
    });
    MultisigTransfer::propose_transfer(
        origin,
        actor_cid_number,
        proposer_role_code,
        funding_account_id,
        beneficiary_account_id,
        amount,
        remark,
    )
}

impl votingengine::InstitutionRoleProvider<AccountId32> for TestInstitutionRoleProvider {
    fn is_active_assignment(cid_number: &[u8], who: &AccountId32, role_code: &[u8]) -> bool {
        Self::active_accounts_for_role(cid_number, role_code).contains(who)
    }

    fn active_accounts_for_role(cid_number: &[u8], role_code: &[u8]) -> Vec<AccountId32> {
        let Some(code) = core::str::from_utf8(cid_number)
            .ok()
            .and_then(votingengine::types::institution_code_from_cid_number)
        else {
            return Vec::new();
        };
        if test_role_code(code) != role_code {
            return Vec::new();
        }
        get_institution_admins(code, cid_number).unwrap_or_default()
    }
}

impl entity_primitives::InstitutionRoleAuthorizationQuery<AccountId32>
    for TestInstitutionRoleProvider
{
    fn role_has_permission(
        role_subject: &entity_primitives::RoleSubject<Vec<u8>, Vec<u8>>,
        business_action_id: &entity_primitives::BusinessActionId<Vec<u8>>,
        _operation: entity_primitives::RolePermissionOperation,
    ) -> bool {
        let Some(code) = core::str::from_utf8(role_subject.cid_number.as_slice())
            .ok()
            .and_then(votingengine::types::institution_code_from_cid_number)
        else {
            return false;
        };
        role_subject.role_code.as_slice() == test_role_code(code)
            && business_action_id.module_tag.as_slice() == crate::MODULE_TAG
            && matches!(
                business_action_id.action_code,
                entity_primitives::business_action::ACTION_MULTISIG_TRANSFER
                    | entity_primitives::business_action::ACTION_SAFETY_FUND_TRANSFER
                    | entity_primitives::business_action::ACTION_FEE_SWEEP_TO_MAIN
            )
    }

    fn is_authorized(
        admin: &AccountId32,
        role_subject: &entity_primitives::RoleSubject<Vec<u8>, Vec<u8>>,
        business_action_id: &entity_primitives::BusinessActionId<Vec<u8>>,
        operation: entity_primitives::RolePermissionOperation,
    ) -> bool {
        Self::role_has_permission(role_subject, business_action_id, operation)
            && Self::is_active_assignment(
                role_subject.cid_number.as_slice(),
                admin,
                role_subject.role_code.as_slice(),
            )
    }

    fn role_subjects_with_permission(
        cid_number: &[u8],
        business_action_id: &entity_primitives::BusinessActionId<Vec<u8>>,
        operation: entity_primitives::RolePermissionOperation,
    ) -> Vec<entity_primitives::RoleSubject<Vec<u8>, Vec<u8>>> {
        let Some(code) = core::str::from_utf8(cid_number)
            .ok()
            .and_then(votingengine::types::institution_code_from_cid_number)
        else {
            return Vec::new();
        };
        let role_subject = entity_primitives::RoleSubject {
            cid_number: cid_number.to_vec(),
            role_code: test_role_code(code).to_vec(),
        };
        Self::role_has_permission(&role_subject, business_action_id, operation)
            .then_some(role_subject)
            .into_iter()
            .collect()
    }
}

type ExtraAdminsByInstitution =
    std::collections::BTreeMap<(InstitutionCode, Vec<u8>), Vec<AccountId32>>;
type NamedAccountByInstitution = std::collections::BTreeMap<(Vec<u8>, Vec<u8>), AccountId32>;

thread_local! {
    static PROTECTED_ACCOUNT: core::cell::RefCell<Option<AccountId32>> = const { core::cell::RefCell::new(None) };
    static DENIED_SPEND_SOURCE: core::cell::RefCell<Option<AccountId32>> = const { core::cell::RefCell::new(None) };
    static EXTRA_ADMINS: core::cell::RefCell<ExtraAdminsByInstitution> =
        const { core::cell::RefCell::new(std::collections::BTreeMap::new()) };
    static INSTITUTION_THRESHOLDS: core::cell::RefCell<
        std::collections::BTreeMap<(InstitutionCode, Vec<u8>), u32>,
    > = const { core::cell::RefCell::new(std::collections::BTreeMap::new()) };
    static INSTITUTION_ACCOUNTS: core::cell::RefCell<
        std::collections::BTreeMap<AccountId32, (Vec<u8>, InstitutionCode)>,
    > = const { core::cell::RefCell::new(std::collections::BTreeMap::new()) };
    static INSTITUTION_NAMED_ACCOUNTS: core::cell::RefCell<NamedAccountByInstitution> =
        const { core::cell::RefCell::new(std::collections::BTreeMap::new()) };
}

/// 测试注入：机构管理员只按 `(institution_code, cid_number)` 寻址。
fn set_institution_admins(code: InstitutionCode, cid_number: &[u8], admins: Vec<AccountId32>) {
    EXTRA_ADMINS.with(|m| {
        m.borrow_mut().insert((code, cid_number.to_vec()), admins);
    });
}

fn set_institution_threshold(code: InstitutionCode, cid_number: &[u8], threshold: u32) {
    INSTITUTION_THRESHOLDS.with(|thresholds| {
        thresholds
            .borrow_mut()
            .insert((code, cid_number.to_vec()), threshold);
    });
}

fn get_institution_admins(code: InstitutionCode, cid_number: &[u8]) -> Option<Vec<AccountId32>> {
    EXTRA_ADMINS.with(|m| m.borrow().get(&(code, cid_number.to_vec())).cloned())
}

fn register_institution_account(
    account: AccountId32,
    cid_number: &[u8],
    institution_code: InstitutionCode,
) {
    INSTITUTION_ACCOUNTS.with(|accounts| {
        accounts
            .borrow_mut()
            .insert(account, (cid_number.to_vec(), institution_code));
    });
}

fn register_named_institution_account(
    account: AccountId32,
    cid_number: &[u8],
    institution_code: InstitutionCode,
    account_name: &[u8],
) {
    register_institution_account(account.clone(), cid_number, institution_code);
    INSTITUTION_NAMED_ACCOUNTS.with(|accounts| {
        accounts
            .borrow_mut()
            .insert((cid_number.to_vec(), account_name.to_vec()), account);
    });
}

pub struct TestInstitutionQuery;
impl entity_primitives::InstitutionMultisigQuery<AccountId32> for TestInstitutionQuery {
    fn lookup_institution_account(cid_number: &[u8], account_name: &[u8]) -> Option<AccountId32> {
        INSTITUTION_NAMED_ACCOUNTS.with(|accounts| {
            accounts
                .borrow()
                .get(&(cid_number.to_vec(), account_name.to_vec()))
                .cloned()
        })
    }

    fn lookup_cid(addr: &AccountId32) -> Option<Vec<u8>> {
        INSTITUTION_ACCOUNTS.with(|accounts| {
            accounts
                .borrow()
                .get(addr)
                .map(|(cid_number, _)| cid_number.clone())
        })
    }

    fn lookup_org(addr: &AccountId32) -> Option<InstitutionCode> {
        INSTITUTION_ACCOUNTS.with(|accounts| accounts.borrow().get(addr).map(|(_, code)| *code))
    }

    fn lookup_admin_config(
        addr: &AccountId32,
    ) -> Option<primitives::multisig::MultisigConfigSnapshot<AccountId32>> {
        let (cid_number, code) =
            INSTITUTION_ACCOUNTS.with(|accounts| accounts.borrow().get(addr).cloned())?;
        let admins = get_institution_admins(code, &cid_number)?;
        let admins_len = u32::try_from(admins.len()).ok()?;
        Some(primitives::multisig::MultisigConfigSnapshot {
            admins,
            admins_len,
            threshold: admins_len,
        })
    }

    fn account_exists(addr: &AccountId32) -> bool {
        INSTITUTION_ACCOUNTS.with(|accounts| accounts.borrow().contains_key(addr))
    }
}

/// 测试回调与生产逻辑使用同一链上费公式，并真实从明确付款账户扣费。
pub struct TestOnchainFeeCharger;
impl primitives::fee_policy::OnchainFeeCharger<AccountId32, Balance> for TestOnchainFeeCharger {
    fn charge(
        payer_account_id: &AccountId32,
        transaction_amount: Balance,
    ) -> Result<Balance, sp_runtime::DispatchError> {
        let fee = primitives::fee_policy::calculate_onchain_fee(transaction_amount);
        let imbalance = Balances::withdraw(
            payer_account_id,
            fee,
            WithdrawReasons::FEE,
            ExistenceRequirement::KeepAlive,
        )?;
        drop(imbalance);
        Ok(fee)
    }
}

pub struct TestProtectedSourceChecker;
impl entity_primitives::ProtectedSourceChecker<AccountId32> for TestProtectedSourceChecker {
    fn is_protected(address: &AccountId32) -> bool {
        PROTECTED_ACCOUNT.with(|pa| pa.borrow().as_ref() == Some(address))
    }
}

pub struct TestInstitutionAsset;
impl primitives::institution_asset::InstitutionAsset<AccountId32> for TestInstitutionAsset {
    fn can_spend(
        source: &AccountId32,
        _action: primitives::institution_asset::InstitutionAssetAction,
    ) -> bool {
        DENIED_SPEND_SOURCE.with(|blocked| blocked.borrow().as_ref() != Some(source))
    }
}

pub struct TestTimeProvider;
impl frame_support::traits::UnixTime for TestTimeProvider {
    fn now() -> core::time::Duration {
        core::time::Duration::from_secs(1_782_864_000) // 2026-07-01
    }
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
    type MaxCleanupStepsPerBlock = ConstU32<8>;
    type CleanupKeysPerStep = ConstU32<64>;
    type CitizenIdentityReader = TestCitizenIdentityReader;
    type JointVoteResultCallback = ();
    // 挂上本模块 Executor,3 组业务提案通过后自动走 callback 执行。
    type InternalVoteResultCallback = crate::InternalVoteExecutor<Test>;
    type InternalAdminProvider = TestInternalAdminProvider;
    // 与真实 runtime 一致(1989)。联邦注册局创世内置 215 管理员,mock 上限须覆盖。
    type MaxAdminsPerInstitution = ConstU32<1989>;
    type MaxProposalDataLen = ConstU32<1024>;
    type MaxProposalObjectLen = ConstU32<{ 10 * 1024 }>;
    type MaxModuleTagLen = ConstU32<32>;
    type MaxManualExecutionAttempts = ConstU32<3>;
    type ExecutionRetryGraceBlocks = frame_support::traits::ConstU64<216>;
    type MaxExecutionRetryDeadlinesPerBlock = ConstU32<128>;
    type MaxCleanupActivationsPerBlock = ConstU32<50>;
    type MaxPendingRetryExpirationsPerBlock = ConstU32<16>;
    type TimeProvider = TestTimeProvider;
    type WeightInfo = ();
    type TrackHandlers = (InternalVote, ());
    type LegislationVoteResultCallback = ();
    type ElectionVoteResultCallback = ();
}

impl internal_vote::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type InstitutionRoleProvider = TestInstitutionRoleProvider;
    type WeightInfo = ();
}

impl personal_manage::pallet::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type Currency = Balances;
    type InternalVoteEngine = internal_vote::Pallet<Test>;
    type AccountValidator = TestAccountValidator;
    type ReservedAccountChecker = TestReservedAccountChecker;
    type ProtectedSourceChecker = TestProtectedSourceChecker;
    type InstitutionAsset = TestInstitutionAsset;
    type PersonalAdminLifecycle = personal_admins::Pallet<Test>;
    type PersonalAdminQuery = personal_admins::Pallet<Test>;
    type OnchainFeeCharger = TestOnchainFeeCharger;
    type MaxAccountNameLength = ConstU32<128>;
    type MaxPersonalAccountAdmins = ConstU32<64>;
    type MinCreateAmount = ConstU128<111>;
    type WeightInfo = ();
}

impl personal_admins::pallet::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type InternalVoteEngine = internal_vote::Pallet<Test>;
    type MaxPersonalAccountAdmins = ConstU32<64>;
    type WeightInfo = ();
}

impl pallet::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type Currency = Balances;
    type InternalVoteEngine = internal_vote::Pallet<Test>;
    type InstitutionRoleAuthorization = TestInstitutionRoleProvider;
    type InstitutionAsset = TestInstitutionAsset;
    type ProtectedSourceChecker = TestProtectedSourceChecker;
    type MaxRemarkLen = ConstU32<256>;
    type OnchainFeeCharger = TestOnchainFeeCharger;
    // 测试 mock 把个人多签生命周期灌进 personal-manage，
    // 个人多签管理员灌进 personal-admins，动态阈值灌进 internal-vote。
    // InstitutionQuery 使用 CID 唯一主键的测试聚合查询。
    type PersonalQuery = personal_manage::Pallet<Test>;
    type InstitutionQuery = TestInstitutionQuery;
    type WeightInfo = ();
}

/// 测试 helper：从 `(institution_code, seed_context, index)` 派生 sr25519 keypair。
///
/// `seed_context` 只保证测试密钥确定性，不承担机构身份或管理员寻址语义。
/// 公钥的 32 字节直接作为 AccountId32,满足 `pubkey_from_accountid` 的铁律。
fn derive_admin_pair(
    institution_code: InstitutionCode,
    seed_context: &AccountId32,
    index: u8,
) -> (AccountId32, sr25519::Pair) {
    let mut seed_bytes = [0u8; 32];
    seed_bytes[0] = institution_code[0];
    seed_bytes[1] = index;
    // 后 30 字节由机构 AccountId 前 30 字节填充,保证不同机构的 seed 不同。
    let context_bytes: &[u8] = seed_context.as_ref();
    seed_bytes[2..32].copy_from_slice(&context_bytes[..30]);
    let pair = sr25519::Pair::from_seed(&seed_bytes);
    let account = AccountId32::new(pair.public().0);
    (account, pair)
}

fn nrc_admin(index: usize) -> AccountId32 {
    derive_admin_pair(NRC, &nrc_main_account(), index as u8).0
}

fn prc_admin(index: usize) -> AccountId32 {
    derive_admin_pair(PRC, &prc_main_account(), index as u8).0
}

fn prb_admin(index: usize) -> AccountId32 {
    derive_admin_pair(PRB, &prb_main_account(), index as u8).0
}

fn frg_admin(index: usize) -> AccountId32 {
    derive_admin_pair(FRG, &frg_main_account(), index as u8).0
}

fn njd_admin(index: usize) -> AccountId32 {
    derive_admin_pair(NJD, &njd_main_account(), index as u8).0
}

// 统一状态机整改:业务模块不再持有独立 vote/finalize call,投票统一走
// `InternalVote::cast`;`cast_transfer_votes_n` 直接用 admin 账户逐个投票。

fn nrc_main_account() -> AccountId32 {
    AccountId32::new(CHINA_CB[0].main_account)
}

fn nrc_fee_account() -> AccountId32 {
    AccountId32::new(CHINA_CB[0].fee_account)
}

fn prc_main_account() -> AccountId32 {
    AccountId32::new(CHINA_CB[1].main_account)
}

fn prc_fee_account() -> AccountId32 {
    AccountId32::new(CHINA_CB[1].fee_account)
}

fn prb_main_account() -> AccountId32 {
    AccountId32::new(CHINA_CH[0].main_account)
}

fn prb_fee_account() -> AccountId32 {
    AccountId32::new(CHINA_CH[0].fee_account)
}

fn frg_node() -> &'static primitives::cid::china::china_zf::ChinaZf {
    CHINA_ZF
        .iter()
        .find(|node| institution_code_from_cid_number(node.cid_number) == Some(FRG))
        .expect("FRG must exist in CHINA_ZF")
}

fn frg_main_account() -> AccountId32 {
    AccountId32::new(frg_node().main_account)
}

fn frg_fee_account() -> AccountId32 {
    AccountId32::new(frg_node().fee_account)
}

fn njd_node() -> &'static primitives::cid::china::china_sf::ChinaSf {
    CHINA_SF
        .iter()
        .find(|node| institution_code_from_cid_number(node.cid_number) == Some(NJD))
        .expect("NJD must exist in CHINA_SF")
}

fn njd_main_account() -> AccountId32 {
    AccountId32::new(njd_node().main_account)
}

fn njd_fee_account() -> AccountId32 {
    AccountId32::new(njd_node().fee_account)
}

fn nrc_actor_cid() -> CidNumber {
    protocol_cid_number(CHINA_CB[0].cid_number.as_bytes())
}

fn prc_actor_cid() -> CidNumber {
    protocol_cid_number(CHINA_CB[1].cid_number.as_bytes())
}

fn prb_actor_cid() -> CidNumber {
    protocol_cid_number(CHINA_CH[0].cid_number.as_bytes())
}

fn frg_actor_cid() -> CidNumber {
    protocol_cid_number(frg_node().cid_number.as_bytes())
}

fn njd_actor_cid() -> CidNumber {
    protocol_cid_number(njd_node().cid_number.as_bytes())
}

fn personal_account_id() -> AccountId32 {
    AccountId32::new([0x55; 32])
}

fn personal_account_admin(index: usize) -> AccountId32 {
    personal_account_pair(index).0
}

/// 注册个人账户(PERSONAL_CODE)的 admin sr25519 keypair helper。
/// seed 按 (PERSONAL_CODE, personal_account_id, index) 派生,保证确定性。
fn personal_account_pair(index: usize) -> (AccountId32, sr25519::Pair) {
    derive_admin_pair(PERSONAL_CODE, &personal_account_id(), index as u8)
}

fn personal_account_pairs(count: u8) -> Vec<(AccountId32, sr25519::Pair)> {
    (0..count)
        .map(|i| personal_account_pair(i as usize))
        .collect()
}

fn institution_account_id() -> AccountId32 {
    AccountId32::new([0x66; 32])
}

fn institution_fee_account() -> AccountId32 {
    AccountId32::new([0x67; 32])
}

fn institution_admin(index: usize) -> AccountId32 {
    institution_pair(index).0
}

/// 机构账户(PRIVATE_CODE / 0x05)的 admin sr25519 keypair helper。
fn institution_pair(index: usize) -> (AccountId32, sr25519::Pair) {
    derive_admin_pair(PRIVATE_CODE, &institution_account_id(), index as u8)
}

fn institution_pairs(count: u8) -> Vec<(AccountId32, sr25519::Pair)> {
    (0..count).map(|i| institution_pair(i as usize)).collect()
}

fn test_cid_number() -> CidNumber {
    primitives::cid::generator::generate_cid_number(
        primitives::cid::generator::GenerateCidNumberInput {
            public_key: "multisig-institution",
            p1: "0",
            province_code: "GD",
            province_name: "广东省",
            city_code: "001",
            city_name: "荔湾市",
            year: "2026",
            institution: "SFLP",
        },
    )
    .expect("test institution CID should generate")
    .into_bytes()
    .try_into()
    .expect("cid number should fit")
}

fn protocol_cid_number(raw: &[u8]) -> CidNumber {
    raw.to_vec().try_into().expect("protocol CID should fit")
}

fn insert_active_institution_account(
    account: &AccountId32,
    admins: BoundedVec<AccountId32, ConstU32<1989>>,
) {
    let cid_number = test_cid_number();
    register_named_institution_account(
        account.clone(),
        cid_number.as_slice(),
        PRIVATE_CODE,
        primitives::account_derive::RESERVED_NAME_MAIN,
    );
    let fee_account = institution_fee_account();
    register_named_institution_account(
        fee_account.clone(),
        cid_number.as_slice(),
        PRIVATE_CODE,
        primitives::account_derive::RESERVED_NAME_FEE,
    );
    let _ = Balances::deposit_creating(&fee_account, 10_000);
    set_institution_admins(PRIVATE_CODE, cid_number.as_slice(), admins.to_vec());
    set_institution_threshold(PRIVATE_CODE, cid_number.as_slice(), 2);
}

/// 为固定治理机构登记 CID、账户归属、岗位权限与任职，供 `VotePlan` 建立快照。
fn insert_active_fixed_institution_account(
    institution_code: InstitutionCode,
    account: &AccountId32,
    cid_number_raw: &[u8],
) {
    register_named_institution_account(
        account.clone(),
        cid_number_raw,
        institution_code,
        primitives::account_derive::RESERVED_NAME_MAIN,
    );
    let fee_account = match institution_code {
        NRC => nrc_fee_account(),
        PRC => prc_fee_account(),
        PRB => prb_fee_account(),
        FRG => frg_fee_account(),
        NJD => njd_fee_account(),
        _ => return,
    };
    register_named_institution_account(
        fee_account,
        cid_number_raw,
        institution_code,
        primitives::account_derive::RESERVED_NAME_FEE,
    );
}

/// 收款人：使用一个不是管理员也不是机构的普通地址
fn beneficiary_account_id() -> AccountId32 {
    AccountId32::new([99u8; 32])
}

/// 获取最近一次 create_internal_proposal 分配的 proposal_id。
fn last_proposal_id() -> u64 {
    votingengine::Pallet::<Test>::next_proposal_id().saturating_sub(1)
}

/// 返回 `(institution_code, seed_context)` 对应的前 `count` 个 sr25519 测试 keypair。
fn admin_pairs(
    institution_code: InstitutionCode,
    seed_context: AccountId32,
    count: u8,
) -> Vec<(AccountId32, sr25519::Pair)> {
    (0..count)
        .map(|i| derive_admin_pair(institution_code, &seed_context, i))
        .collect()
}

fn nrc_pairs(count: u8) -> Vec<(AccountId32, sr25519::Pair)> {
    admin_pairs(NRC, nrc_main_account(), count)
}

fn prc_pairs(count: u8) -> Vec<(AccountId32, sr25519::Pair)> {
    admin_pairs(PRC, prc_main_account(), count)
}

fn prb_pairs(count: u8) -> Vec<(AccountId32, sr25519::Pair)> {
    admin_pairs(PRB, prb_main_account(), count)
}

fn nrc_pass_count() -> usize {
    primitives::count_const::NRC_INTERNAL_THRESHOLD as usize
}

fn prc_pass_count() -> usize {
    primitives::count_const::PRC_INTERNAL_THRESHOLD as usize
}

fn prb_pass_count() -> usize {
    primitives::count_const::PRB_INTERNAL_THRESHOLD as usize
}

fn nrc_pass_pairs() -> Vec<(AccountId32, sr25519::Pair)> {
    nrc_pairs(primitives::count_const::NRC_INTERNAL_THRESHOLD as u8)
}

fn prc_pass_pairs() -> Vec<(AccountId32, sr25519::Pair)> {
    prc_pairs(primitives::count_const::PRC_INTERNAL_THRESHOLD as u8)
}

fn prb_pass_pairs() -> Vec<(AccountId32, sr25519::Pair)> {
    prb_pairs(primitives::count_const::PRB_INTERNAL_THRESHOLD as u8)
}

/// 测试辅助:走投票引擎公开 `internal_vote` extrinsic,
/// 让 `pairs` 前 `n` 个成员各投一张赞成票。
///
/// 发起人已在创建提案事务中自动赞成，调用方只传剩余补票人。
fn cast_transfer_votes_n(
    pairs: &[(AccountId32, sr25519::Pair)],
    n: usize,
    pid: u64,
) -> frame_support::dispatch::DispatchResult {
    let proposal =
        VotingEngine::proposals(pid).ok_or(votingengine::Error::<Test>::ProposalNotFound)?;
    let ticket_claim = match proposal.actor_cid_number {
        None => internal_vote::InternalVoteTicketClaim::Personal,
        Some(actor_cid_number) => {
            let institution_code = core::str::from_utf8(actor_cid_number.as_slice())
                .ok()
                .and_then(votingengine::types::institution_code_from_cid_number)
                .ok_or(votingengine::Error::<Test>::InvalidInstitution)?;
            internal_vote::InternalVoteTicketClaim::InstitutionRole(
                test_role_code(institution_code)
                    .to_vec()
                    .try_into()
                    .expect("test role fits protocol bound"),
            )
        }
    };
    for (admin, _pair) in pairs.iter().take(n) {
        <internal_vote::Pallet<Test>>::do_internal_vote(
            admin.clone(),
            pid,
            ticket_claim.clone(),
            true,
        )?;
        if VotingEngine::proposals(pid)
            .map(|proposal| proposal.status != STATUS_VOTING)
            .unwrap_or(true)
        {
            break;
        }
    }
    if VotingEngine::proposals(pid)
        .map(|proposal| proposal.status != STATUS_VOTING)
        .unwrap_or(false)
    {
        // 通过判定只入队；转账回调由 votingengine 维护管线异步执行。
        <VotingEngine as Hooks<u64>>::on_initialize(System::block_number());
    }
    Ok(())
}

fn new_test_ext() -> sp_io::TestExternalities {
    let mut storage = frame_system::GenesisConfig::<Test>::default()
        .build_storage()
        .expect("test storage should build");

    let balances = vec![
        (nrc_main_account(), 10_000),
        (prc_main_account(), 10_000),
        (prb_main_account(), 10_000),
        (frg_main_account(), 10_000),
        (njd_main_account(), 10_000),
        (nrc_fee_account(), 10_000),
        (prc_fee_account(), 10_000),
        (prb_fee_account(), 10_000),
        (frg_fee_account(), 10_000),
        (njd_fee_account(), 10_000),
    ];
    pallet_balances::GenesisConfig::<Test> {
        balances,
        ..Default::default()
    }
    .assimilate_storage(&mut storage)
    .expect("balances should assimilate");
    let mut ext: sp_io::TestExternalities = storage.into();
    ext.execute_with(|| {
        EXTRA_ADMINS.with(|admins| admins.borrow_mut().clear());
        INSTITUTION_THRESHOLDS.with(|thresholds| thresholds.borrow_mut().clear());
        INSTITUTION_ACCOUNTS.with(|accounts| accounts.borrow_mut().clear());
        INSTITUTION_NAMED_ACCOUNTS.with(|accounts| accounts.borrow_mut().clear());
        // 为储备治理三档注入 sr25519 派生 admin。
        // 注入数量必须覆盖 votingengine 的固定制度阈值,保证投票测试走真实状态机。
        // 管理员提供器只按 `(institution_code, actor_cid_number)` 读取本测试注入值。
        let frg = frg_main_account();
        let njd = njd_main_account();
        let dq = personal_account_id();
        let nrc_accts: Vec<AccountId32> = nrc_pass_pairs().into_iter().map(|(a, _)| a).collect();
        let prc_accts: Vec<AccountId32> = prc_pass_pairs().into_iter().map(|(a, _)| a).collect();
        let prb_accts: Vec<AccountId32> = prb_pass_pairs().into_iter().map(|(a, _)| a).collect();
        let frg_accts: Vec<AccountId32> = (0..primitives::count_const::FRG_INTERNAL_THRESHOLD)
            .map(|index| frg_admin(index as usize))
            .collect();
        let njd_accts: Vec<AccountId32> = (0..primitives::count_const::NJD_INTERNAL_THRESHOLD)
            .map(|index| njd_admin(index as usize))
            .collect();
        set_institution_admins(NRC, CHINA_CB[0].cid_number.as_bytes(), nrc_accts);
        set_institution_admins(PRC, CHINA_CB[1].cid_number.as_bytes(), prc_accts);
        set_institution_admins(PRB, CHINA_CH[0].cid_number.as_bytes(), prb_accts);
        set_institution_admins(FRG, frg_node().cid_number.as_bytes(), frg_accts);
        set_institution_admins(NJD, njd_node().cid_number.as_bytes(), njd_accts);
        insert_active_fixed_institution_account(
            NRC,
            &nrc_main_account(),
            CHINA_CB[0].cid_number.as_bytes(),
        );
        insert_active_fixed_institution_account(
            PRC,
            &prc_main_account(),
            CHINA_CB[1].cid_number.as_bytes(),
        );
        insert_active_fixed_institution_account(
            PRB,
            &prb_main_account(),
            CHINA_CH[0].cid_number.as_bytes(),
        );
        insert_active_fixed_institution_account(FRG, &frg, frg_node().cid_number.as_bytes());
        insert_active_fixed_institution_account(NJD, &njd, njd_node().cid_number.as_bytes());
        // 个人多签管理员由 personal-admins 提供；机构管理员统一按 CID 注入。
        let _ = dq;
    });
    ext
}

mod cases;
