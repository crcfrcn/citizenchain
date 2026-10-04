//! 集成测试：验证 citizen-identity 登记公民投票身份后触发 citizen-issuance 一次性奖励。

use citizen_identity::{
    CandidateIdentityPayload, CidNumberBound, CitizenIdentityAuthority, CitizenIdentityLevel,
    CitizenSex, CitizenStatus, VotingIdentityPayload,
};
use frame_support::{
    assert_ok, derive_impl, parameter_types,
    traits::{ConstU128, ConstU32, Hooks, VariantCountOf},
};
use frame_system as system;
use primitives::citizen_const::{CITIZEN_ISSUANCE_HIGH_REWARD, CITIZEN_ISSUANCE_MAX_COUNT};
use sp_runtime::{traits::IdentityLookup, BuildStorage};

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
    pub type CitizenIdentity = citizen_identity;
    #[runtime::pallet_index(3)]
    pub type CitizenIssuance = citizen_issuance;
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl system::Config for Test {
    type Block = Block;
    type AccountId = u64;
    type AccountData = pallet_balances::AccountData<u128>;
    type Lookup = IdentityLookup<Self::AccountId>;
}

impl pallet_balances::Config for Test {
    type MaxLocks = ConstU32<0>;
    type MaxReserves = ConstU32<0>;
    type ReserveIdentifier = [u8; 8];
    type Balance = u128;
    type RuntimeEvent = RuntimeEvent;
    type DustRemoval = ();
    type ExistentialDeposit = ConstU128<1>;
    type AccountStore = System;
    type WeightInfo = ();
    type FreezeIdentifier = RuntimeFreezeReason;
    type MaxFreezes = VariantCountOf<RuntimeFreezeReason>;
    type RuntimeHoldReason = RuntimeHoldReason;
    type RuntimeFreezeReason = RuntimeFreezeReason;
    type DoneSlashHandler = ();
}

parameter_types! {
    pub const MaxCitizenSignatureLength: u32 = 64;
    pub const MaxPopulationDaysPerBlock: u32 = 366;
    pub const MaxPopulationTransitionsPerBlock: u32 = 2_048;
    pub MaxPopulationMaintenanceWeightPerBlock: frame_support::weights::Weight = frame_support::weights::Weight::MAX;
}

/// 集成测试只验证模块衔接，授权与签名规则在 runtime 配置单测覆盖。
pub struct TestCitizenIdentityAuthority;
impl CitizenIdentityAuthority<u64, citizen_identity::pallet::SignatureOf<Test>>
    for TestCitizenIdentityAuthority
{
    fn can_manage_voting_identity(
        registrar: &u64,
        actor_cid_number: &[u8],
        actor_role_code: &[u8],
        residence_province_code: &[u8],
        residence_city_code: &[u8],
        _level: CitizenIdentityLevel,
        action_code: u32,
    ) -> bool {
        *registrar == 100
            && actor_cid_number == registrar_cid_number().as_slice()
            && actor_role_code == registrar_role_code().as_slice()
            && residence_province_code == b"43"
            && residence_city_code == b"4301"
            && matches!(action_code, 0 | 1 | 2 | 6)
    }

    fn verify_citizen_signature(
        _account_id: &u64,
        _payload: &[u8],
        signature: &citizen_identity::pallet::SignatureOf<Test>,
    ) -> bool {
        signature.as_slice() == b"valid"
    }

    fn verify_rebind_signature(
        _account_id: &u64,
        _payload: &[u8],
        signature: &citizen_identity::pallet::SignatureOf<Test>,
    ) -> bool {
        signature.as_slice() == b"valid"
    }

    fn can_manage_anonymous_cid(
        registrar: &u64,
        actor_cid_number: &[u8],
        actor_role_code: &[u8],
        action_code: u32,
    ) -> bool {
        *registrar == 100
            && actor_cid_number == registrar_cid_number().as_slice()
            && actor_role_code == registrar_role_code().as_slice()
            && matches!(action_code, 6)
    }

    fn verify_occupy_signature(
        _account_id: &u64,
        _payload: &[u8],
        signature: &citizen_identity::pallet::SignatureOf<Test>,
    ) -> bool {
        signature.as_slice() == b"valid"
    }

    fn verify_admin_rebind_signature(
        _account_id: &u64,
        _payload: &[u8],
        signature: &citizen_identity::pallet::SignatureOf<Test>,
    ) -> bool {
        signature.as_slice() == b"valid"
    }
}

/// 固定链上时间(2026-07-02 00:00 UTC),集成测试夹具护照落在有效期窗口内。
pub struct FixedTime;
impl frame_support::traits::UnixTime for FixedTime {
    fn now() -> core::time::Duration {
        core::time::Duration::from_secs(1_782_950_400)
    }
}

#[cfg(feature = "runtime-benchmarks")]
pub struct TestCitizenIdentityBenchmarkHelper;

#[cfg(feature = "runtime-benchmarks")]
impl citizen_identity::BenchmarkHelper<u64, citizen_identity::pallet::SignatureOf<Test>>
    for TestCitizenIdentityBenchmarkHelper
{
    fn signer() -> (sp_core::sr25519::Public, u64) {
        let public = sp_io::crypto::sr25519_generate(0.into(), None);
        let mut account_id = [0u8; 8];
        account_id.copy_from_slice(&public.0[..8]);
        (public, u64::from_le_bytes(account_id))
    }

    fn sign(
        signer: &sp_core::sr25519::Public,
        message: &[u8],
    ) -> citizen_identity::pallet::SignatureOf<Test> {
        sp_io::crypto::sr25519_sign(0.into(), signer, message)
            .expect("benchmark signer exists")
            .0
            .to_vec()
            .try_into()
            .expect("sr25519 signature fits")
    }
}

impl citizen_identity::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type MaxCitizenSignatureLength = MaxCitizenSignatureLength;
    type CitizenIdentityAuthority = TestCitizenIdentityAuthority;
    #[cfg(feature = "runtime-benchmarks")]
    type BenchmarkHelper = TestCitizenIdentityBenchmarkHelper;
    type OnVotingIdentityRegistered = CitizenIssuance;
    type TimeProvider = FixedTime;
    type MaxPopulationDaysPerBlock = MaxPopulationDaysPerBlock;
    type MaxPopulationTransitionsPerBlock = MaxPopulationTransitionsPerBlock;
    type MaxPopulationMaintenanceWeightPerBlock = MaxPopulationMaintenanceWeightPerBlock;
    type WeightInfo = ();
}

impl citizen_issuance::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type Currency = Balances;
    type WeightInfo = ();
}

fn new_test_ext() -> sp_io::TestExternalities {
    let storage = frame_system::GenesisConfig::<Test>::default()
        .build_storage()
        .expect("frame system genesis storage should build");
    let mut ext = sp_io::TestExternalities::new(storage);
    ext.execute_with(|| {
        System::set_block_number(10);
        citizen_identity::PopulationReadyDate::<Test>::put(20260702);
    });
    ext
}

/// 按 tag 生成真实规则公民 CID 号(格式/校验和/机构码全合规)。
fn citizen_cid_number(tag: &str) -> Vec<u8> {
    primitives::cid::generator::generate_cid_number(
        primitives::cid::generator::GenerateCidNumberInput {
            public_key: tag,
            p1: "1",
            province_code: "GD",
            province_name: "广东省",
            city_code: "001",
            city_name: "荔湾市",
            year: "2026",
            institution: "CTZN",
        },
    )
    .expect("citizen cid should generate")
    .into_bytes()
}

/// 测试注册局机构 CID；管理员账户 100 只作为外层签名 origin。
fn registrar_cid_number() -> CidNumberBound {
    primitives::cid::china::china_zf::CHINA_ZF[5]
        .cid_number
        .as_bytes()
        .to_vec()
        .try_into()
        .expect("registrar cid number should fit")
}

/// 测试管理员只有任职注册局省专员岗位后才能执行占号和身份登记。
fn registrar_role_code() -> citizen_identity::RoleCodeBound {
    b"PROVINCE_COMMISSIONER_43"
        .to_vec()
        .try_into()
        .expect("registrar role code should fit")
}

/// 占号先行:身份写入前必须先占号(注册局 CID + 管理员 100)。占即绑账户 1。
fn occupy_tag(tag: &str) {
    let expires_at = <FixedTime as frame_support::traits::UnixTime>::now()
        .as_secs()
        .saturating_add(300);
    assert_ok!(CitizenIdentity::occupy_cid(
        RuntimeOrigin::signed(100),
        registrar_cid_number(),
        registrar_role_code(),
        citizen_cid_number(tag)
            .try_into()
            .expect("cid number should fit"),
        1,
        expires_at,
        valid_signature(),
    ));
}

fn payload(account_id: u64, cid_number: &[u8]) -> VotingIdentityPayload<u64> {
    VotingIdentityPayload {
        cid_number: cid_number
            .to_vec()
            .try_into()
            .expect("cid number should fit"),
        account_id,
        passport_valid_from: 20260630,
        passport_valid_until: 20360630,
        citizen_status: CitizenStatus::Normal,
        residence_province_code: b"43".to_vec().try_into().expect("province should fit"),
        residence_city_code: b"4301".to_vec().try_into().expect("city should fit"),
        residence_town_code: b"4301001".to_vec().try_into().expect("town should fit"),
    }
}

/// 竞选身份载荷:投票载荷 + 竞选专属字段(出生地/姓名/性别/出生日期)。
/// 出生日期落在夹具当前日期(2026-07-02)之前且年龄 ≥ 16。
fn candidate_payload(account_id: u64, cid_number: &[u8]) -> CandidateIdentityPayload<u64> {
    CandidateIdentityPayload {
        voting: payload(account_id, cid_number),
        birth_province_code: b"43"
            .to_vec()
            .try_into()
            .expect("birth province should fit"),
        birth_city_code: b"4301".to_vec().try_into().expect("birth city should fit"),
        birth_town_code: b"4301001"
            .to_vec()
            .try_into()
            .expect("birth town should fit"),
        family_name: "李"
            .as_bytes()
            .to_vec()
            .try_into()
            .expect("family name should fit"),
        given_name: "四"
            .as_bytes()
            .to_vec()
            .try_into()
            .expect("given name should fit"),
        citizen_sex: CitizenSex::Male,
        birth_date: 20000101,
    }
}

fn valid_signature() -> citizen_identity::pallet::SignatureOf<Test> {
    b"valid".to_vec().try_into().expect("signature should fit")
}

/// 防重放三件套之一:授权过期时间。与 `occupy_tag` 同口径,落在 600 秒窗口内。
fn authorization_expires_at() -> u64 {
    <FixedTime as frame_support::traits::UnixTime>::now()
        .as_secs()
        .saturating_add(300)
}

/// 防重放三件套之一:授权版本必须**等于**链上当前值。
///
/// 版本随每次身份写入单调 +1,所以「先登记投票身份、再升级竞选身份」这类顺序用例里
/// 第二笔的期望版本已经不是 0。一律实时读取而不是写死常量 —— 写死会让顺序用例拿到
/// `IdentityVersionMismatch`,而那正是本文件要断言的成功路径。
fn identity_version(cid_number: &[u8]) -> u64 {
    let bound = CidNumberBound::try_from(cid_number.to_vec()).expect("cid number should fit");
    citizen_identity::VotingEligibilityVersionCount::<Test>::get(bound)
}

#[test]
fn register_voting_identity_triggers_reward_issuance() {
    new_test_ext().execute_with(|| {
        // 占号先行:身份写入前置。
        occupy_tag("0001");

        let cid_number = citizen_cid_number("0001");
        let cid_number_bound =
            CidNumberBound::try_from(cid_number.clone()).expect("cid number should fit");

        assert_ok!(CitizenIdentity::register_voting_identity(
            RuntimeOrigin::signed(100),
            registrar_cid_number(),
            registrar_role_code(),
            payload(1, &cid_number),
            identity_version(&cid_number),
            authorization_expires_at(),
            valid_signature(),
        ));

        assert_eq!(Balances::free_balance(1), 0);
        assert_eq!(citizen_issuance::PendingRewardCount::<Test>::get(), 1);
        CitizenIssuance::on_finalize(System::block_number());

        assert_eq!(Balances::free_balance(1), CITIZEN_ISSUANCE_HIGH_REWARD);
        assert_eq!(citizen_issuance::RewardedCount::<Test>::get(), 1);
        assert!(citizen_issuance::IdentityRewardClaimed::<Test>::contains_key(cid_number_bound));
        assert!(citizen_issuance::AccountRewarded::<Test>::contains_key(1));
    });
}

#[test]
fn candidate_first_onchain_triggers_reward_issuance() {
    new_test_ext().execute_with(|| {
        // 占号先行:身份写入前置。
        occupy_tag("0001");

        let cid_number = citizen_cid_number("0001");
        let cid_number_bound =
            CidNumberBound::try_from(cid_number.clone()).expect("cid number should fit");

        // 该公民从未登记投票身份,直接以竞选身份完成首次上链(第 3 条:不强制先投票)。
        assert_ok!(CitizenIdentity::upgrade_to_candidate_identity(
            RuntimeOrigin::signed(100),
            registrar_cid_number(),
            registrar_role_code(),
            candidate_payload(1, &cid_number),
            identity_version(&cid_number),
            authorization_expires_at(),
            valid_signature(),
        ));

        assert_eq!(Balances::free_balance(1), 0);
        assert_eq!(citizen_issuance::PendingRewardCount::<Test>::get(), 1);
        CitizenIssuance::on_finalize(System::block_number());

        // 竞选身份首次上链与投票身份首次登记同权,发放一次性高额认证奖励。
        assert_eq!(Balances::free_balance(1), CITIZEN_ISSUANCE_HIGH_REWARD);
        assert_eq!(citizen_issuance::RewardedCount::<Test>::get(), 1);
        assert!(citizen_issuance::IdentityRewardClaimed::<Test>::contains_key(cid_number_bound));
        assert!(citizen_issuance::AccountRewarded::<Test>::contains_key(1));
    });
}

#[test]
fn voting_first_then_candidate_upgrade_does_not_double_issue() {
    new_test_ext().execute_with(|| {
        // 占号先行:身份写入前置。
        occupy_tag("0001");

        // 先以投票身份首次上链,领取一次性公民币。
        assert_ok!(CitizenIdentity::register_voting_identity(
            RuntimeOrigin::signed(100),
            registrar_cid_number(),
            registrar_role_code(),
            payload(1, &citizen_cid_number("0001")),
            identity_version(&citizen_cid_number("0001")),
            authorization_expires_at(),
            valid_signature(),
        ));
        CitizenIssuance::on_finalize(System::block_number());
        assert_eq!(Balances::free_balance(1), CITIZEN_ISSUANCE_HIGH_REWARD);

        // 再升级为竞选身份:已有投票身份(old.is_some()),不再触发回调,不二次发币。
        assert_ok!(CitizenIdentity::upgrade_to_candidate_identity(
            RuntimeOrigin::signed(100),
            registrar_cid_number(),
            registrar_role_code(),
            candidate_payload(1, &citizen_cid_number("0001")),
            // 上一笔投票身份写入已把版本推进,这里必须实时读取而非沿用 0。
            identity_version(&citizen_cid_number("0001")),
            authorization_expires_at(),
            valid_signature(),
        ));
        assert_eq!(citizen_issuance::PendingRewardCount::<Test>::get(), 0);
        CitizenIssuance::on_finalize(System::block_number());

        assert_eq!(Balances::free_balance(1), CITIZEN_ISSUANCE_HIGH_REWARD);
        assert_eq!(citizen_issuance::RewardedCount::<Test>::get(), 1);
    });
}

#[test]
fn updating_existing_identity_does_not_issue_second_reward() {
    new_test_ext().execute_with(|| {
        // 占号先行:身份写入前置。
        occupy_tag("0001");

        assert_ok!(CitizenIdentity::register_voting_identity(
            RuntimeOrigin::signed(100),
            registrar_cid_number(),
            registrar_role_code(),
            payload(1, &citizen_cid_number("0001")),
            identity_version(&citizen_cid_number("0001")),
            authorization_expires_at(),
            valid_signature(),
        ));
        CitizenIssuance::on_finalize(System::block_number());
        let mut updated = payload(1, &citizen_cid_number("0001"));
        updated.passport_valid_until = 20370630;
        assert_ok!(CitizenIdentity::update_voting_identity(
            RuntimeOrigin::signed(100),
            registrar_cid_number(),
            registrar_role_code(),
            updated,
            // 更新走的是同一套防重放前置;此时版本已被上一笔登记推进。
            identity_version(&citizen_cid_number("0001")),
            authorization_expires_at(),
            valid_signature(),
        ));
        CitizenIssuance::on_finalize(System::block_number());

        assert_eq!(Balances::free_balance(1), CITIZEN_ISSUANCE_HIGH_REWARD);
        assert_eq!(citizen_issuance::RewardedCount::<Test>::get(), 1);
        assert!(
            citizen_issuance::IdentityRewardClaimed::<Test>::contains_key(
                CidNumberBound::try_from(citizen_cid_number("0001"))
                    .expect("cid number should fit")
            )
        );
    });
}

#[test]
fn max_reward_cap_is_applied_from_identity_callback() {
    new_test_ext().execute_with(|| {
        // 占号先行:身份写入前置。
        occupy_tag("CAP");

        citizen_issuance::RewardedCount::<Test>::put(CITIZEN_ISSUANCE_MAX_COUNT);

        assert_ok!(CitizenIdentity::register_voting_identity(
            RuntimeOrigin::signed(100),
            registrar_cid_number(),
            registrar_role_code(),
            payload(1, &citizen_cid_number("CAP")),
            identity_version(&citizen_cid_number("CAP")),
            authorization_expires_at(),
            valid_signature(),
        ));

        CitizenIssuance::on_finalize(System::block_number());

        assert_eq!(Balances::free_balance(1), 0);
        assert_eq!(
            citizen_issuance::RewardedCount::<Test>::get(),
            CITIZEN_ISSUANCE_MAX_COUNT
        );
    });
}
