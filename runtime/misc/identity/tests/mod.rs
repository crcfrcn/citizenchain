#![cfg(test)]

use super::*;

use frame_support::{assert_noop, assert_ok, derive_impl, parameter_types, traits::Hooks};
use frame_system as system;
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
    pub type CitizenIdentity = super;
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl system::Config for Test {
    type Block = Block;
    type AccountId = u64;
    type Lookup = IdentityLookup<Self::AccountId>;
}

parameter_types! {
    pub const MaxCitizenSignatureLength: u32 = 64;
    pub const MaxPopulationDaysPerBlock: u32 = 366;
    pub const MaxPopulationTransitionsPerBlock: u32 = 2;
    pub MaxPopulationMaintenanceWeightPerBlock: frame_support::weights::Weight = frame_support::weights::Weight::MAX;
}

/// 固定链上时间:2026-07-02 00:00 UTC(UTC+8 为 2026-07-02 08:00,
/// 折算日期 20260702),让默认夹具护照(20260630-20360630)处于有效期窗口。
pub struct FixedTime;
impl frame_support::traits::UnixTime for FixedTime {
    fn now() -> core::time::Duration {
        TEST_TIME_SECS.with(|value| core::time::Duration::from_secs(value.get()))
    }
}

std::thread_local! {
    static TEST_TIME_SECS: core::cell::Cell<u64> = const { core::cell::Cell::new(1_782_950_400) };
}

fn test_time_secs() -> u64 {
    TEST_TIME_SECS.with(core::cell::Cell::get)
}

fn set_day_offset(days: i64) {
    TEST_TIME_SECS.with(|value| {
        let delta = days.unsigned_abs().saturating_mul(86_400);
        let timestamp = if days >= 0 {
            1_782_950_400u64.saturating_add(delta)
        } else {
            1_782_950_400u64.saturating_sub(delta)
        };
        value.set(timestamp);
    });
}

pub struct TestCitizenIdentityAuthority;
impl CitizenIdentityAuthority<u64, pallet::SignatureOf<Test>> for TestCitizenIdentityAuthority {
    fn can_manage_voting_identity(
        registrar: &u64,
        actor_cid_number: &[u8],
        actor_role_code: &[u8],
        residence_province_code: &[u8],
        residence_city_code: &[u8],
        _level: CitizenIdentityLevel,
        _action_code: u32,
    ) -> bool {
        *registrar == 100
            && actor_cid_number == registrar_cid_number().as_slice()
            && actor_role_code == registrar_role_code().as_slice()
            && residence_province_code == b"43"
            && residence_city_code == b"4301"
    }

    fn verify_citizen_signature(
        _account_id: &u64,
        _payload: &[u8],
        signature: &pallet::SignatureOf<Test>,
    ) -> bool {
        signature.as_slice() == b"valid"
    }

    fn verify_rebind_signature(
        account_id: &u64,
        payload: &[u8],
        signature: &pallet::SignatureOf<Test>,
    ) -> bool {
        signature.as_slice() == rebind_signature_bytes(account_id, payload)
    }

    fn can_manage_anonymous_cid(
        registrar: &u64,
        actor_cid_number: &[u8],
        actor_role_code: &[u8],
        _action_code: u32,
    ) -> bool {
        // 匿名 CID 全国号,不做辖区匹配:只认在册注册局(账户 100 + 岗位主体一致)。
        *registrar == 100
            && actor_cid_number == registrar_cid_number().as_slice()
            && actor_role_code == registrar_role_code().as_slice()
    }

    fn verify_occupy_signature(
        account_id: &u64,
        payload: &[u8],
        signature: &pallet::SignatureOf<Test>,
    ) -> bool {
        signature.as_slice() == rebind_signature_bytes(account_id, payload)
    }

    fn verify_admin_rebind_signature(
        account_id: &u64,
        payload: &[u8],
        signature: &pallet::SignatureOf<Test>,
    ) -> bool {
        signature.as_slice() == rebind_signature_bytes(account_id, payload)
    }
}

#[cfg(feature = "runtime-benchmarks")]
pub struct TestBenchmarkHelper;

#[cfg(feature = "runtime-benchmarks")]
impl BenchmarkHelper<u64, pallet::SignatureOf<Test>> for TestBenchmarkHelper {
    fn signer() -> (sp_core::sr25519::Public, u64) {
        let public = sp_io::crypto::sr25519_generate(0.into(), None);
        let mut account_id = [0u8; 8];
        account_id.copy_from_slice(&public.0[..8]);
        (public, u64::from_le_bytes(account_id))
    }

    fn sign(signer: &sp_core::sr25519::Public, message: &[u8]) -> pallet::SignatureOf<Test> {
        sp_io::crypto::sr25519_sign(0.into(), signer, message)
            .expect("benchmark signer exists")
            .0
            .to_vec()
            .try_into()
            .expect("sr25519 signature fits")
    }
}

impl Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type MaxCitizenSignatureLength = MaxCitizenSignatureLength;
    type CitizenIdentityAuthority = TestCitizenIdentityAuthority;
    #[cfg(feature = "runtime-benchmarks")]
    type BenchmarkHelper = TestBenchmarkHelper;
    type OnVotingIdentityRegistered = ();
    type TimeProvider = FixedTime;
    type MaxPopulationDaysPerBlock = MaxPopulationDaysPerBlock;
    type MaxPopulationTransitionsPerBlock = MaxPopulationTransitionsPerBlock;
    type MaxPopulationMaintenanceWeightPerBlock = MaxPopulationMaintenanceWeightPerBlock;
    type WeightInfo = ();
}

fn new_test_ext() -> sp_io::TestExternalities {
    let storage = frame_system::GenesisConfig::<Test>::default()
        .build_storage()
        .expect("frame system genesis storage should build");
    let mut ext = sp_io::TestExternalities::new(storage);
    set_day_offset(0);
    ext.execute_with(|| {
        System::set_block_number(10);
        PopulationReadyDate::<Test>::put(20260702);
    });
    ext
}

fn new_test_ext_with_cid_bindings(
    bindings: Vec<(CidNumberBound, u64, CidNumberBound)>,
) -> sp_io::TestExternalities {
    let mut storage = frame_system::GenesisConfig::<Test>::default()
        .build_storage()
        .expect("frame system genesis storage should build");
    GenesisConfig::<Test> {
        initial_cid_bindings: bindings,
    }
    .assimilate_storage(&mut storage)
    .expect("citizen identity genesis storage should build");
    sp_io::TestExternalities::new(storage)
}

fn code(bytes: &[u8]) -> AreaCodeBound {
    bytes.to_vec().try_into().expect("area code should fit")
}

fn cid(bytes: &[u8]) -> CidNumberBound {
    bytes.to_vec().try_into().expect("cid number should fit")
}

/// 测试注册局机构 CID；管理员账户 100 只作为外层 origin。
fn registrar_cid_number() -> CidNumberBound {
    cid(primitives::cid::china::china_zf::CHINA_ZF[5]
        .cid_number
        .as_bytes())
}

/// 按 tag 生成真实规则公民 CID 号(格式/校验和/机构码全合规)。
fn citizen_cid_number(tag: &str) -> alloc::vec::Vec<u8> {
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

fn family_name(bytes: &[u8]) -> FamilyName {
    bytes.to_vec().try_into().expect("family name should fit")
}

fn given_name(bytes: &[u8]) -> GivenName {
    bytes.to_vec().try_into().expect("given name should fit")
}

/// 测试注册局管理员必须以明确岗位主体发起业务，管理员账户本身不产生权限。
fn registrar_role_code() -> RoleCodeBound {
    b"PROVINCE_COMMISSIONER_43"
        .to_vec()
        .try_into()
        .expect("registrar role code should fit")
}

/// 占号先行:身份写入前必须先占号(注册局 CID + 管理员 100)。占即绑账户,默认账户 1。
fn occupy_tag(tag: &str) {
    occupy_tag_as(tag, 1);
}

/// 注册局占号并绑定指定账户(多占测试须给不同账户,一账户一 CID)。
fn occupy_tag_as(tag: &str, account: u64) {
    let cid_bytes = citizen_cid_number(tag);
    let expires_at = rebind_expires_at();
    assert_ok!(CitizenIdentity::occupy_cid(
        RuntimeOrigin::signed(100),
        registrar_cid_number(),
        registrar_role_code(),
        cid(&cid_bytes),
        account,
        expires_at,
        occupy_signature(account, &cid_bytes, expires_at),
    ));
}

/// 按 tag 生成真实规则公权机构号(市政府 CGOV),供家族拒绝用例。
fn public_cid_number(tag: &str) -> alloc::vec::Vec<u8> {
    primitives::cid::generator::generate_cid_number(
        primitives::cid::generator::GenerateCidNumberInput {
            public_key: tag,
            p1: "0",
            province_code: "GD",
            province_name: "广东省",
            city_code: "001",
            city_name: "荔湾市",
            year: "2026",
            institution: "CGOV",
        },
    )
    .expect("public cid should generate")
    .into_bytes()
}

fn valid_signature() -> pallet::SignatureOf<Test> {
    b"valid".to_vec().try_into().expect("signature should fit")
}

fn rebind_signature_bytes(account_id: &u64, payload: &[u8]) -> [u8; 32] {
    let mut material = account_id.encode();
    material.extend_from_slice(payload);
    sp_io::hashing::blake2_256(&material)
}

fn rebind_authorization(
    cid_number: &[u8],
    current_account_id: u64,
    new_account_id: u64,
    expected_binding_revision: u64,
    expires_at: u64,
) -> CidRebindAuthorization<<Test as frame_system::Config>::Hash, u64> {
    CidRebindAuthorization {
        genesis_hash: System::block_hash(0),
        cid_number: cid(cid_number),
        current_account_id,
        new_account_id,
        expected_binding_revision,
        expires_at,
    }
}

fn rebind_signature(
    signer_account_id: u64,
    cid_number: &[u8],
    current_account_id: u64,
    new_account_id: u64,
    expected_binding_revision: u64,
    expires_at: u64,
) -> pallet::SignatureOf<Test> {
    let payload = rebind_authorization(
        cid_number,
        current_account_id,
        new_account_id,
        expected_binding_revision,
        expires_at,
    )
    .encode();
    rebind_signature_bytes(&signer_account_id, &payload)
        .to_vec()
        .try_into()
        .expect("rebind signature should fit")
}

fn occupy_signature(
    account_id: u64,
    cid_number: &[u8],
    expires_at: u64,
) -> pallet::SignatureOf<Test> {
    let payload = CidOccupyAuthorization {
        genesis_hash: System::block_hash(0),
        cid_number: cid(cid_number),
        account_id,
        expected_binding_revision: 0,
        expires_at,
    }
    .encode();
    rebind_signature_bytes(&account_id, &payload)
        .to_vec()
        .try_into()
        .expect("occupy signature should fit")
}

fn rebind_expires_at() -> u64 {
    test_time_secs() + MAX_CID_AUTHORIZATION_LIFETIME_SECS
}

/// 身份写入授权的有效期上界；与换绑授权同窗口。
fn identity_expires_at() -> u64 {
    rebind_expires_at()
}

/// 读取该 CID 链上当前身份版本；尚无身份时为 0。
///
/// 正常调用必须提交当前版本；用旧版本提交即视为重放，链上拒绝。
fn identity_version(cid_number: &[u8]) -> u64 {
    VotingEligibilityVersionCount::<Test>::get(cid(cid_number))
}

/// 从投票身份载荷取当前身份版本。
fn voting_version(payload: &VotingIdentityPayload<u64>) -> u64 {
    VotingEligibilityVersionCount::<Test>::get(&payload.cid_number)
}

/// 从竞选身份载荷取当前身份版本。
fn candidate_version(payload: &CandidateIdentityPayload<u64>) -> u64 {
    VotingEligibilityVersionCount::<Test>::get(&payload.voting.cid_number)
}

fn voting_payload(account_id: u64, cid_number: &[u8]) -> VotingIdentityPayload<u64> {
    VotingIdentityPayload {
        cid_number: cid(cid_number),
        account_id,
        passport_valid_from: 20260630,
        passport_valid_until: 20360630,
        citizen_status: CitizenStatus::Normal,
        residence_province_code: code(b"43"),
        residence_city_code: code(b"4301"),
        residence_town_code: code(b"4301001"),
    }
}

fn candidate_payload(account_id: u64, cid_number: &[u8]) -> CandidateIdentityPayload<u64> {
    CandidateIdentityPayload {
        voting: voting_payload(account_id, cid_number),
        birth_province_code: code(b"43"),
        birth_city_code: code(b"4301"),
        birth_town_code: code(b"4301001"),
        family_name: family_name(b"Citizen"),
        given_name: given_name(b"One"),
        citizen_sex: CitizenSex::Female,
        // 固定时间 20260702 下年龄 26 周岁,满足最小年龄。
        birth_date: 20000131,
    }
}

fn town_scope() -> PopulationScope {
    PopulationScope::Town(code(b"43"), code(b"4301"), code(b"4301001"))
}

// ─── CidCount 有效 CID 计数 ─────────────────────────────────────────

// ───────────────── 身份写入防重放：版本 / 时间窗 ─────────────────

mod authorization;
mod population;
mod registry;

mod property;
