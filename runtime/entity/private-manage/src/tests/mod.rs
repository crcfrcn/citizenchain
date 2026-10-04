#![cfg(test)]

extern crate alloc;

use super::*;
use admin_primitives::InstitutionAdminQuery as _;
use frame_support::{
    derive_impl,
    traits::{ConstU128, ConstU32, Currency, ExistenceRequirement, Hooks, WithdrawReasons},
    BoundedVec,
};
use frame_system as system;
use sp_runtime::{traits::IdentityLookup, AccountId32, BuildStorage};
use votingengine::types::InstitutionCode;

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

    #[runtime::pallet_index(3)]
    pub type InternalVote = internal_vote;

    #[runtime::pallet_index(4)]
    pub type PrivateAdmins = private_admins;

    #[runtime::pallet_index(5)]
    pub type PrivateManage = super;
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
    type ExistentialDeposit = ConstU128<100>;
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

/// 测试查询仍委托 private-manage 的 CID/账户双索引；只注入创世注册局的
/// 出资账户归属，避免把管理员账户误当成生产机构账户真源。
pub struct TestInstitutionQuery;
impl entity_primitives::InstitutionMultisigQuery<AccountId32> for TestInstitutionQuery {
    fn lookup_institution_account(cid_number: &[u8], account_name: &[u8]) -> Option<AccountId32> {
        <PrivateManage as entity_primitives::InstitutionMultisigQuery<AccountId32>>::lookup_institution_account(
            cid_number,
            account_name,
        )
    }

    fn account_belongs_to(cid_number: &[u8], addr: &AccountId32) -> bool {
        (cid_number == b"GD001-FRG00-000000001-2026" && addr == &registry_funding_account())
            || <PrivateManage as entity_primitives::InstitutionMultisigQuery<AccountId32>>::account_belongs_to(
                cid_number,
                addr,
            )
    }

    fn lookup_cid(addr: &AccountId32) -> Option<alloc::vec::Vec<u8>> {
        (addr == &registry_funding_account())
            .then(|| b"GD001-FRG00-000000001-2026".to_vec())
            .or_else(|| {
                <PrivateManage as entity_primitives::InstitutionMultisigQuery<AccountId32>>::lookup_cid(addr)
            })
    }

    fn lookup_org(addr: &AccountId32) -> Option<InstitutionCode> {
        <PrivateManage as entity_primitives::InstitutionMultisigQuery<AccountId32>>::lookup_org(
            addr,
        )
    }

    fn lookup_admin_config(
        addr: &AccountId32,
    ) -> Option<primitives::multisig::MultisigConfigSnapshot<AccountId32>> {
        <PrivateManage as entity_primitives::InstitutionMultisigQuery<AccountId32>>::lookup_admin_config(addr)
    }

    fn account_exists(addr: &AccountId32) -> bool {
        addr == &registry_funding_account()
            || <PrivateManage as entity_primitives::InstitutionMultisigQuery<AccountId32>>::account_exists(addr)
    }
}

/// 回调执行测试按生产链上费公式从明确的机构费用账户扣款。
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

/// 这里只验证 pallet 边界；注册局辖区规则由 runtime 配置层测试负责。
pub struct TestRegistryAuthority;
impl crate::traits::RegistryAuthority<AccountId32> for TestRegistryAuthority {
    fn can_register_institution_origin(
        registrar_account_id: &AccountId32,
        actor_cid_number: &[u8],
        actor_role_code: &[u8],
        target_cid_number: &[u8],
        _target_institution_code: InstitutionCode,
    ) -> bool {
        registrar_account_id == &registrar()
            && actor_cid_number == b"GD001-FRG00-000000001-2026"
            && actor_role_code == b"REGISTRY-ROLE"
            && !target_cid_number.is_empty()
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
            .expect("account_id fits CID"),
        account_id: who.clone(),
    }
}

/// 投票引擎只按机构 CID 查询管理员；机构账户不参与授权寻址。
pub struct TestInternalAdminProvider;
impl votingengine::InternalAdminProvider<AccountId32> for TestInternalAdminProvider {
    fn is_institution_admin(
        institution_code: InstitutionCode,
        cid_number: &[u8],
        who: &AccountId32,
    ) -> bool {
        PrivateAdmins::is_institution_admin(institution_code, cid_number, who)
    }

    fn institution_threshold(_institution_code: InstitutionCode, cid_number: &[u8]) -> Option<u32> {
        let cid = pallet::CidNumberOf::<Test>::try_from(cid_number.to_vec()).ok()?;
        pallet::InstitutionGovernanceThresholds::<Test>::get(cid)
    }
}

pub struct TestTimeProvider;
impl frame_support::traits::UnixTime for TestTimeProvider {
    fn now() -> core::time::Duration {
        core::time::Duration::from_secs(1_782_864_000)
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
    type InternalVoteResultCallback = crate::InternalVoteExecutor<Test>;
    type InternalAdminProvider = TestInternalAdminProvider;
    type MaxAdminsPerInstitution = ConstU32<1989>;
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

impl internal_vote::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type InstitutionRoleProvider = TestInstitutionRoleProvider;
    type WeightInfo = ();
}

pub struct TestInstitutionRoleProvider;

impl votingengine::InstitutionRoleProvider<AccountId32> for TestInstitutionRoleProvider {
    fn is_active_assignment(cid_number: &[u8], who: &AccountId32, role_code: &[u8]) -> bool {
        <crate::Pallet<Test> as entity_primitives::InstitutionRoleQuery<AccountId32>>::is_active_assignment(
            cid_number,
            who,
            role_code,
        )
    }

    fn active_accounts_for_role(cid_number: &[u8], role_code: &[u8]) -> Vec<AccountId32> {
        <crate::Pallet<Test> as entity_primitives::InstitutionRoleQuery<AccountId32>>::active_accounts_for_role(
            cid_number,
            role_code,
        )
    }
}

pub fn grant_close_role(cid_number: &pallet::CidNumberOf<Test>) -> crate::RoleCodeOf {
    let role_code: crate::RoleCodeOf = b"TEST_CLOSE_ROLE".to_vec().try_into().expect("role fits");
    pallet::InstitutionRoles::<Test>::insert(
        cid_number,
        &role_code,
        entity_primitives::InstitutionRole {
            cid_number: cid_number.clone(),
            role_code: role_code.clone(),
            role_name: account_name("关闭账户岗位".as_bytes()),
            term_required: false,
            role_status: entity_primitives::InstitutionRoleStatus::Active,
        },
    );
    let assignments = [admin(1), admin(2)]
        .into_iter()
        .map(|account_id| entity_primitives::InstitutionAdminAssignment {
            cid_number: cid_number.clone(),
            account_id,
            role_code: role_code.clone(),
            term_start: 0,
            term_end: 0,
            assignment_source:
                entity_primitives::InstitutionAssignmentSource::InstitutionGovernance,
            assignment_source_ref: BoundedVec::default(),
            assignment_status: entity_primitives::InstitutionAssignmentStatus::Active,
        })
        .collect::<Vec<_>>();
    pallet::InstitutionRoleAssignments::<Test>::insert(
        cid_number,
        &role_code,
        crate::institution::role::RoleAssignmentsOf::<Test>::try_from(assignments)
            .expect("assignments fit"),
    );
    let permissions = [
        entity_primitives::RolePermissionOperation::Propose,
        entity_primitives::RolePermissionOperation::Vote,
    ]
    .into_iter()
    .map(|operation| entity_primitives::RoleBusinessPermission {
        role_subject: entity_primitives::RoleSubject {
            cid_number: cid_number.clone(),
            role_code: role_code.clone(),
        },
        business_action_id: entity_primitives::BusinessActionId {
            module_tag: crate::MODULE_TAG
                .to_vec()
                .try_into()
                .expect("module tag fits"),
            action_code: entity_primitives::business_action::ACTION_INSTITUTION_CLOSE,
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

impl private_admins::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type MaxAdminsPerInstitution = ConstU32<1989>;
    type ChainPhase = TestChainPhase; // 复用 Phase 2 可切换相位 mock
    type CitizenIdentityBinding = TestCitizenBinding;
    type LegalRepresentativeQuery = PrivateManage; // 读 InstitutionInfo.legal_representative
}

/// 岗位生命周期单测只验证 entity 约束，允许测试 CID 持有提交的业务动作权限。
pub struct TestInstitutionCapabilityPolicy;
impl entity_primitives::InstitutionCapabilityPolicy for TestInstitutionCapabilityPolicy {
    fn allows(
        _cid_number: &[u8],
        _business_action_id: &entity_primitives::BusinessActionId<alloc::vec::Vec<u8>>,
        _operation: entity_primitives::RolePermissionOperation,
    ) -> bool {
        true
    }
}

thread_local! {
    /// 测试相位开关;默认 Genesis(false),每个测试经 new_test_ext 复位。
    static IS_OPERATION: core::cell::Cell<bool> = const { core::cell::Cell::new(false) };
    /// 换绑测试用:某 CID 当前绑定的钱包(再次 bind 即换绑)。
    static CID_WALLET: core::cell::RefCell<Option<(alloc::vec::Vec<u8>, AccountId32)>> =
        const { core::cell::RefCell::new(None) };
}

/// 可切换的相位 mock:Genesis(默认)放行、Operation 强制 LR 岗四要素完整。
pub struct TestChainPhase;
impl admin_primitives::ChainPhaseCheck for TestChainPhase {
    fn is_operation() -> bool {
        IS_OPERATION.with(|cell| cell.get())
    }
}

/// 可换绑的 citizen-identity 绑定 mock:某 CID 只绑定一个钱包,可改绑以模拟换绑。
pub struct TestCitizenBinding;
impl admin_primitives::CitizenIdentityBindingQuery<AccountId32> for TestCitizenBinding {
    fn matches_citizen_account(cid_number: &[u8], account: &AccountId32) -> bool {
        CID_WALLET.with(|cell| {
            cell.borrow()
                .as_ref()
                .map(|(cid, wallet)| cid.as_slice() == cid_number && wallet == account)
                .unwrap_or(false)
        })
    }
}

pub fn set_operation_phase(is_operation: bool) {
    IS_OPERATION.with(|cell| cell.set(is_operation));
}

/// 把某 CID 绑定到某钱包(再次调用即换绑,旧钱包随即失去该 CID 的授权)。
pub fn bind_cid(cid_number: &[u8], wallet: AccountId32) {
    CID_WALLET.with(|cell| *cell.borrow_mut() = Some((cid_number.to_vec(), wallet)));
}

impl pallet::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type Currency = Balances;
    type InternalVoteEngine = internal_vote::Pallet<Test>;
    type ChainPhase = TestChainPhase;
    type AdminLifecycle = PrivateAdmins;
    type SiblingInstitutionQuery = ();
    type InstitutionAdminQuery = PrivateAdmins;
    type InstitutionCapabilityPolicy = TestInstitutionCapabilityPolicy;
    type AccountValidator = TestAccountValidator;
    type ReservedAccountChecker = TestReservedAccountChecker;
    type ProtectedSourceChecker = TestProtectedSourceChecker;
    type InstitutionAsset = TestInstitutionAsset;
    type InstitutionQuery = TestInstitutionQuery;
    type OnchainFeeCharger = TestOnchainFeeCharger;
    type RegistryAuthority = TestRegistryAuthority;
    type MaxAdmins = ConstU32<10>;
    type MaxCidNumberLength = ConstU32<{ primitives::core_const::CID_NUMBER_MAX_BYTES }>;
    type MaxAccountNameLength = ConstU32<128>;
    type MaxInstitutionAccounts = ConstU32<8>;
    type WeightInfo = ();
}

pub fn admin(index: u8) -> AccountId32 {
    let mut bytes = [0u8; 32];
    bytes[0] = 2;
    bytes[1] = index;
    bytes[2] = 0xAB;
    AccountId32::new(bytes)
}

pub fn registrar() -> AccountId32 {
    admin(0)
}

/// 注册局无私钥机构账户；管理员只签名，机构创建本金从此账户支出。
pub fn registry_funding_account() -> AccountId32 {
    AccountId32::new([0x32; 32])
}

pub fn beneficiary_account_id() -> AccountId32 {
    AccountId32::new([99u8; 32])
}

pub fn generated_cid(tag: &str, institution: &str) -> pallet::CidNumberOf<Test> {
    let number = primitives::cid::generator::generate_cid_number(
        primitives::cid::generator::GenerateCidNumberInput {
            public_key: tag,
            p1: "0",
            province_code: "GD",
            province_name: "广东省",
            city_code: "001",
            city_name: "荔湾市",
            year: "2026",
            institution,
        },
    )
    .expect("CID 夹具必须符合真实生成规则");
    number.into_bytes().try_into().expect("CID 长度必须受界")
}

pub fn account_name(value: &[u8]) -> pallet::AccountNameOf<Test> {
    value.to_vec().try_into().expect("账户名必须受界")
}

pub fn institution_admins(accounts: &[AccountId32]) -> crate::InstitutionAdminsInputOf<Test> {
    accounts
        .iter()
        .cloned()
        .map(|account_id| admin_primitives::Admin {
            account_id,
            cid_number: Default::default(),
            family_name: "管理".as_bytes().to_vec().try_into().expect("name fits"),
            given_name: "员".as_bytes().to_vec().try_into().expect("name fits"),
        })
        .collect::<alloc::vec::Vec<_>>()
        .try_into()
        .expect("admins fit")
}

pub fn initial_accounts(items: &[(&[u8], Balance)]) -> pallet::InstitutionInitialAccountsOf<Test> {
    items
        .iter()
        .map(|(name, amount)| crate::InstitutionInitialAccount {
            account_name: account_name(name),
            amount: *amount,
        })
        .collect::<alloc::vec::Vec<_>>()
        .try_into()
        .expect("初始账户列表必须受界")
}

pub fn create_institution(
    cid_number: pallet::CidNumberOf<Test>,
    institution_code: InstitutionCode,
    accounts: pallet::InstitutionInitialAccountsOf<Test>,
) -> sp_runtime::DispatchResult {
    let target_admins = [admin(1), admin(2)];
    // 测试机构均非 UNIN，无父级可传。
    let protocol_accounts =
        crate::institution::accounts::build_required_protocol_accounts::<Test>(&cid_number, None)?;
    let (created_accounts, _, _, _) = crate::institution::accounts::validate_initial_accounts::<
        Test,
    >(&cid_number, &protocol_accounts, None)?;
    PrivateManage::store_default_legal_representative_role(&cid_number)?;
    pallet::Institutions::<Test>::insert(
        &cid_number,
        crate::InstitutionInfo {
            cid_full_name: account_name("测试私权机构".as_bytes()),
            cid_short_name: account_name("测试机构".as_bytes()),
            town_code: BoundedVec::new(),
            legal_representative: None,
            institution_code,
            created_at: System::block_number(),
        },
    );
    for account_id in created_accounts {
        pallet::InstitutionAccounts::<Test>::insert(
            &cid_number,
            &account_id.account_name,
            crate::InstitutionAccountInfo {
                account_id: account_id.account_id.clone(),
                initial_balance: account_id.amount,
                created_at: System::block_number(),
            },
        );
        pallet::AccountRegisteredCid::<Test>::insert(
            &account_id.account_id,
            crate::RegisteredInstitution {
                cid_number: cid_number.clone(),
                account_name: account_id.account_name,
            },
        );
    }
    let admins = institution_admins(&target_admins);
    PrivateManage::set_institution_admins(&cid_number, institution_code, &admins)?;
    pallet::InstitutionGovernanceThresholds::<Test>::insert(&cid_number, 2);
    grant_close_role(&cid_number);

    // 创建 call 不再接收账户清单或初始入金。新增账户已改为机构自身提案+内部投票流程;
    // 测试 setup 直接落库命名账户,不再依赖新增账户投票路径(新增流程由 cases.rs 的
    // add_account_* 用例独立覆盖);余额注入只用于构造后续操作场景。
    for item in accounts.iter().filter(|item| {
        item.account_name.as_slice() != crate::RESERVED_NAME_MAIN
            && item.account_name.as_slice() != crate::RESERVED_NAME_FEE
    }) {
        let (account_id, _) = PrivateManage::derive_institution_account(
            cid_number.as_slice(),
            item.account_name.as_slice(),
        )
        .expect("命名账户必须可派生");
        pallet::InstitutionAccounts::<Test>::insert(
            &cid_number,
            &item.account_name,
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
                account_name: item.account_name.clone(),
            },
        );
    }
    for item in accounts.iter().filter(|item| item.amount > 0) {
        let account_id = account_of(&cid_number, item.account_name.as_slice());
        let _ = Balances::deposit_creating(&account_id, item.amount);
    }
    Ok(())
}

pub fn account_of(cid_number: &pallet::CidNumberOf<Test>, name: &[u8]) -> AccountId32 {
    PrivateManage::derive_institution_account(cid_number.as_slice(), name)
        .expect("机构账户必须可派生")
        .0
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
    let account_names = names
        .iter()
        .map(|name| account_name(name))
        .collect::<alloc::vec::Vec<_>>()
        .try_into()
        .expect("account_id names fit");
    PrivateManage::propose_add_institution_account(
        origin,
        cid_number,
        account_names,
        proposer_role_code,
    )
}

pub fn cast_yes_votes(proposal_id: u64) -> sp_runtime::DispatchResult {
    // 创建提案时发起人 admin(1) 已由引擎自动投赞成票，只需第二名管理员补票。
    <internal_vote::Pallet<Test>>::do_internal_vote(
        admin(2),
        proposal_id,
        internal_vote::InternalVoteTicketClaim::InstitutionRole(
            b"TEST_CLOSE_ROLE"
                .to_vec()
                .try_into()
                .expect("test close role fits"),
        ),
        true,
    )?;
    <VotingEngine as Hooks<u64>>::on_initialize(System::block_number());
    Ok(())
}

pub fn new_test_ext() -> sp_io::TestExternalities {
    set_operation_phase(false); // 每个测试默认 Genesis 期。
    CID_WALLET.with(|cell| *cell.borrow_mut() = None); // 清空换绑状态
    let mut storage = frame_system::GenesisConfig::<Test>::default()
        .build_storage()
        .expect("测试存储必须构建成功");
    pallet_balances::GenesisConfig::<Test> {
        balances: alloc::vec![
            (registry_funding_account(), 1_000_000),
            (beneficiary_account_id(), 100)
        ],
        dev_accounts: None,
    }
    .assimilate_storage(&mut storage)
    .expect("测试余额必须写入创世");
    let mut ext: sp_io::TestExternalities = storage.into();
    ext.execute_with(|| System::set_block_number(1));
    ext
}

mod cases;
