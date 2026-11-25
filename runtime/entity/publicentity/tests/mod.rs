//! 装配公权实体测试Runtime、注册局身份及投票权限夹具。

#![cfg(test)]

extern crate alloc;

use super::*;
use frame_support::{
    derive_impl,
    traits::{ConstU128, ConstU32, Currency, ExistenceRequirement, Hooks, WithdrawReasons},
    BoundedVec,
};
use frame_system as system;
use sp_core::{sr25519, Pair as PairT};
use sp_runtime::{traits::IdentityLookup, AccountId32, BuildStorage};

use engingcore::types::{code_bytes, InstitutionCode};

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
    pub type VotingEngine = engingcore;

    #[runtime::pallet_index(3)]
    pub type InternalVote = internalvote;

    #[runtime::pallet_index(6)]
    pub type PublicEntity = super;
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
    type ExistentialDeposit = ConstU128<111>;
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

// ─── Trait mock 实现 ─────────────────────────────────────────────────────

pub struct TestAccountValidator;
impl primitives::multisig::AccountValidator<AccountId32> for TestAccountValidator {
    fn is_valid(account_id: &AccountId32) -> bool {
        account_id != &AccountId32::new([0u8; 32])
    }
}

pub struct TestReservedAccountChecker;
impl primitives::multisig::ReservedAccountGuard<AccountId32> for TestReservedAccountChecker {
    fn is_reserved(account_id: &AccountId32) -> bool {
        *account_id == AccountId32::new([0xAA; 32])
    }
}

pub struct TestProtectedSourceChecker;
impl primitives::multisig::ProtectedSourceChecker<AccountId32> for TestProtectedSourceChecker {
    fn is_protected(_address: &AccountId32) -> bool {
        false
    }
}

pub struct TestInstitutionAsset;
impl primitives::institution_asset::InstitutionAsset<AccountId32> for TestInstitutionAsset {
    fn can_spend(
        _source: &AccountId32,
        _action: primitives::institution_asset::InstitutionAssetAction,
    ) -> bool {
        true
    }
}

/// 测试查询仍以 publicentity 的 CID/账户双索引为机构账户真源；仅额外注入
/// 尚未进入本 pallet 存储的注册局出资账户，模拟创世注册局已有账户关系。
pub struct TestInstitutionQuery;
impl entitybase::InstitutionMultisigQuery<AccountId32> for TestInstitutionQuery {
    fn lookup_institution_account(cid_number: &[u8], account_name: &[u8]) -> Option<AccountId32> {
        <PublicEntity as entitybase::InstitutionMultisigQuery<AccountId32>>::lookup_institution_account(
            cid_number,
            account_name,
        )
    }

    fn account_belongs_to(cid_number: &[u8], addr: &AccountId32) -> bool {
        (cid_number == b"REGISTRY-CID" && addr == &registry_funding_account())
            || <PublicEntity as entitybase::InstitutionMultisigQuery<AccountId32>>::account_belongs_to(
                cid_number,
                addr,
            )
    }

    fn lookup_cid(addr: &AccountId32) -> Option<alloc::vec::Vec<u8>> {
        (addr == &registry_funding_account())
            .then(|| b"REGISTRY-CID".to_vec())
            .or_else(|| {
                <PublicEntity as entitybase::InstitutionMultisigQuery<AccountId32>>::lookup_cid(
                    addr,
                )
            })
    }

    fn lookup_org(addr: &AccountId32) -> Option<InstitutionCode> {
        <PublicEntity as entitybase::InstitutionMultisigQuery<AccountId32>>::lookup_org(addr)
    }


    fn account_exists(addr: &AccountId32) -> bool {
        addr == &registry_funding_account()
            || <PublicEntity as entitybase::InstitutionMultisigQuery<AccountId32>>::account_exists(
                addr,
            )
    }
}

/// 回调执行测试使用与生产一致的链上费公式，并从明确的机构费用账户扣款。
pub struct TestOnchainFeeCharger;
impl primitives::fee_policy::OnchainFeeCharger<AccountId32, Balance> for TestOnchainFeeCharger {
    fn charge(
        payer_account_id: &AccountId32,
        transaction_amount: Balance,
        has_remark: bool,
    ) -> Result<Balance, sp_runtime::DispatchError> {
        let fee = primitives::fee_policy::calculate_onchain_fee(transaction_amount)
            .saturating_add(if has_remark { primitives::fee_policy::FLAT_FEE } else { 0 },
        );
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

/// 注册局权限 mock:本测试包只验证 publicentity 的机构登记行为,真实 FRG/CREG
/// 省市边界由 runtime 配置层测试覆盖。发起管理员(origin)必须是注册局在册管理员。
pub struct TestRegistryAuthority;
impl crate::traits::RegistryAuthority<AccountId32> for TestRegistryAuthority {
    fn can_register_institution_origin(
        registrar: &AccountId32,
        actor_cid_number: &[u8],
        actor_role_code: &[u8],
        target_cid_number: &[u8],
        _target_institution_code: InstitutionCode,
    ) -> bool {
        registrar == &creator_account_id()
            && actor_cid_number == b"REGISTRY-CID"
            && actor_role_code == b"REGISTRY-ROLE"
            && !target_cid_number.is_empty()
    }
}

pub struct TestCitizenIdentityReader;
impl engingcore::CitizenIdentityReader<AccountId32> for TestCitizenIdentityReader {
    fn person_cid(who: &AccountId32) -> Option<engingcore::CidNumber> {
        PEOPLE.with(|people| people.borrow().iter().find(|(_, wallet)| *wallet == who).map(|(cid, _)| cid.clone()))
    }
    fn person_account(cid: &engingcore::CidNumber) -> Option<AccountId32> {
        PEOPLE.with(|people| people.borrow().get(cid).cloned())
    }
    #[cfg(feature = "runtime-benchmarks")]
    fn benchmark_seed_identity(who: &AccountId32, _: &engingcore::PopulationScope) {
        fixture_person(who, "CTZN");
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
}

fn test_citizen_subject(who: &AccountId32) -> engingcore::CitizenSubject<AccountId32> {
    engingcore::CitizenSubject {
        cid_number: <AccountId32 as AsRef<[u8]>>::as_ref(who)
            .to_vec()
            .try_into()
            .expect("account_id fits CID"),
        account_id: who.clone(),
    }
}

pub struct TestInternalVoteProvider;
impl engingcore::InternalVoteProvider<AccountId32> for TestInternalVoteProvider {


    fn institution_threshold(_institution_code: InstitutionCode, cid_number: &[u8]) -> Option<u32> {
        let cid = pallet::CidNumberOf::<Test>::try_from(cid_number.to_vec()).ok()?;
        pallet::InstitutionGovernanceThresholds::<Test>::get(cid)
    }
}

pub struct TestTimeProvider;
impl frame_support::traits::UnixTime for TestTimeProvider {
    fn now() -> core::time::Duration {
        core::time::Duration::from_secs(1_782_864_000) // 2026-07-01
    }
}

// ─── Pallet Config 实现 ─────────────────────────────────────────────────

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
    type CitizenIdentityReader = TestCitizenIdentityReader;
    type JointVoteResultCallback = ();
    // 接 publicentity 的 InternalVoteExecutor (lib.rs 末尾导出)
    type InternalVoteResultCallback = crate::InternalVoteExecutor<Test>;
    type InternalVoteProvider = TestInternalVoteProvider;
    // 公权机构多签上限=1989(同真实 runtime);全链创世测试含联邦注册局 215 管理员,须覆盖。
    // 个人多签上限是另一项 MaxPersonalAccountAdmins=64,不受此影响。
    type MaxSnapshotVoters = ConstU32<1989>;
    type MaxProposalDataLen = ConstU32<2048>;
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

impl internalvote::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type InstitutionRoleProvider = TestInstitutionRoleProvider;
    type WeightInfo = ();
}

pub struct TestInstitutionRoleProvider;

impl engingcore::InstitutionRoleProvider<AccountId32> for TestInstitutionRoleProvider {
    fn is_active_assignment(cid_number: &[u8], who: &AccountId32, role_code: &[u8]) -> bool {
        <crate::Pallet<Test> as entitybase::InstitutionRoleQuery<AccountId32>>::is_active_assignment(
            cid_number, who, role_code,
        )
    }

    fn active_persons_for_role(cid_number: &[u8], role_code: &[u8]) -> Vec<Vec<u8>> {
        crate::Pallet::<Test>::snapshot_role_persons(cid_number, role_code)
    }
}

pub fn grant_close_role(cid_number: &pallet::CidNumberOf<Test>) -> crate::RoleCodeOf {
    let role_code: crate::RoleCodeOf = b"TEST_CLOSE_ROLE".to_vec().try_into().expect("role fits");
    let role = entitybase::InstitutionRole {
        cid_number: cid_number.clone(),
        role_code: role_code.clone(),
        role_name: account_name("关闭账户岗位".as_bytes()),
        term_required: false,
        role_status: entitybase::InstitutionRoleStatus::Active,
        seats: 3,
    };
    pallet::InstitutionRoles::<Test>::insert(cid_number, &role_code, role);
    let assignments = (0..3).map(admin)
        .map(|wallet| entitybase::InstitutionAssignment {
            cid_number: cid_number.clone(),
            person_cid: fixture_person(&wallet, "CTZN"),
            role_code: role_code.clone(),
            term_start: 0,
            term_end: 0,
            assignment_source: entitybase::InstitutionAssignmentSource::InstitutionGovernance,
            assignment_source_ref: BoundedVec::default(),
            assignment_status: entitybase::InstitutionAssignmentStatus::Active,
        })
        .collect::<Vec<_>>();
    pallet::InstitutionRoleAssignments::<Test>::insert(
        cid_number,
        &role_code,
        crate::institution::role::RoleAssignmentsOf::<Test>::try_from(assignments)
            .expect("assignments fit"),
    );
    let permissions = [
        entitybase::RolePermissionOperation::Propose,
        entitybase::RolePermissionOperation::Vote,
    ]
    .into_iter()
    .map(|operation| entitybase::RoleBusinessPermission {
        role_subject: entitybase::RoleSubject {
            cid_number: cid_number.clone(),
            role_code: role_code.clone(),
        },
        business_action_id: entitybase::BusinessActionId {
            module_tag: crate::MODULE_TAG
                .to_vec()
                .try_into()
                .expect("module tag fits"),
            action_code: entitybase::business_action::ACTION_INSTITUTION_CLOSE,
        },
        operation,
    })
    .collect::<Vec<_>>();
    pallet::InstitutionRolePermissions::<Test>::insert(
        cid_number,
        &role_code,
        BoundedVec::try_from(permissions).expect("permissions fit"),
    );
    role_code
}

pub fn grant_governance_role(cid_number: &pallet::CidNumberOf<Test>) -> crate::RoleCodeOf {
    let role_code: crate::RoleCodeOf = b"TEST_GOVERNANCE_ROLE"
        .to_vec()
        .try_into()
        .expect("role fits");
    let role = entitybase::InstitutionRole {
        cid_number: cid_number.clone(),
        role_code: role_code.clone(),
        role_name: account_name("机构治理岗位".as_bytes()),
        term_required: false,
        role_status: entitybase::InstitutionRoleStatus::Active,
        seats: 2,
    };
    pallet::InstitutionRoles::<Test>::insert(cid_number, &role_code, role);
    let assignments = (0..2)
        .map(admin)
        .map(|wallet| entitybase::InstitutionAssignment {
            cid_number: cid_number.clone(),
            person_cid: fixture_person(&wallet, "CTZN"),
            role_code: role_code.clone(),
            term_start: 0,
            term_end: 0,
            assignment_source: entitybase::InstitutionAssignmentSource::InstitutionGovernance,
            assignment_source_ref: BoundedVec::default(),
            assignment_status: entitybase::InstitutionAssignmentStatus::Active,
        })
        .collect::<Vec<_>>();
    pallet::InstitutionRoleAssignments::<Test>::insert(
        cid_number,
        &role_code,
        crate::institution::role::RoleAssignmentsOf::<Test>::try_from(assignments)
            .expect("assignments fit"),
    );
    let permissions = [
        entitybase::RolePermissionOperation::Propose,
        entitybase::RolePermissionOperation::Vote,
    ]
    .into_iter()
    .map(|operation| entitybase::RoleBusinessPermission {
        role_subject: entitybase::RoleSubject {
            cid_number: cid_number.clone(),
            role_code: role_code.clone(),
        },
        business_action_id: entitybase::BusinessActionId {
            module_tag: crate::MODULE_TAG
                .to_vec()
                .try_into()
                .expect("module tag fits"),
            action_code: entitybase::business_action::ACTION_INSTITUTION_GOVERNANCE,
        },
        operation,
    })
    .collect::<Vec<_>>();
    pallet::InstitutionRolePermissions::<Test>::insert(
        cid_number,
        &role_code,
        BoundedVec::try_from(permissions).expect("permissions fit"),
    );
    role_code
}

pub struct TestInstitutionCapabilityPolicy;
impl entitybase::InstitutionCapabilityPolicy for TestInstitutionCapabilityPolicy {
    fn allows(
        _cid_number: &[u8],
        _business_action_id: &entitybase::BusinessActionId<alloc::vec::Vec<u8>>,
        _operation: entitybase::RolePermissionOperation,
    ) -> bool {
        true
    }
}

/// Observe the production registration callback through real pallet execution.
pub struct RecordAccountRegistration;
impl frame_support::traits::OnNewAccount<AccountId32> for RecordAccountRegistration {
    fn on_new_account(account: &AccountId32) {
        sp_io::storage::set(&codec::Encode::encode(&(b"evm-receive-registration", account)), &[1],
        );
    }
}
impl frame_support::traits::Get<frame_support::weights::Weight> for RecordAccountRegistration {
    fn get() -> frame_support::weights::Weight { frame_support::weights::Weight::zero() }
}



impl pallet::Config for Test {
    type OnAccountRegistered = RecordAccountRegistration;
    type RuntimeEvent = RuntimeEvent;
    type Currency = Balances;
    type InternalVoteEngine = internalvote::Pallet<Test>;
    type AccountValidator = TestAccountValidator;
    type ReservedAccountChecker = TestReservedAccountChecker;
    type ProtectedSourceChecker = TestProtectedSourceChecker;
    type InstitutionAsset = TestInstitutionAsset;
    type InstitutionQuery = TestInstitutionQuery;
    type OnchainFeeCharger = TestOnchainFeeCharger;
    type RegistryAuthority = TestRegistryAuthority;
    type SiblingInstitutionQuery = ();
    type InstitutionCapabilityPolicy = TestInstitutionCapabilityPolicy;
    type MaxRoleEntries = ConstU32<10>;
    type MaxCidNumberLength = ConstU32<{ primitives::core_const::CID_NUMBER_MAX_BYTES }>;
    type MaxAccountNameLength = ConstU32<128>;
    type MaxInstitutionAccounts = ConstU32<8>;
    type WeightInfo = ();
}

// ─── 测试 helper ────────────────────────────────────────────────────────

/// 派生 sr25519 admin 账户。seed 区分本测试套命名空间。
pub fn derive_admin_pair(index: u8) -> (AccountId32, sr25519::Pair) {
    let mut seed_bytes = [0u8; 32];
    seed_bytes[0] = 2; // publicentity 命名空间(personal 用 1)
    seed_bytes[1] = index;
    seed_bytes[2] = 0xAB;
    let pair = sr25519::Pair::from_seed(&seed_bytes);
    (AccountId32::new(pair.public().0), pair)
}

pub fn admin(index: u8) -> AccountId32 {
    derive_admin_pair(index).0
}

pub fn creator_account_id() -> AccountId32 {
    admin(0)
}

/// 注册局无私钥机构账户；创建机构时只由管理员签名，本金从此账户支出。
pub fn registry_funding_account() -> AccountId32 {
    AccountId32::new([0x31; 32])
}

pub fn beneficiary_account_id() -> AccountId32 {
    AccountId32::new([99u8; 32])
}

/// 用 primitives::cid 真实生成器产 CID 号字节。
///
/// tag 与旧假号一一对应:同 tag 产同号(保留去重语义),不同 tag 产不同号。
pub fn generated_cid_bytes(tag: &str, institution: &str) -> alloc::vec::Vec<u8> {
    primitives::cid::generator::generate_cid_number(
        primitives::cid::generator::GenerateCidNumberInput {
            public_key: tag,
            // 固定盈利策略机构码忽略 p1;可变/继承策略(如 UNIN)取非盈利。
            p1: "0",
            province_code: "GD",
            province_name: "广东省",
            city_code: "001",
            city_name: "荔湾市",
            year: 2026,
            institution,
        },
    )
    .expect("cid generates")
    .into_bytes()
}

/// 指定机构码的真 CID 号夹具。
pub fn generated_cid(tag: &str, institution: &str) -> pallet::CidNumberOf<Test> {
    BoundedVec::try_from(generated_cid_bytes(tag, institution)).expect("cid_number fits")
}

pub fn cid_full_name(s: &[u8]) -> pallet::AccountNameOf<Test> {
    BoundedVec::try_from(s.to_vec()).expect("cid_full_name fits")
}

pub fn cid_short_name(s: &[u8]) -> pallet::AccountNameOf<Test> {
    BoundedVec::try_from(s.to_vec()).expect("cid_short_name fits")
}

pub fn empty_town_code() -> pallet::AccountNameOf<Test> {
    BoundedVec::new()
}


pub fn account_name(s: &[u8]) -> pallet::AccountNameOf<Test> {
    BoundedVec::try_from(s.to_vec()).expect("account_name fits")
}

pub fn account_names_bv(names: &[&[u8]]) -> pallet::InstitutionAccountNamesOf<Test> {
    let v: alloc::vec::Vec<pallet::AccountNameOf<Test>> =
        names.iter().map(|n| account_name(n)).collect();
    BoundedVec::try_from(v).expect("account_id names fit")
}

pub fn last_proposal_id() -> u128 {
    engingcore::Pallet::<Test>::next_proposal_id().saturating_sub(1)
}

pub fn cast_yes_votes(admins: &[AccountId32], n: usize, pid: u128) -> sp_runtime::DispatchResult {
    use engingcore::STATUS_VOTING;
    for who in admins.iter().take(n) {
        <internalvote::Pallet<Test>>::do_internal_vote(
            who.clone(),
            pid,
            internalvote::InternalVoteTicketClaim::InstitutionRole(
                b"TEST_CLOSE_ROLE"
                    .to_vec()
                    .try_into()
                    .expect("test close role fits"),
            ),
            true,
        )?;
        if VotingEngine::proposals(pid)
            .map(|p| p.status != STATUS_VOTING)
            .unwrap_or(true)
        {
            break;
        }
    }
    if VotingEngine::proposals(pid)
        .map(|proposal| proposal.status != STATUS_VOTING)
        .unwrap_or(false)
    {
        <VotingEngine as Hooks<u64>>::on_initialize(System::block_number());
    }
    Ok(())
}

/// 测试用:以指定 actor CID + 岗位码 + 任职管理员账户发起自定义账户关闭提案。
pub fn propose_named_account_close(
    origin: RuntimeOrigin,
    actor_cid_number: pallet::CidNumberOf<Test>,
    account_id: AccountId32,
    beneficiary_account_id: AccountId32,
) -> sp_runtime::DispatchResult {
    let proposer_role_code: crate::RoleCodeOf =
        b"TEST_CLOSE_ROLE".to_vec().try_into().expect("role fits");
    PublicEntity::propose_close_public_institution(
        origin,
        actor_cid_number,
        proposer_role_code,
        account_id,
        beneficiary_account_id,
    )
}

/// 测试用:以本机构任职管理员账户 + `TEST_CLOSE_ROLE` 岗位发起新增账户提案。
/// 新增与关闭复用同一账户生命周期能力(`ACTION_INSTITUTION_CLOSE`),故同岗位即可提案。
pub fn propose_add_custom_account(
    origin: RuntimeOrigin,
    cid_number: pallet::CidNumberOf<Test>,
    names: &[&[u8]],
) -> sp_runtime::DispatchResult {
    let proposer_role_code: crate::RoleCodeOf =
        b"TEST_CLOSE_ROLE".to_vec().try_into().expect("role fits");
    PublicEntity::propose_add_institution_account(
        origin,
        cid_number,
        account_names_bv(names),
        proposer_role_code,
    )
}

/// 测试 setup:直接把一个自定义命名账户写入双索引,等价于新增账户投票通过后的落库结果。
/// 关闭账户等测试无需先跑一整轮新增投票即可获得可关闭的自定义账户。
pub fn insert_custom_account(cid_number: &pallet::CidNumberOf<Test>, name: &[u8]) -> AccountId32 {
    let stored_name = account_name(name);
    let (account_id, _) = PublicEntity::derive_institution_account(cid_number.as_slice(), name)
        .expect("custom account_id derives");
    pallet::InstitutionAccounts::<Test>::insert(
        cid_number,
        &stored_name,
        crate::InstitutionAccountInfo {
            account_id: account_id.clone(),
            initial_balance: 0,
            created_at: System::block_number(),
        },
    );
    pallet::AccountRegisteredCid::<Test>::insert(
        &account_id,
        crate::RegisteredInstitution {
            cid_number: cid_number.clone(),
            account_name: stored_name,
        },
    );
    account_id
}

pub fn new_test_ext() -> sp_io::TestExternalities {
    PEOPLE.with(|people| people.borrow_mut().clear());
    let storage = frame_system::GenesisConfig::<Test>::default()
        .build_storage()
        .expect("test storage should build");
    let mut ext: sp_io::TestExternalities = storage.into();
    ext.execute_with(|| {
        System::set_block_number(1);
    });
    ext
}

mod cases;

// 原仅供测试的账户构造，统一归所属测试目录。
mod accounts;


thread_local! {
    static PEOPLE: core::cell::RefCell<std::collections::BTreeMap<engingcore::CidNumber, AccountId32>> =
        const { core::cell::RefCell::new(std::collections::BTreeMap::new()) };
}

fn fixture_person(wallet: &AccountId32, kind: &str) -> engingcore::CidNumber {
    use engingcore::CitizenIdentityReader as _;
    if let Some(person_cid) = TestCitizenIdentityReader::person_cid(wallet) {
        return person_cid;
    }
    let person_cid = generated_cid_bytes(&alloc::format!("fixture-person-{wallet:?}"), kind).try_into().unwrap();
    bind_test_wallet(&person_cid, wallet.clone());
    person_cid
}

fn bind_test_wallet(person_cid: &engingcore::CidNumber, wallet: AccountId32) {
    PEOPLE.with(|people| {
        let mut people = people.borrow_mut();
        people.retain(|_, bound| *bound != wallet);
        people.insert(person_cid.clone(), wallet);
    });
}
