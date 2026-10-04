#![cfg(test)]

use super::*;
/// NodeGuard 镜像 `CidRecord` 的完整字段序和状态判别值；任一变化都属于 storage 契约变化。
#[test]
fn cid_record_scale_contract_matches_node_guard() {
    use codec::Encode;

    assert_eq!(CidRecordStatus::Active.encode(), vec![0]);
    assert_eq!(CidRecordStatus::Revoked.encode(), vec![1]);

    let record = CidRecord {
        registrar_cid_number: registrar_cid_number(),
        commitment: [2u8; 32],
        residence_province_code: b"GD".to_vec().try_into().expect("province"),
        residence_city_code: b"001".to_vec().try_into().expect("city"),
        status: CidRecordStatus::Revoked,
        registered_at: 8u32,
        revoked_at: Some(9u32),
    };
    assert_eq!(
        record.encode(),
        (
            registrar_cid_number().to_vec(),
            [2u8; 32],
            b"GD".to_vec(),
            b"001".to_vec(),
            CidRecordStatus::Revoked,
            8u32,
            Some(9u32),
        )
            .encode()
    );
}

/// 四端共同签名协议的字段声明顺序就是 SCALE 顺序；任何重排都必须让本测试失败。
#[test]
fn cid_authorization_scale_contract_keeps_canonical_field_order() {
    use codec::Encode;

    let genesis_hash = sp_core::H256::repeat_byte(0x11);
    let cid_number: CidNumberBound = b"CID-AUTH".to_vec().try_into().expect("authorization CID");
    let current_account_id = [0x22u8; 32];
    let new_account_id = [0x33u8; 32];

    let rebind = CidRebindAuthorization {
        genesis_hash,
        cid_number: cid_number.clone(),
        current_account_id,
        new_account_id,
        expected_binding_revision: 7,
        expires_at: 8,
    };
    assert_eq!(
        rebind.encode(),
        (
            genesis_hash,
            cid_number.clone(),
            current_account_id,
            new_account_id,
            7u64,
            8u64,
        )
            .encode()
    );

    let occupy = CidOccupyAuthorization {
        genesis_hash,
        cid_number: cid_number.clone(),
        account_id: new_account_id,
        expected_binding_revision: 0,
        expires_at: 9,
    };
    assert_eq!(
        occupy.encode(),
        (genesis_hash, cid_number, new_account_id, 0u64, 9u64).encode()
    );
}

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

#[test]
fn genesis_cid_binding_writes_active_registry_and_bidirectional_indexes() {
    use codec::Encode;

    let cid_number = cid(&citizen_cid_number("genesis-binding"));
    let registrar = registrar_cid_number();
    new_test_ext_with_cid_bindings(vec![(cid_number.clone(), 42, registrar.clone())]).execute_with(
        || {
            let record = CidRegistry::<Test>::get(&cid_number).expect("创世 CID 必须存在登记记录");
            assert_eq!(record.registrar_cid_number, registrar);
            assert_eq!(
                record.commitment,
                sp_io::hashing::blake2_256(&42u64.encode())
            );
            assert!(record.residence_province_code.is_empty());
            assert!(record.residence_city_code.is_empty());
            assert_eq!(record.status, CidRecordStatus::Active);
            assert_eq!(record.registered_at, 0);
            assert_eq!(record.revoked_at, None);
            assert_eq!(AccountIdByCid::<Test>::get(&cid_number), Some(42));
            assert_eq!(CidByAccountId::<Test>::get(42), Some(cid_number.clone()));
            assert_eq!(BindingRevisionByCid::<Test>::get(&cid_number), Some(1));
            assert!(!VotingIdentityByCid::<Test>::contains_key(&cid_number));
            assert!(!CandidateIdentityByCid::<Test>::contains_key(&cid_number));
        },
    );
}

#[test]
#[should_panic(expected = "创世 CID 不能重复")]
fn genesis_cid_binding_rejects_duplicate_cid() {
    let cid_number = cid(&citizen_cid_number("genesis-duplicate-cid"));
    let registrar = registrar_cid_number();
    let _ = new_test_ext_with_cid_bindings(vec![
        (cid_number.clone(), 42, registrar.clone()),
        (cid_number, 43, registrar),
    ]);
}

#[test]
#[should_panic(expected = "创世 CID 必须是合法的 CTZN 或 NATP 人主体号码")]
fn genesis_cid_binding_rejects_invalid_cid_number() {
    let _ = new_test_ext_with_cid_bindings(vec![(cid(b"not-a-cid"), 42, registrar_cid_number())]);
}

#[test]
#[should_panic(expected = "创世 AccountId 不能绑定多个 CID")]
fn genesis_cid_binding_rejects_duplicate_account_id() {
    let registrar = registrar_cid_number();
    let _ = new_test_ext_with_cid_bindings(vec![
        (
            cid(&citizen_cid_number("genesis-account-a")),
            42,
            registrar.clone(),
        ),
        (cid(&citizen_cid_number("genesis-account-b")), 42, registrar),
    ]);
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

#[test]
fn register_voting_identity_stores_identity_and_counts_scope() {
    new_test_ext().execute_with(|| {
        // 占号先行:身份写入前置。
        occupy_tag("0001");

        assert_ok!(CitizenIdentity::register_voting_identity(
            RuntimeOrigin::signed(100),
            registrar_cid_number(),
            registrar_role_code(),
            voting_payload(1, &citizen_cid_number("0001")),
            identity_version(&citizen_cid_number("0001")),
            identity_expires_at(),
            valid_signature(),
        ));

        assert!(VotingIdentityByCid::<Test>::contains_key(cid(
            &citizen_cid_number("0001")
        )));
        assert_eq!(
            AccountIdByCid::<Test>::get(cid(&citizen_cid_number("0001"))),
            Some(1)
        );
        assert_eq!(
            CidByAccountId::<Test>::get(1),
            Some(cid(&citizen_cid_number("0001")))
        );
        assert_eq!(CountryVotingCount::<Test>::get(), 1);
        assert_eq!(ProvinceVotingCount::<Test>::get(code(b"43")), 1);
        assert!(CitizenIdentity::voting_subject(&1, &town_scope()).is_some());
    });
}

#[test]
fn self_occupy_cid_binds_account_and_stays_anonymous() {
    use codec::Encode;
    new_test_ext().execute_with(|| {
        let cid_bytes = citizen_cid_number("self1");
        assert_ok!(CitizenIdentity::self_occupy_cid(
            RuntimeOrigin::signed(1),
            cid(&cid_bytes),
        ));
        // 占即绑:双向绑定成立。
        assert_eq!(AccountIdByCid::<Test>::get(cid(&cid_bytes)), Some(1));
        assert_eq!(CidByAccountId::<Test>::get(1), Some(cid(&cid_bytes)));
        // 登记记录:SELF registrar + 空居住地 + Active + commitment = blake2_256(account_id)。
        let rec = CidRegistry::<Test>::get(cid(&cid_bytes)).expect("record");
        assert_eq!(
            rec.registrar_cid_number.to_vec(),
            SELF_OCCUPY_REGISTRAR.to_vec()
        );
        assert!(rec.residence_province_code.is_empty());
        assert!(rec.residence_city_code.is_empty());
        assert_eq!(rec.status, CidRecordStatus::Active);
        assert_eq!(rec.commitment, sp_io::hashing::blake2_256(&1u64.encode()));
        // 匿名:无投票身份。
        assert!(!VotingIdentityByCid::<Test>::contains_key(cid(&cid_bytes)));
    });
}

#[test]
fn self_occupy_cid_is_idempotent_for_same_account_id() {
    new_test_ext().execute_with(|| {
        let cid_bytes = citizen_cid_number("self2");
        assert_ok!(CitizenIdentity::self_occupy_cid(
            RuntimeOrigin::signed(1),
            cid(&cid_bytes),
        ));
        // 同账户同 CID 重放幂等(commitment 同值,SELF registrar 同值),不报错。
        assert_ok!(CitizenIdentity::self_occupy_cid(
            RuntimeOrigin::signed(1),
            cid(&cid_bytes),
        ));
        assert_eq!(CidByAccountId::<Test>::get(1), Some(cid(&cid_bytes)));
    });
}

#[test]
fn self_occupy_cid_rejects_second_cid_for_same_account_id() {
    new_test_ext().execute_with(|| {
        assert_ok!(CitizenIdentity::self_occupy_cid(
            RuntimeOrigin::signed(1),
            cid(&citizen_cid_number("self3a")),
        ));
        // 一账户一 CID:同账户再占另一个 CID 被拒。
        assert_noop!(
            CitizenIdentity::self_occupy_cid(
                RuntimeOrigin::signed(1),
                cid(&citizen_cid_number("self3b")),
            ),
            Error::<Test>::AccountIdAlreadyBoundToAnotherCid
        );
    });
}

#[test]
fn self_occupy_cid_rejects_cid_taken_by_another_account_id() {
    new_test_ext().execute_with(|| {
        let cid_bytes = citizen_cid_number("self4");
        assert_ok!(CitizenIdentity::self_occupy_cid(
            RuntimeOrigin::signed(1),
            cid(&cid_bytes),
        ));
        // CID 已被账户 1 占绑,账户 2 占同一 CID 被拒(双向闭环)。
        assert_noop!(
            CitizenIdentity::self_occupy_cid(RuntimeOrigin::signed(2), cid(&cid_bytes)),
            Error::<Test>::CidAccountIdBindingMismatch
        );
    });
}

#[test]
fn self_occupy_cid_rejects_non_citizen_code() {
    new_test_ext().execute_with(|| {
        // 自助占号只认 CTZN;机构码(CGOV)CID 被 ensure_valid_citizen_cid 拒。
        assert_noop!(
            CitizenIdentity::self_occupy_cid(
                RuntimeOrigin::signed(1),
                cid(&public_cid_number("self5")),
            ),
            Error::<Test>::InvalidCitizenCode
        );
    });
}

#[test]
fn self_occupy_cid_accepts_resident_natp_type() {
    new_test_ext().execute_with(|| {
        // q3:用户可自选 NATP(居民)作为匿名身份类型(与 CTZN 公民并列)。
        let natp = primitives::cid::generator::generate_cid_number(
            primitives::cid::generator::GenerateCidNumberInput {
                public_key: "natp-self",
                p1: "1",
                province_code: "",
                province_name: "",
                city_code: "",
                city_name: "",
                year: "2026",
                institution: "NATP",
            },
        )
        .expect("natp cid should generate")
        .into_bytes();
        assert_ok!(CitizenIdentity::self_occupy_cid(
            RuntimeOrigin::signed(3),
            cid(&natp),
        ));
        assert_eq!(CidByAccountId::<Test>::get(3), Some(cid(&natp)));
        // 匿名:居民也不写投票身份。
        assert!(!VotingIdentityByCid::<Test>::contains_key(cid(&natp)));
    });
}

#[test]
fn self_occupy_cid_rejects_smtp_type() {
    new_test_ext().execute_with(|| {
        // 自助只限 CTZN/NATP:智能人 SMTP 不可自助占号(须其它流程)。
        let smtp = primitives::cid::generator::generate_cid_number(
            primitives::cid::generator::GenerateCidNumberInput {
                public_key: "smtp-self",
                p1: "1",
                province_code: "",
                province_name: "",
                city_code: "",
                city_name: "",
                year: "2026",
                institution: "SMTP",
            },
        )
        .expect("smtp cid should generate")
        .into_bytes();
        assert_noop!(
            CitizenIdentity::self_occupy_cid(RuntimeOrigin::signed(3), cid(&smtp)),
            Error::<Test>::InvalidCitizenCode
        );
    });
}

#[test]
fn self_rebind_cid_account_id_moves_binding_to_new_account_id() {
    new_test_ext().execute_with(|| {
        let cid_bytes = citizen_cid_number("rebind1");
        assert_ok!(CitizenIdentity::self_occupy_cid(
            RuntimeOrigin::signed(1),
            cid(&cid_bytes),
        ));
        let expires_at = rebind_expires_at();
        // 新账户 2 作 origin(证新账户受控)，当前账户 1 授权签名。
        assert_ok!(CitizenIdentity::self_rebind_cid_account_id(
            RuntimeOrigin::signed(2),
            cid(&cid_bytes),
            1,
            expires_at,
            rebind_signature(1, &cid_bytes, 1, 2, 1, expires_at),
        ));
        // 换绑后：CID 绑定新账户 2，此前账户 1 的反向索引清除。
        assert_eq!(AccountIdByCid::<Test>::get(cid(&cid_bytes)), Some(2));
        assert_eq!(CidByAccountId::<Test>::get(2), Some(cid(&cid_bytes)));
        assert_eq!(CidByAccountId::<Test>::get(1), None);
        assert_eq!(BindingRevisionByCid::<Test>::get(cid(&cid_bytes)), Some(2));
    });
}

#[test]
fn self_rebind_cid_account_id_rejects_unoccupied_cid() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            CitizenIdentity::self_rebind_cid_account_id(
                RuntimeOrigin::signed(2),
                cid(&citizen_cid_number("rebind_none")),
                1,
                rebind_expires_at(),
                valid_signature(),
            ),
            Error::<Test>::NotBoundToAnyCid
        );
    });
}

#[test]
fn self_rebind_cid_account_id_rejects_invalid_current_account_signature() {
    new_test_ext().execute_with(|| {
        let cid_bytes = citizen_cid_number("rebind2");
        assert_ok!(CitizenIdentity::self_occupy_cid(
            RuntimeOrigin::signed(1),
            cid(&cid_bytes),
        ));
        let bad_sig: pallet::SignatureOf<Test> = b"nope".to_vec().try_into().expect("sig fits");
        let expires_at = rebind_expires_at();
        assert_noop!(
            CitizenIdentity::self_rebind_cid_account_id(
                RuntimeOrigin::signed(2),
                cid(&cid_bytes),
                1,
                expires_at,
                bad_sig,
            ),
            Error::<Test>::InvalidRebindSignature
        );
    });
}

#[test]
fn self_rebind_cid_account_id_rejects_new_account_bound_to_another_cid() {
    new_test_ext().execute_with(|| {
        // 账户 1 占 cidA;账户 2 占 cidB;把 cidA 换绑到已绑 cidB 的账户 2 → 拒。
        let cid_a = citizen_cid_number("rebind3a");
        let cid_b = citizen_cid_number("rebind3b");
        assert_ok!(CitizenIdentity::self_occupy_cid(
            RuntimeOrigin::signed(1),
            cid(&cid_a),
        ));
        assert_ok!(CitizenIdentity::self_occupy_cid(
            RuntimeOrigin::signed(2),
            cid(&cid_b),
        ));
        let expires_at = rebind_expires_at();
        assert_noop!(
            CitizenIdentity::self_rebind_cid_account_id(
                RuntimeOrigin::signed(2),
                cid(&cid_a),
                1,
                expires_at,
                rebind_signature(1, &cid_a, 1, 2, 1, expires_at),
            ),
            Error::<Test>::AccountIdAlreadyBoundToAnotherCid
        );
    });
}

#[test]
fn self_rebind_cid_account_id_rejects_civic_cid() {
    new_test_ext().execute_with(|| {
        // civic:占号 + 注册投票身份 → 有 VotingIdentity → 自助换绑拒(q1,走注册局)。
        occupy_tag("0001");
        assert_ok!(CitizenIdentity::register_voting_identity(
            RuntimeOrigin::signed(100),
            registrar_cid_number(),
            registrar_role_code(),
            voting_payload(1, &citizen_cid_number("0001")),
            identity_version(&citizen_cid_number("0001")),
            identity_expires_at(),
            valid_signature(),
        ));
        assert_noop!(
            CitizenIdentity::self_rebind_cid_account_id(
                RuntimeOrigin::signed(2),
                cid(&citizen_cid_number("0001")),
                1,
                rebind_expires_at(),
                valid_signature(),
            ),
            Error::<Test>::CivicRebindRequiresRegistrar
        );
    });
}

#[test]
fn self_rebind_rejects_wrong_revision_expiry_same_account_and_cross_chain_signature() {
    new_test_ext().execute_with(|| {
        let cid_bytes = citizen_cid_number("rebind-guards");
        assert_ok!(CitizenIdentity::self_occupy_cid(
            RuntimeOrigin::signed(1),
            cid(&cid_bytes),
        ));
        let expires_at = rebind_expires_at();
        let signature = rebind_signature(1, &cid_bytes, 1, 2, 1, expires_at);

        assert_noop!(
            CitizenIdentity::self_rebind_cid_account_id(
                RuntimeOrigin::signed(2),
                cid(&cid_bytes),
                2,
                expires_at,
                signature.clone(),
            ),
            Error::<Test>::BindingRevisionMismatch
        );
        assert_noop!(
            CitizenIdentity::self_rebind_cid_account_id(
                RuntimeOrigin::signed(2),
                cid(&cid_bytes),
                1,
                test_time_secs(),
                signature.clone(),
            ),
            Error::<Test>::CidAuthorizationExpired
        );
        assert_noop!(
            CitizenIdentity::self_rebind_cid_account_id(
                RuntimeOrigin::signed(2),
                cid(&cid_bytes),
                1,
                test_time_secs() + MAX_CID_AUTHORIZATION_LIFETIME_SECS + 1,
                signature.clone(),
            ),
            Error::<Test>::CidAuthorizationLifetimeTooLong
        );
        assert_noop!(
            CitizenIdentity::self_rebind_cid_account_id(
                RuntimeOrigin::signed(1),
                cid(&cid_bytes),
                1,
                expires_at,
                rebind_signature(1, &cid_bytes, 1, 1, 1, expires_at),
            ),
            Error::<Test>::RebindAccountIdUnchanged
        );

        frame_system::BlockHash::<Test>::insert(0, sp_core::H256::repeat_byte(9));
        assert_noop!(
            CitizenIdentity::self_rebind_cid_account_id(
                RuntimeOrigin::signed(2),
                cid(&cid_bytes),
                1,
                expires_at,
                signature,
            ),
            Error::<Test>::InvalidRebindSignature
        );
    });
}

#[test]
fn self_rebind_rejects_a_historical_authorization_after_two_later_rotations() {
    new_test_ext().execute_with(|| {
        let cid_bytes = citizen_cid_number("rebind-replay");
        assert_ok!(CitizenIdentity::self_occupy_cid(
            RuntimeOrigin::signed(1),
            cid(&cid_bytes),
        ));
        let expires_at = rebind_expires_at();
        let historical = rebind_signature(1, &cid_bytes, 1, 2, 1, expires_at);
        assert_ok!(CitizenIdentity::self_rebind_cid_account_id(
            RuntimeOrigin::signed(2),
            cid(&cid_bytes),
            1,
            expires_at,
            historical.clone(),
        ));
        assert_ok!(CitizenIdentity::self_rebind_cid_account_id(
            RuntimeOrigin::signed(1),
            cid(&cid_bytes),
            2,
            expires_at,
            rebind_signature(2, &cid_bytes, 2, 1, 2, expires_at),
        ));
        assert_eq!(BindingRevisionByCid::<Test>::get(cid(&cid_bytes)), Some(3));

        assert_noop!(
            CitizenIdentity::self_rebind_cid_account_id(
                RuntimeOrigin::signed(2),
                cid(&cid_bytes),
                1,
                expires_at,
                historical,
            ),
            Error::<Test>::BindingRevisionMismatch
        );
    });
}

#[test]
fn admin_rebind_cid_account_id_moves_binding_to_new_account_id() {
    new_test_ext().execute_with(|| {
        // 注册局占号绑账户 1（匿名），随后由有权限注册局直接换绑到账户 2；
        // 本入口只要求注册局鉴权与新账户签名，不要求当前账户签名。
        occupy_tag_as("adm1", 1);
        let cid_bytes = citizen_cid_number("adm1");
        let expires_at = rebind_expires_at();
        assert_ok!(CitizenIdentity::admin_rebind_cid_account_id(
            RuntimeOrigin::signed(100),
            registrar_cid_number(),
            registrar_role_code(),
            cid(&cid_bytes),
            2,
            1,
            expires_at,
            rebind_signature(2, &cid_bytes, 1, 2, 1, expires_at),
        ));
        assert_eq!(AccountIdByCid::<Test>::get(cid(&cid_bytes)), Some(2));
        assert_eq!(CidByAccountId::<Test>::get(2), Some(cid(&cid_bytes)));
        assert_eq!(CidByAccountId::<Test>::get(1), None);
        assert_eq!(BindingRevisionByCid::<Test>::get(cid(&cid_bytes)), Some(2));
    });
}

#[test]
fn admin_rebind_cid_account_id_rejects_unoccupied_cid() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            CitizenIdentity::admin_rebind_cid_account_id(
                RuntimeOrigin::signed(100),
                registrar_cid_number(),
                registrar_role_code(),
                cid(&citizen_cid_number("adm_none")),
                2,
                1,
                rebind_expires_at(),
                valid_signature(),
            ),
            Error::<Test>::NotBoundToAnyCid
        );
    });
}

#[test]
fn admin_rebind_cid_account_id_rebinds_civic_cid_with_scoped_registrar() {
    new_test_ext().execute_with(|| {
        let cid_bytes = citizen_cid_number("0001");
        occupy_tag("0001");
        assert_ok!(CitizenIdentity::upgrade_to_candidate_identity(
            RuntimeOrigin::signed(100),
            registrar_cid_number(),
            registrar_role_code(),
            candidate_payload(1, &cid_bytes),
            identity_version(&cid_bytes),
            identity_expires_at(),
            valid_signature(),
        ));
        let voting_identity_before =
            VotingIdentityByCid::<Test>::get(cid(&cid_bytes)).expect("voting identity exists");
        let candidate_identity_before = CandidateIdentityByCid::<Test>::get(cid(&cid_bytes))
            .expect("candidate identity exists");
        let expires_at = rebind_expires_at();
        assert_ok!(CitizenIdentity::admin_rebind_cid_account_id(
            RuntimeOrigin::signed(100),
            registrar_cid_number(),
            registrar_role_code(),
            cid(&cid_bytes),
            2,
            1,
            expires_at,
            rebind_signature(2, &cid_bytes, 1, 2, 1, expires_at),
        ));
        assert_eq!(AccountIdByCid::<Test>::get(cid(&cid_bytes)), Some(2));
        // 投票、候选资料始终按 CID 存储；换绑只替换当前签名账户，不迁移或改写资料。
        assert_eq!(
            VotingIdentityByCid::<Test>::get(cid(&cid_bytes)),
            Some(voting_identity_before)
        );
        assert_eq!(
            CandidateIdentityByCid::<Test>::get(cid(&cid_bytes)),
            Some(candidate_identity_before)
        );
        assert!(CitizenIdentity::citizen_subject(&1).is_none());
        assert!(CitizenIdentity::voting_subject(&1, &town_scope()).is_none());
        assert!(CitizenIdentity::candidate_subject(&1, &town_scope()).is_none());
        assert_eq!(
            CitizenIdentity::citizen_subject(&2)
                .expect("实名 CID 应由新钱包继续使用")
                .cid_number,
            cid(&cid_bytes)
        );
        assert!(CitizenIdentity::voting_subject(&2, &town_scope()).is_some());
        assert!(CitizenIdentity::candidate_subject(&2, &town_scope()).is_some());
    });
}

#[test]
fn admin_rebind_cid_account_id_rejects_unauthorized_registrar() {
    new_test_ext().execute_with(|| {
        occupy_tag_as("adm3", 1);
        let cid_bytes = citizen_cid_number("adm3");
        let expires_at = rebind_expires_at();
        assert_noop!(
            CitizenIdentity::admin_rebind_cid_account_id(
                RuntimeOrigin::signed(999),
                registrar_cid_number(),
                registrar_role_code(),
                cid(&cid_bytes),
                2,
                1,
                expires_at,
                rebind_signature(2, &cid_bytes, 1, 2, 1, expires_at),
            ),
            Error::<Test>::UnauthorizedRegistrar
        );
    });
}

#[test]
fn admin_rebind_cid_account_id_rejects_invalid_new_signature() {
    new_test_ext().execute_with(|| {
        occupy_tag_as("adm2", 1);
        let bad_sig: pallet::SignatureOf<Test> = b"nope".to_vec().try_into().expect("sig fits");
        assert_noop!(
            CitizenIdentity::admin_rebind_cid_account_id(
                RuntimeOrigin::signed(100),
                registrar_cid_number(),
                registrar_role_code(),
                cid(&citizen_cid_number("adm2")),
                2,
                1,
                rebind_expires_at(),
                bad_sig,
            ),
            Error::<Test>::InvalidAdminRebindSignature
        );
    });
}

#[test]
fn admin_rebind_rejects_stale_expired_same_account_and_cross_chain_authorizations() {
    new_test_ext().execute_with(|| {
        let cid_bytes = citizen_cid_number("adm-guards");
        occupy_tag_as("adm-guards", 1);
        let expires_at = rebind_expires_at();
        let signature = rebind_signature(2, &cid_bytes, 1, 2, 1, expires_at);

        assert_noop!(
            CitizenIdentity::admin_rebind_cid_account_id(
                RuntimeOrigin::signed(100),
                registrar_cid_number(),
                registrar_role_code(),
                cid(&cid_bytes),
                2,
                2,
                expires_at,
                signature.clone(),
            ),
            Error::<Test>::BindingRevisionMismatch
        );
        assert_noop!(
            CitizenIdentity::admin_rebind_cid_account_id(
                RuntimeOrigin::signed(100),
                registrar_cid_number(),
                registrar_role_code(),
                cid(&cid_bytes),
                2,
                1,
                test_time_secs(),
                signature.clone(),
            ),
            Error::<Test>::CidAuthorizationExpired
        );
        assert_noop!(
            CitizenIdentity::admin_rebind_cid_account_id(
                RuntimeOrigin::signed(100),
                registrar_cid_number(),
                registrar_role_code(),
                cid(&cid_bytes),
                1,
                1,
                expires_at,
                rebind_signature(1, &cid_bytes, 1, 1, 1, expires_at),
            ),
            Error::<Test>::RebindAccountIdUnchanged
        );

        frame_system::BlockHash::<Test>::insert(0, sp_core::H256::repeat_byte(6));
        assert_noop!(
            CitizenIdentity::admin_rebind_cid_account_id(
                RuntimeOrigin::signed(100),
                registrar_cid_number(),
                registrar_role_code(),
                cid(&cid_bytes),
                2,
                1,
                expires_at,
                signature,
            ),
            Error::<Test>::InvalidAdminRebindSignature
        );
        assert_eq!(AccountIdByCid::<Test>::get(cid(&cid_bytes)), Some(1));
        assert_eq!(BindingRevisionByCid::<Test>::get(cid(&cid_bytes)), Some(1));
    });
}

#[test]
fn admin_rebind_cid_account_id_rejects_new_account_bound_to_another_cid() {
    new_test_ext().execute_with(|| {
        // adm4a 绑账户 1、adm4b 绑账户 2;把 adm4a 换绑到已绑 adm4b 的账户 2 → 拒。
        occupy_tag_as("adm4a", 1);
        occupy_tag_as("adm4b", 2);
        let cid_bytes = citizen_cid_number("adm4a");
        let expires_at = rebind_expires_at();
        assert_noop!(
            CitizenIdentity::admin_rebind_cid_account_id(
                RuntimeOrigin::signed(100),
                registrar_cid_number(),
                registrar_role_code(),
                cid(&cid_bytes),
                2,
                1,
                expires_at,
                rebind_signature(2, &cid_bytes, 1, 2, 1, expires_at),
            ),
            Error::<Test>::AccountIdAlreadyBoundToAnotherCid
        );
    });
}

#[test]
fn natp_resident_cannot_become_voting_citizen() {
    new_test_ext().execute_with(|| {
        // q3 补充约束:NATP 居民永远无法升级投票/竞选公民(register 走 CTZN 校验)。
        let natp = primitives::cid::generator::generate_cid_number(
            primitives::cid::generator::GenerateCidNumberInput {
                public_key: "natp-civic",
                p1: "1",
                province_code: "",
                province_name: "",
                city_code: "",
                city_name: "",
                year: "2026",
                institution: "NATP",
            },
        )
        .expect("natp cid should generate")
        .into_bytes();
        assert_ok!(CitizenIdentity::self_occupy_cid(
            RuntimeOrigin::signed(5),
            cid(&natp),
        ));
        assert_noop!(
            CitizenIdentity::register_voting_identity(
                RuntimeOrigin::signed(100),
                registrar_cid_number(),
                registrar_role_code(),
                voting_payload(5, &natp),
                identity_version(&natp),
                identity_expires_at(),
                valid_signature(),
            ),
            Error::<Test>::InvalidCitizenCode
        );
    });
}

#[test]
fn duplicate_cid_cannot_move_to_another_account_id() {
    new_test_ext().execute_with(|| {
        // 占号先行:身份写入前置。
        occupy_tag("0001");

        assert_ok!(CitizenIdentity::register_voting_identity(
            RuntimeOrigin::signed(100),
            registrar_cid_number(),
            registrar_role_code(),
            voting_payload(1, &citizen_cid_number("0001")),
            identity_version(&citizen_cid_number("0001")),
            identity_expires_at(),
            valid_signature(),
        ));

        assert_noop!(
            CitizenIdentity::register_voting_identity(
                RuntimeOrigin::signed(100),
                registrar_cid_number(),
                registrar_role_code(),
                voting_payload(2, &citizen_cid_number("0001")),
                identity_version(&citizen_cid_number("0001")),
                identity_expires_at(),
                valid_signature(),
            ),
            Error::<Test>::VotingIdentityAlreadyExists
        );
    });
}

#[test]
fn updating_identity_cannot_replace_permanent_cid() {
    new_test_ext().execute_with(|| {
        // 占号先行:身份写入前置(占即绑,两号绑不同账户)。
        occupy_tag_as("0001", 1);
        occupy_tag_as("0002", 2);

        assert_ok!(CitizenIdentity::register_voting_identity(
            RuntimeOrigin::signed(100),
            registrar_cid_number(),
            registrar_role_code(),
            voting_payload(1, &citizen_cid_number("0001")),
            identity_version(&citizen_cid_number("0001")),
            identity_expires_at(),
            valid_signature(),
        ));
        assert_noop!(
            CitizenIdentity::update_voting_identity(
                RuntimeOrigin::signed(100),
                registrar_cid_number(),
                registrar_role_code(),
                voting_payload(1, &citizen_cid_number("0002")),
                identity_version(&citizen_cid_number("0002")),
                identity_expires_at(),
                valid_signature(),
            ),
            Error::<Test>::CidAccountIdBindingMismatch
        );
        assert_eq!(
            AccountIdByCid::<Test>::get(cid(&citizen_cid_number("0001"))),
            Some(1)
        );
        // 占即绑:0002 虽未登记投票身份,占号时已绑账户 2(匿名 CID)。
        assert_eq!(
            AccountIdByCid::<Test>::get(cid(&citizen_cid_number("0002"))),
            Some(2)
        );
        assert_eq!(CountryVotingCount::<Test>::get(), 1);
        assert_eq!(
            TownVotingCount::<Test>::get((code(b"43"), code(b"4301"), code(b"4301001"))),
            1
        );
    });
}

#[test]
fn candidate_identity_requires_full_profile_and_enables_candidate_reader() {
    new_test_ext().execute_with(|| {
        // 占号先行:身份写入前置。
        occupy_tag("CANDIDATE");

        assert_ok!(CitizenIdentity::upgrade_to_candidate_identity(
            RuntimeOrigin::signed(100),
            registrar_cid_number(),
            registrar_role_code(),
            candidate_payload(1, &citizen_cid_number("CANDIDATE")),
            identity_version(&citizen_cid_number("CANDIDATE")),
            identity_expires_at(),
            valid_signature(),
        ));

        assert!(CandidateIdentityByCid::<Test>::contains_key(cid(
            &citizen_cid_number("CANDIDATE")
        )));
        assert!(CitizenIdentity::voting_subject(&1, &town_scope()).is_some());
        assert!(CitizenIdentity::candidate_subject(&1, &town_scope()).is_some());
    });
}

#[test]
fn citizen_subject_requires_active_bidirectional_cid_account_id_binding() {
    new_test_ext().execute_with(|| {
        occupy_tag("SUBJECT");
        let cid_number = citizen_cid_number("SUBJECT");
        assert_ok!(CitizenIdentity::register_voting_identity(
            RuntimeOrigin::signed(100),
            registrar_cid_number(),
            registrar_role_code(),
            voting_payload(1, &cid_number),
            identity_version(&cid_number),
            identity_expires_at(),
            valid_signature(),
        ));

        assert_eq!(
            CitizenIdentity::citizen_subject(&1),
            Some(CitizenSubject {
                cid_number: cid(&cid_number),
                account_id: 1,
            })
        );

        // 反向绑定与账户存储键不一致时 fail-closed，不能只凭裸账户形成主体。
        AccountIdByCid::<Test>::insert(cid(&cid_number), 2);
        assert_eq!(CitizenIdentity::citizen_subject(&1), None);
        assert!(CitizenIdentity::voting_subject(&1, &town_scope()).is_none());
    });
}

#[test]
fn citizen_subject_rejects_revoked_identity_and_cid() {
    new_test_ext().execute_with(|| {
        occupy_tag("SUBJECT-REVOKED");
        let cid_number = citizen_cid_number("SUBJECT-REVOKED");
        assert_ok!(CitizenIdentity::register_voting_identity(
            RuntimeOrigin::signed(100),
            registrar_cid_number(),
            registrar_role_code(),
            voting_payload(1, &cid_number),
            identity_version(&cid_number),
            identity_expires_at(),
            valid_signature(),
        ));
        assert_ok!(CitizenIdentity::revoke_cid(
            RuntimeOrigin::signed(100),
            registrar_cid_number(),
            registrar_role_code(),
            cid(&cid_number),
        ));

        assert_eq!(CitizenIdentity::citizen_subject(&1), None);
    });
}

#[test]
fn candidate_identity_requires_family_name_and_given_name_separately() {
    new_test_ext().execute_with(|| {
        occupy_tag_as("EMPTY-FAMILY", 1);
        let mut empty_family = candidate_payload(1, &citizen_cid_number("EMPTY-FAMILY"));
        empty_family.family_name = Default::default();
        assert_noop!(
            CitizenIdentity::upgrade_to_candidate_identity(
                RuntimeOrigin::signed(100),
                registrar_cid_number(),
                registrar_role_code(),
                empty_family.clone(),
                candidate_version(&empty_family),
                identity_expires_at(),
                valid_signature(),
            ),
            Error::<Test>::EmptyFamilyName
        );

        occupy_tag_as("EMPTY-GIVEN", 2);
        let mut empty_given = candidate_payload(2, &citizen_cid_number("EMPTY-GIVEN"));
        empty_given.given_name = Default::default();
        assert_noop!(
            CitizenIdentity::upgrade_to_candidate_identity(
                RuntimeOrigin::signed(100),
                registrar_cid_number(),
                registrar_role_code(),
                empty_given.clone(),
                candidate_version(&empty_given),
                identity_expires_at(),
                valid_signature(),
            ),
            Error::<Test>::EmptyGivenName
        );
    });
}

#[test]
fn revoke_identity_marks_status_and_removes_effective_population() {
    new_test_ext().execute_with(|| {
        // 占号先行:身份写入前置。
        occupy_tag("REVOKE");

        assert_ok!(CitizenIdentity::upgrade_to_candidate_identity(
            RuntimeOrigin::signed(100),
            registrar_cid_number(),
            registrar_role_code(),
            candidate_payload(1, &citizen_cid_number("REVOKE")),
            identity_version(&citizen_cid_number("REVOKE")),
            identity_expires_at(),
            valid_signature(),
        ));
        assert_ok!(CitizenIdentity::revoke_identity(
            RuntimeOrigin::signed(100),
            registrar_cid_number(),
            registrar_role_code(),
            cid(&citizen_cid_number("REVOKE")),
        ));

        let stored = VotingIdentityByCid::<Test>::get(cid(&citizen_cid_number("REVOKE")))
            .expect("identity should remain");
        assert_eq!(stored.citizen_status, CitizenStatus::Revoked);
        assert!(!CandidateIdentityByCid::<Test>::contains_key(cid(
            &citizen_cid_number("REVOKE")
        )));
        assert_eq!(
            BindingRevisionByCid::<Test>::get(cid(&citizen_cid_number("REVOKE"))),
            Some(2)
        );
        assert_eq!(CountryVotingCount::<Test>::get(), 0);
        assert!(CitizenIdentity::voting_subject(&1, &town_scope()).is_none());

        let cid_number = cid(&citizen_cid_number("REVOKE"));
        let identity_before = VotingIdentityByCid::<Test>::get(&cid_number);
        let record_before = CidRegistry::<Test>::get(&cid_number);
        let revision_before = BindingRevisionByCid::<Test>::get(&cid_number);
        assert_noop!(
            CitizenIdentity::revoke_identity(
                RuntimeOrigin::signed(100),
                registrar_cid_number(),
                registrar_role_code(),
                cid_number.clone(),
            ),
            Error::<Test>::CidAlreadyRevoked
        );
        assert_eq!(
            VotingIdentityByCid::<Test>::get(&cid_number),
            identity_before
        );
        assert_eq!(CidRegistry::<Test>::get(&cid_number), record_before);
        assert_eq!(
            BindingRevisionByCid::<Test>::get(&cid_number),
            revision_before
        );
        assert_eq!(CountryVotingCount::<Test>::get(), 0);
    });
}

#[test]
fn revoke_identity_revision_preflight_failures_leave_state_unchanged() {
    new_test_ext().execute_with(|| {
        let cid_bytes = citizen_cid_number("REVOKE-REV");
        occupy_tag("REVOKE-REV");
        assert_ok!(CitizenIdentity::register_voting_identity(
            RuntimeOrigin::signed(100),
            registrar_cid_number(),
            registrar_role_code(),
            voting_payload(1, &cid_bytes),
            identity_version(&cid_bytes),
            identity_expires_at(),
            valid_signature(),
        ));
        let cid_number = cid(&cid_bytes);
        let identity_before = VotingIdentityByCid::<Test>::get(&cid_number);
        let record_before = CidRegistry::<Test>::get(&cid_number);
        let population_before = CountryVotingCount::<Test>::get();

        BindingRevisionByCid::<Test>::remove(&cid_number);
        assert_noop!(
            CitizenIdentity::revoke_identity(
                RuntimeOrigin::signed(100),
                registrar_cid_number(),
                registrar_role_code(),
                cid_number.clone(),
            ),
            Error::<Test>::BindingRevisionMissing
        );
        assert_eq!(
            VotingIdentityByCid::<Test>::get(&cid_number),
            identity_before
        );
        assert_eq!(CidRegistry::<Test>::get(&cid_number), record_before);
        assert_eq!(CountryVotingCount::<Test>::get(), population_before);

        BindingRevisionByCid::<Test>::insert(&cid_number, u64::MAX);
        assert_noop!(
            CitizenIdentity::revoke_identity(
                RuntimeOrigin::signed(100),
                registrar_cid_number(),
                registrar_role_code(),
                cid_number.clone(),
            ),
            Error::<Test>::BindingRevisionOverflow
        );
        assert_eq!(
            VotingIdentityByCid::<Test>::get(&cid_number),
            identity_before
        );
        assert_eq!(CidRegistry::<Test>::get(&cid_number), record_before);
        assert_eq!(CountryVotingCount::<Test>::get(), population_before);
        assert_eq!(
            BindingRevisionByCid::<Test>::get(&cid_number),
            Some(u64::MAX)
        );
    });
}

#[test]
fn population_data_reads_current_scope_count() {
    new_test_ext().execute_with(|| {
        // 占号先行:身份写入前置。
        occupy_tag("0001");

        assert_ok!(CitizenIdentity::register_voting_identity(
            RuntimeOrigin::signed(100),
            registrar_cid_number(),
            registrar_role_code(),
            voting_payload(1, &citizen_cid_number("0001")),
            identity_version(&citizen_cid_number("0001")),
            identity_expires_at(),
            valid_signature(),
        ));

        let population_data = CitizenIdentity::governance_population_data(&town_scope())
            .expect("current population should be ready");
        assert_eq!(population_data.eligible_total, 1);
        assert_eq!(population_data.eligibility_revision, 1);
        assert_eq!(population_data.eligibility_date, 20260702);
        let voter_subject =
            CitizenIdentity::voting_subject_at_population_data(&1, &population_data)
                .expect("snapshot eligibility should return the complete citizen subject");
        assert_eq!(voter_subject.cid_number, citizen_cid_number("0001"));
        assert_eq!(voter_subject.account_id, 1);
    });
}

#[test]
fn population_data_revision_freezes_membership_before_identity_update() {
    new_test_ext().execute_with(|| {
        occupy_tag("SNAPSHOT-OLD");
        assert_ok!(CitizenIdentity::register_voting_identity(
            RuntimeOrigin::signed(100),
            registrar_cid_number(),
            registrar_role_code(),
            voting_payload(1, &citizen_cid_number("SNAPSHOT-OLD")),
            identity_version(&citizen_cid_number("SNAPSHOT-OLD")),
            identity_expires_at(),
            valid_signature(),
        ));

        let old_population_data = CitizenIdentity::governance_population_data(&town_scope())
            .expect("old population should be ready");
        assert_eq!(old_population_data.eligible_total, 1);
        assert!(
            CitizenIdentity::voting_subject_at_population_data(&1, &old_population_data).is_some()
        );

        // 同一账户迁往另一乡镇后，旧提案仍按创建时身份判断；新提案使用新身份。
        let mut moved = voting_payload(1, &citizen_cid_number("SNAPSHOT-OLD"));
        moved.residence_town_code = code(b"4301002");
        assert_ok!(CitizenIdentity::update_voting_identity(
            RuntimeOrigin::signed(100),
            registrar_cid_number(),
            registrar_role_code(),
            moved.clone(),
            voting_version(&moved),
            identity_expires_at(),
            valid_signature(),
        ));

        assert!(
            CitizenIdentity::voting_subject_at_population_data(&1, &old_population_data).is_some()
        );
        let new_population_data = CitizenIdentity::governance_population_data(&town_scope())
            .expect("new population should be ready");
        assert_eq!(new_population_data.eligible_total, 0);
        assert!(
            CitizenIdentity::voting_subject_at_population_data(&1, &new_population_data).is_none()
        );
    });
}

#[test]
fn invalid_citizen_code_is_rejected() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            CitizenIdentity::register_voting_identity(
                RuntimeOrigin::signed(100),
                registrar_cid_number(),
                registrar_role_code(),
                voting_payload(1, b"OLD-0001"),
                identity_version(b"OLD-0001"),
                identity_expires_at(),
                valid_signature(),
            ),
            Error::<Test>::InvalidCitizenCode
        );
    });
}

#[test]
fn expired_passport_cannot_vote_and_is_excluded_from_population() {
    new_test_ext().execute_with(|| {
        // 占号先行:身份写入前置。
        occupy_tag("EXPIRED");

        let mut payload = voting_payload(1, &citizen_cid_number("EXPIRED"));
        payload.passport_valid_from = 20200101;
        payload.passport_valid_until = 20250101;

        assert_ok!(CitizenIdentity::register_voting_identity(
            RuntimeOrigin::signed(100),
            registrar_cid_number(),
            registrar_role_code(),
            payload.clone(),
            voting_version(&payload),
            identity_expires_at(),
            valid_signature(),
        ));

        assert_eq!(CountryVotingCount::<Test>::get(), 0);
        assert!(CitizenIdentity::voting_subject(&1, &town_scope()).is_none());
        assert!(CitizenIdentity::candidate_subject(&1, &town_scope()).is_none());
    });
}

#[test]
fn not_yet_valid_passport_cannot_vote() {
    new_test_ext().execute_with(|| {
        // 占号先行:身份写入前置。
        occupy_tag("FUTURE");

        let mut payload = voting_payload(1, &citizen_cid_number("FUTURE"));
        payload.passport_valid_from = 20300101;
        payload.passport_valid_until = 20400101;

        assert_ok!(CitizenIdentity::register_voting_identity(
            RuntimeOrigin::signed(100),
            registrar_cid_number(),
            registrar_role_code(),
            payload.clone(),
            voting_version(&payload),
            identity_expires_at(),
            valid_signature(),
        ));

        assert_eq!(CountryVotingCount::<Test>::get(), 0);
        assert_eq!(PopulationTransitionCountByDate::<Test>::get(20300101), 1);
        assert!(CitizenIdentity::voting_subject(&1, &town_scope()).is_none());
    });
}

#[test]
fn first_population_date_must_initialize_before_identity_write() {
    new_test_ext().execute_with(|| {
        PopulationReadyDate::<Test>::kill();
        occupy_tag("BOOTSTRAP");
        assert_noop!(
            CitizenIdentity::register_voting_identity(
                RuntimeOrigin::signed(100),
                registrar_cid_number(),
                registrar_role_code(),
                voting_payload(1, &citizen_cid_number("BOOTSTRAP")),
                identity_version(&citizen_cid_number("BOOTSTRAP")),
                identity_expires_at(),
                valid_signature(),
            ),
            Error::<Test>::PopulationDataNotReady
        );

        CitizenIdentity::on_idle(System::block_number(), frame_support::weights::Weight::MAX);
        assert_eq!(PopulationReadyDate::<Test>::get(), 20260702);
        assert_ok!(CitizenIdentity::register_voting_identity(
            RuntimeOrigin::signed(100),
            registrar_cid_number(),
            registrar_role_code(),
            voting_payload(1, &citizen_cid_number("BOOTSTRAP")),
            identity_version(&citizen_cid_number("BOOTSTRAP")),
            identity_expires_at(),
            valid_signature(),
        ));
    });
}

#[test]
fn passport_activates_on_valid_from_and_deactivates_after_valid_until() {
    new_test_ext().execute_with(|| {
        occupy_tag("ONE-DAY");
        let mut payload = voting_payload(1, &citizen_cid_number("ONE-DAY"));
        payload.passport_valid_from = 20260703;
        payload.passport_valid_until = 20260703;
        assert_ok!(CitizenIdentity::register_voting_identity(
            RuntimeOrigin::signed(100),
            registrar_cid_number(),
            registrar_role_code(),
            payload.clone(),
            voting_version(&payload),
            identity_expires_at(),
            valid_signature(),
        ));
        assert_eq!(CountryVotingCount::<Test>::get(), 0);

        set_day_offset(1);
        assert!(CitizenIdentity::governance_population_data(&PopulationScope::Country).is_none());
        CitizenIdentity::on_idle(System::block_number(), frame_support::weights::Weight::MAX);
        assert_eq!(PopulationReadyDate::<Test>::get(), 20260703);
        assert_eq!(CountryVotingCount::<Test>::get(), 1);
        assert_eq!(
            CitizenIdentity::governance_population_data(&PopulationScope::Country)
                .expect("activation date should be ready")
                .eligibility_date,
            20260703
        );

        set_day_offset(2);
        CitizenIdentity::on_idle(System::block_number(), frame_support::weights::Weight::MAX);
        assert_eq!(PopulationReadyDate::<Test>::get(), 20260704);
        assert_eq!(CountryVotingCount::<Test>::get(), 0);
    });
}

#[test]
fn population_transition_limit_hides_partial_day_and_blocks_identity_changes() {
    new_test_ext().execute_with(|| {
        for (account_id, tag) in [(1, "BATCH-1"), (2, "BATCH-2"), (3, "BATCH-3")] {
            occupy_tag_as(tag, account_id);
            let mut payload = voting_payload(account_id, &citizen_cid_number(tag));
            payload.passport_valid_from = 20260703;
            payload.passport_valid_until = 20300101;
            assert_ok!(CitizenIdentity::register_voting_identity(
                RuntimeOrigin::signed(100),
                registrar_cid_number(),
                registrar_role_code(),
                payload.clone(),
                voting_version(&payload),
                identity_expires_at(),
                valid_signature(),
            ));
        }

        set_day_offset(1);
        CitizenIdentity::on_idle(System::block_number(), frame_support::weights::Weight::MAX);
        assert_eq!(PopulationReadyDate::<Test>::get(), 20260702);
        assert_eq!(PopulationTransitionCursorByDate::<Test>::get(20260703), 2);
        assert_eq!(CountryVotingCount::<Test>::get(), 2);
        assert!(CitizenIdentity::governance_population_data(&PopulationScope::Country).is_none());

        let mut update = voting_payload(1, &citizen_cid_number("BATCH-1"));
        update.passport_valid_from = 20260703;
        update.passport_valid_until = 20310101;
        assert_noop!(
            CitizenIdentity::update_voting_identity(
                RuntimeOrigin::signed(100),
                registrar_cid_number(),
                registrar_role_code(),
                update.clone(),
                voting_version(&update),
                identity_expires_at(),
                valid_signature(),
            ),
            Error::<Test>::PopulationDataNotReady
        );

        CitizenIdentity::on_idle(System::block_number(), frame_support::weights::Weight::MAX);
        assert_eq!(PopulationReadyDate::<Test>::get(), 20260703);
        assert_eq!(CountryVotingCount::<Test>::get(), 3);
        assert!(CitizenIdentity::governance_population_data(&PopulationScope::Country).is_some());
    });
}

#[test]
fn identity_update_invalidates_old_population_transitions_by_revision() {
    new_test_ext().execute_with(|| {
        occupy_tag("RESCHEDULE");
        let mut first = voting_payload(1, &citizen_cid_number("RESCHEDULE"));
        first.passport_valid_from = 20260703;
        first.passport_valid_until = 20260703;
        assert_ok!(CitizenIdentity::register_voting_identity(
            RuntimeOrigin::signed(100),
            registrar_cid_number(),
            registrar_role_code(),
            first.clone(),
            voting_version(&first),
            identity_expires_at(),
            valid_signature(),
        ));

        let mut replacement = voting_payload(1, &citizen_cid_number("RESCHEDULE"));
        replacement.passport_valid_from = 20260704;
        replacement.passport_valid_until = 20260705;
        assert_ok!(CitizenIdentity::update_voting_identity(
            RuntimeOrigin::signed(100),
            registrar_cid_number(),
            registrar_role_code(),
            replacement.clone(),
            voting_version(&replacement),
            identity_expires_at(),
            valid_signature(),
        ));

        set_day_offset(1);
        CitizenIdentity::on_idle(System::block_number(), frame_support::weights::Weight::MAX);
        assert_eq!(CountryVotingCount::<Test>::get(), 0);
        set_day_offset(2);
        CitizenIdentity::on_idle(System::block_number(), frame_support::weights::Weight::MAX);
        assert_eq!(PopulationReadyDate::<Test>::get(), 20260704);
        assert_eq!(CountryVotingCount::<Test>::get(), 1);
    });
}

#[test]
fn strict_calendar_validation_handles_months_leap_years_and_year_boundary() {
    new_test_ext().execute_with(|| {
        assert!(CitizenIdentity::is_plausible_yyyymmdd(20240229));
        assert!(!CitizenIdentity::is_plausible_yyyymmdd(20230229));
        assert!(!CitizenIdentity::is_plausible_yyyymmdd(20260431));
        assert_eq!(
            CitizenIdentity::next_calendar_date(20240229),
            Some(20240301)
        );
        assert_eq!(
            CitizenIdentity::next_calendar_date(20261231),
            Some(20270101)
        );
        assert_eq!(CitizenIdentity::next_calendar_date(99991231), None);
    });
}

#[test]
fn population_faults_closed_when_chain_date_moves_backwards() {
    new_test_ext().execute_with(|| {
        set_day_offset(-1);
        CitizenIdentity::on_idle(System::block_number(), frame_support::weights::Weight::MAX);
        assert_eq!(
            PopulationMaintenanceFault::<Test>::get(),
            Some(PopulationFault::DateMovedBackwards)
        );
        assert!(CitizenIdentity::governance_population_data(&PopulationScope::Country).is_none());
    });
}

#[test]
fn candidate_identity_stores_sex_and_public_profile() {
    new_test_ext().execute_with(|| {
        // 占号先行:身份写入前置。
        occupy_tag("SEX");

        assert_ok!(CitizenIdentity::upgrade_to_candidate_identity(
            RuntimeOrigin::signed(100),
            registrar_cid_number(),
            registrar_role_code(),
            candidate_payload(1, &citizen_cid_number("SEX")),
            identity_version(&citizen_cid_number("SEX")),
            identity_expires_at(),
            valid_signature(),
        ));

        let stored = CandidateIdentityByCid::<Test>::get(cid(&citizen_cid_number("SEX")))
            .expect("candidate stored");
        assert_eq!(stored.citizen_sex, CitizenSex::Female);
        assert_eq!(stored.family_name, family_name(b"Citizen"));
        assert_eq!(stored.given_name, given_name(b"One"));
        assert_eq!(stored.birth_date, 20000131);
        // 固定链上日 20260702 − 出生 20000131 → 26 周岁。
        assert_eq!(CitizenIdentity::candidate_age(&1), Some(26));
    });
}

#[test]
fn current_date_int_matches_fixed_time() {
    new_test_ext().execute_with(|| {
        // FixedTime = 2026-07-02 00:00 UTC → UTC+8 折算 20260702。
        assert_eq!(CitizenIdentity::current_date_int(), 20260702);
    });
}

#[test]
fn age_from_birth_date_handles_birthday_boundary() {
    new_test_ext().execute_with(|| {
        // 固定链上日 20260702。
        assert_eq!(CitizenIdentity::age_from_birth_date(20000701), Some(26)); // 生日已过
        assert_eq!(CitizenIdentity::age_from_birth_date(20000702), Some(26)); // 今日生日
        assert_eq!(CitizenIdentity::age_from_birth_date(20000703), Some(25)); // 生日未到
        assert_eq!(CitizenIdentity::age_from_birth_date(0), None); // 空
        assert_eq!(CitizenIdentity::age_from_birth_date(20300101), None); // 未来出生
    });
}

#[test]
fn candidate_birth_date_is_immutable_on_update() {
    new_test_ext().execute_with(|| {
        occupy_tag("IMMUT");
        let cid = citizen_cid_number("IMMUT");
        assert_ok!(CitizenIdentity::upgrade_to_candidate_identity(
            RuntimeOrigin::signed(100),
            registrar_cid_number(),
            registrar_role_code(),
            candidate_payload(1, &cid),
            identity_version(&cid),
            identity_expires_at(),
            valid_signature(),
        ));

        // 更新竞选身份时试图改出生日期 → 拒绝。
        let mut tampered = candidate_payload(1, &cid);
        tampered.birth_date = 19990101;
        assert_noop!(
            CitizenIdentity::update_candidate_identity(
                RuntimeOrigin::signed(100),
                registrar_cid_number(),
                registrar_role_code(),
                tampered.clone(),
                candidate_version(&tampered),
                identity_expires_at(),
                valid_signature(),
            ),
            Error::<Test>::BirthDateImmutable
        );
    });
}

#[test]
fn candidate_birth_scope_and_sex_are_immutable_on_update() {
    new_test_ext().execute_with(|| {
        occupy_tag("IMMUT-PROFILE");
        let cid = citizen_cid_number("IMMUT-PROFILE");
        assert_ok!(CitizenIdentity::upgrade_to_candidate_identity(
            RuntimeOrigin::signed(100),
            registrar_cid_number(),
            registrar_role_code(),
            candidate_payload(1, &cid),
            identity_version(&cid),
            identity_expires_at(),
            valid_signature(),
        ));

        // 更新竞选身份时试图改性别 → 拒绝(D4a:填后锁定)。
        let mut tampered_sex = candidate_payload(1, &cid);
        tampered_sex.citizen_sex = CitizenSex::Male;
        assert_noop!(
            CitizenIdentity::update_candidate_identity(
                RuntimeOrigin::signed(100),
                registrar_cid_number(),
                registrar_role_code(),
                tampered_sex.clone(),
                candidate_version(&tampered_sex),
                identity_expires_at(),
                valid_signature(),
            ),
            Error::<Test>::CandidateProfileImmutable
        );

        // 试图改出生市码 → 同样拒绝。
        let mut tampered_city = candidate_payload(1, &cid);
        tampered_city.birth_city_code = code(b"4302");
        assert_noop!(
            CitizenIdentity::update_candidate_identity(
                RuntimeOrigin::signed(100),
                registrar_cid_number(),
                registrar_role_code(),
                tampered_city.clone(),
                candidate_version(&tampered_city),
                identity_expires_at(),
                valid_signature(),
            ),
            Error::<Test>::CandidateProfileImmutable
        );
    });
}

#[test]
fn candidate_illegal_birth_date_rejected() {
    new_test_ext().execute_with(|| {
        occupy_tag("BADDOB");
        let mut payload = candidate_payload(1, &citizen_cid_number("BADDOB"));
        payload.birth_date = 20261340; // 非法月/日
        assert_noop!(
            CitizenIdentity::upgrade_to_candidate_identity(
                RuntimeOrigin::signed(100),
                registrar_cid_number(),
                registrar_role_code(),
                payload.clone(),
                candidate_version(&payload),
                identity_expires_at(),
                valid_signature(),
            ),
            Error::<Test>::InvalidBirthDate
        );
    });
}

#[test]
fn candidate_future_birth_date_rejected() {
    new_test_ext().execute_with(|| {
        occupy_tag("FUTDOB");
        let mut payload = candidate_payload(1, &citizen_cid_number("FUTDOB"));
        payload.birth_date = 20990101; // 未来出生 → 算不出年龄
        assert_noop!(
            CitizenIdentity::upgrade_to_candidate_identity(
                RuntimeOrigin::signed(100),
                registrar_cid_number(),
                registrar_role_code(),
                payload.clone(),
                candidate_version(&payload),
                identity_expires_at(),
                valid_signature(),
            ),
            Error::<Test>::InvalidBirthDate
        );
    });
}

#[test]
fn candidate_under_sixteen_cannot_register_onchain_identity() {
    new_test_ext().execute_with(|| {
        // 占号先行:身份写入前置。
        occupy_tag("UNDERAGE");

        // 投票身份不再链上校验年龄;最小年龄门只在竞选身份按 birth_date 实时判定。
        // 固定时间 20260702 下出生 20150301 → 11 周岁,未满 16。
        let mut payload = candidate_payload(1, &citizen_cid_number("UNDERAGE"));
        payload.birth_date = 20150301;

        assert_noop!(
            CitizenIdentity::upgrade_to_candidate_identity(
                RuntimeOrigin::signed(100),
                registrar_cid_number(),
                registrar_role_code(),
                payload.clone(),
                candidate_version(&payload),
                identity_expires_at(),
                valid_signature(),
            ),
            Error::<Test>::UnderCandidateAge
        );
    });
}

#[test]
fn non_citizen_family_code_is_rejected() {
    new_test_ext().execute_with(|| {
        // 真实格式的公权机构号(CGOV)打到公民入口必须被家族断言拒绝。
        let institution_number = primitives::cid::generator::generate_cid_number(
            primitives::cid::generator::GenerateCidNumberInput {
                public_key: "gov",
                p1: "0",
                province_code: "GD",
                province_name: "广东省",
                city_code: "001",
                city_name: "荔湾市",
                year: "2026",
                institution: "CGOV",
            },
        )
        .expect("institution cid should generate")
        .into_bytes();

        assert_noop!(
            CitizenIdentity::register_voting_identity(
                RuntimeOrigin::signed(100),
                registrar_cid_number(),
                registrar_role_code(),
                voting_payload(1, &institution_number),
                identity_version(&institution_number),
                identity_expires_at(),
                valid_signature(),
            ),
            Error::<Test>::InvalidCitizenCode
        );
    });
}

#[test]
fn occupy_cid_binds_account_and_stays_anonymous() {
    use codec::Encode;
    new_test_ext().execute_with(|| {
        let cid_bytes = citizen_cid_number("OCC-BIND");
        let expires_at = rebind_expires_at();
        assert_ok!(CitizenIdentity::occupy_cid(
            RuntimeOrigin::signed(100),
            registrar_cid_number(),
            registrar_role_code(),
            cid(&cid_bytes),
            7,
            expires_at,
            occupy_signature(7, &cid_bytes, expires_at),
        ));
        // 占即绑:双向绑定成立。
        assert_eq!(AccountIdByCid::<Test>::get(cid(&cid_bytes)), Some(7));
        assert_eq!(CidByAccountId::<Test>::get(7), Some(cid(&cid_bytes)));
        assert_eq!(BindingRevisionByCid::<Test>::get(cid(&cid_bytes)), Some(1));
        // 登记记录:注册局 registrar + 空居住地(全国号)+ Active + commitment=blake2_256(账户)。
        let rec = CidRegistry::<Test>::get(cid(&cid_bytes)).expect("record");
        assert_eq!(rec.registrar_cid_number, registrar_cid_number());
        assert!(rec.residence_province_code.is_empty());
        assert!(rec.residence_city_code.is_empty());
        assert_eq!(rec.status, CidRecordStatus::Active);
        assert_eq!(rec.commitment, sp_io::hashing::blake2_256(&7u64.encode()));
        // 匿名:无投票身份。
        assert!(!VotingIdentityByCid::<Test>::contains_key(cid(&cid_bytes)));
    });
}

#[test]
fn occupy_cid_accepts_resident_natp_type() {
    new_test_ext().execute_with(|| {
        let natp = primitives::cid::generator::generate_cid_number(
            primitives::cid::generator::GenerateCidNumberInput {
                public_key: "occ-natp",
                p1: "1",
                province_code: "",
                province_name: "",
                city_code: "",
                city_name: "",
                year: "2026",
                institution: "NATP",
            },
        )
        .expect("natp cid should generate")
        .into_bytes();
        let expires_at = rebind_expires_at();
        assert_ok!(CitizenIdentity::occupy_cid(
            RuntimeOrigin::signed(100),
            registrar_cid_number(),
            registrar_role_code(),
            cid(&natp),
            8,
            expires_at,
            occupy_signature(8, &natp, expires_at),
        ));
        assert_eq!(CidByAccountId::<Test>::get(8), Some(cid(&natp)));
    });
}

#[test]
fn occupy_cid_rejects_replay_after_first_binding() {
    new_test_ext().execute_with(|| {
        occupy_tag_as("OCC-1", 1);
        let cid_bytes = citizen_cid_number("OCC-1");
        let expires_at = rebind_expires_at();
        // 首次绑定证明公开后不得再次提交，哪怕注册局、CID 与账户完全相同。
        assert_noop!(
            CitizenIdentity::occupy_cid(
                RuntimeOrigin::signed(100),
                registrar_cid_number(),
                registrar_role_code(),
                cid(&cid_bytes),
                1,
                expires_at,
                occupy_signature(1, &cid_bytes, expires_at),
            ),
            Error::<Test>::CidAlreadyOccupied
        );
        // 另一账户抢占同 CID 同样由“只允许 revision=0 首次绑定”拒绝。
        assert_noop!(
            CitizenIdentity::occupy_cid(
                RuntimeOrigin::signed(100),
                registrar_cid_number(),
                registrar_role_code(),
                cid(&cid_bytes),
                2,
                expires_at,
                occupy_signature(2, &cid_bytes, expires_at),
            ),
            Error::<Test>::CidAlreadyOccupied
        );
    });
}

#[test]
fn occupy_cid_rejects_invalid_signature() {
    new_test_ext().execute_with(|| {
        let cid_bytes = citizen_cid_number("BAD-SIG");
        assert_noop!(
            CitizenIdentity::occupy_cid(
                RuntimeOrigin::signed(100),
                registrar_cid_number(),
                registrar_role_code(),
                cid(&cid_bytes),
                1,
                rebind_expires_at(),
                b"nope".to_vec().try_into().expect("signature should fit"),
            ),
            Error::<Test>::InvalidOccupySignature
        );
    });
}

#[test]
fn occupy_cid_rejects_expired_overlong_and_cross_chain_authorizations() {
    new_test_ext().execute_with(|| {
        let cid_bytes = citizen_cid_number("OCC-GUARDS");
        assert_noop!(
            CitizenIdentity::occupy_cid(
                RuntimeOrigin::signed(100),
                registrar_cid_number(),
                registrar_role_code(),
                cid(&cid_bytes),
                1,
                test_time_secs(),
                occupy_signature(1, &cid_bytes, test_time_secs()),
            ),
            Error::<Test>::CidAuthorizationExpired
        );
        let overlong = test_time_secs() + MAX_CID_AUTHORIZATION_LIFETIME_SECS + 1;
        assert_noop!(
            CitizenIdentity::occupy_cid(
                RuntimeOrigin::signed(100),
                registrar_cid_number(),
                registrar_role_code(),
                cid(&cid_bytes),
                1,
                overlong,
                occupy_signature(1, &cid_bytes, overlong),
            ),
            Error::<Test>::CidAuthorizationLifetimeTooLong
        );

        let expires_at = rebind_expires_at();
        let signature = occupy_signature(1, &cid_bytes, expires_at);
        frame_system::BlockHash::<Test>::insert(0, sp_core::H256::repeat_byte(7));
        assert_noop!(
            CitizenIdentity::occupy_cid(
                RuntimeOrigin::signed(100),
                registrar_cid_number(),
                registrar_role_code(),
                cid(&cid_bytes),
                1,
                expires_at,
                signature,
            ),
            Error::<Test>::InvalidOccupySignature
        );
    });
}

#[test]
fn occupy_cid_rejects_unauthorized_registrar_and_bad_number() {
    new_test_ext().execute_with(|| {
        // 非在册注册局账户:匿名鉴权拒。
        assert_noop!(
            CitizenIdentity::occupy_cid(
                RuntimeOrigin::signed(999),
                registrar_cid_number(),
                registrar_role_code(),
                cid(&citizen_cid_number("OCC-2")),
                1,
                rebind_expires_at(),
                valid_signature(),
            ),
            Error::<Test>::UnauthorizedRegistrar
        );
        // 公权机构号打公民占号入口:类型断言拒(CTZN|NATP 之外)。
        assert_noop!(
            CitizenIdentity::occupy_cid(
                RuntimeOrigin::signed(100),
                registrar_cid_number(),
                registrar_role_code(),
                cid(&public_cid_number("OCC-2")),
                1,
                rebind_expires_at(),
                valid_signature(),
            ),
            Error::<Test>::InvalidCitizenCode
        );
    });
}

#[test]
fn register_without_occupation_is_rejected() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            CitizenIdentity::register_voting_identity(
                RuntimeOrigin::signed(100),
                registrar_cid_number(),
                registrar_role_code(),
                voting_payload(1, &citizen_cid_number("NO-OCC")),
                identity_version(&citizen_cid_number("NO-OCC")),
                identity_expires_at(),
                valid_signature(),
            ),
            Error::<Test>::CidNotOccupied
        );
    });
}

#[test]
fn revoke_cid_tombstones_and_revokes_bound_identity() {
    new_test_ext().execute_with(|| {
        occupy_tag("RV-1");
        assert_ok!(CitizenIdentity::register_voting_identity(
            RuntimeOrigin::signed(100),
            registrar_cid_number(),
            registrar_role_code(),
            voting_payload(1, &citizen_cid_number("RV-1")),
            identity_version(&citizen_cid_number("RV-1")),
            identity_expires_at(),
            valid_signature(),
        ));
        assert_eq!(CountryVotingCount::<Test>::get(), 1);

        assert_ok!(CitizenIdentity::revoke_cid(
            RuntimeOrigin::signed(100),
            registrar_cid_number(),
            registrar_role_code(),
            cid(&citizen_cid_number("RV-1")),
        ));
        // 登记表墓碑 + 身份联动吊销 + 退出人口分母。
        let rec = CidRegistry::<Test>::get(cid(&citizen_cid_number("RV-1"))).expect("record kept");
        assert_eq!(rec.status, CidRecordStatus::Revoked);
        assert_eq!(
            BindingRevisionByCid::<Test>::get(cid(&citizen_cid_number("RV-1"))),
            Some(2)
        );
        assert_eq!(
            VotingIdentityByCid::<Test>::get(cid(&citizen_cid_number("RV-1")))
                .expect("identity kept")
                .citizen_status,
            CitizenStatus::Revoked
        );
        assert_eq!(CountryVotingCount::<Test>::get(), 0);

        // 再吊销:已墓碑。
        assert_noop!(
            CitizenIdentity::revoke_cid(
                RuntimeOrigin::signed(100),
                registrar_cid_number(),
                registrar_role_code(),
                cid(&citizen_cid_number("RV-1")),
            ),
            Error::<Test>::CidAlreadyRevoked
        );
        // 墓碑号原绑定账户也不可再占(号码永不复用;绑定闭环放行、do_occupy 见墓碑拒)。
        assert_noop!(
            CitizenIdentity::occupy_cid(
                RuntimeOrigin::signed(100),
                registrar_cid_number(),
                registrar_role_code(),
                cid(&citizen_cid_number("RV-1")),
                1,
                rebind_expires_at(),
                valid_signature(),
            ),
            Error::<Test>::CidAlreadyOccupied
        );
        // 墓碑号也不能再注册身份：永久 CID 身份与账户绑定均保留，
        // 归属检查先于墓碑检查拦截(双保险,谁先触发都拒绝)。
        assert_noop!(
            CitizenIdentity::register_voting_identity(
                RuntimeOrigin::signed(100),
                registrar_cid_number(),
                registrar_role_code(),
                voting_payload(2, &citizen_cid_number("RV-1")),
                identity_version(&citizen_cid_number("RV-1")),
                identity_expires_at(),
                valid_signature(),
            ),
            Error::<Test>::CidAlreadyRevoked
        );
    });
}

#[test]
fn revoke_cid_revision_preflight_failures_leave_state_unchanged() {
    new_test_ext().execute_with(|| {
        let cid_bytes = citizen_cid_number("RV-REV");
        occupy_tag("RV-REV");
        assert_ok!(CitizenIdentity::register_voting_identity(
            RuntimeOrigin::signed(100),
            registrar_cid_number(),
            registrar_role_code(),
            voting_payload(1, &cid_bytes),
            identity_version(&cid_bytes),
            identity_expires_at(),
            valid_signature(),
        ));
        let cid_number = cid(&cid_bytes);
        let identity_before = VotingIdentityByCid::<Test>::get(&cid_number);
        let record_before = CidRegistry::<Test>::get(&cid_number);
        let population_before = CountryVotingCount::<Test>::get();

        BindingRevisionByCid::<Test>::remove(&cid_number);
        assert_noop!(
            CitizenIdentity::revoke_cid(
                RuntimeOrigin::signed(100),
                registrar_cid_number(),
                registrar_role_code(),
                cid_number.clone(),
            ),
            Error::<Test>::BindingRevisionMissing
        );
        assert_eq!(
            VotingIdentityByCid::<Test>::get(&cid_number),
            identity_before
        );
        assert_eq!(CidRegistry::<Test>::get(&cid_number), record_before);
        assert_eq!(CountryVotingCount::<Test>::get(), population_before);

        BindingRevisionByCid::<Test>::insert(&cid_number, u64::MAX);
        assert_noop!(
            CitizenIdentity::revoke_cid(
                RuntimeOrigin::signed(100),
                registrar_cid_number(),
                registrar_role_code(),
                cid_number.clone(),
            ),
            Error::<Test>::BindingRevisionOverflow
        );
        assert_eq!(
            VotingIdentityByCid::<Test>::get(&cid_number),
            identity_before
        );
        assert_eq!(CidRegistry::<Test>::get(&cid_number), record_before);
        assert_eq!(CountryVotingCount::<Test>::get(), population_before);
        assert_eq!(
            BindingRevisionByCid::<Test>::get(&cid_number),
            Some(u64::MAX)
        );
    });
}

#[test]
fn revoked_cid_rejects_self_and_admin_rebind_without_state_changes() {
    new_test_ext().execute_with(|| {
        let cid_bytes = citizen_cid_number("RV-REBIND");
        occupy_tag_as("RV-REBIND", 1);
        let expires_at = rebind_expires_at();
        let previous_self_authorization = rebind_signature(1, &cid_bytes, 1, 2, 1, expires_at);
        let previous_admin_authorization = rebind_signature(2, &cid_bytes, 1, 2, 1, expires_at);

        assert_ok!(CitizenIdentity::revoke_cid(
            RuntimeOrigin::signed(100),
            registrar_cid_number(),
            registrar_role_code(),
            cid(&cid_bytes),
        ));
        let cid_number = cid(&cid_bytes);
        let record_before = CidRegistry::<Test>::get(&cid_number);
        assert_eq!(BindingRevisionByCid::<Test>::get(&cid_number), Some(2));

        assert_noop!(
            CitizenIdentity::self_rebind_cid_account_id(
                RuntimeOrigin::signed(2),
                cid_number.clone(),
                1,
                expires_at,
                previous_self_authorization,
            ),
            Error::<Test>::CidAlreadyRevoked
        );
        assert_noop!(
            CitizenIdentity::admin_rebind_cid_account_id(
                RuntimeOrigin::signed(100),
                registrar_cid_number(),
                registrar_role_code(),
                cid_number.clone(),
                2,
                1,
                expires_at,
                previous_admin_authorization,
            ),
            Error::<Test>::CidAlreadyRevoked
        );

        assert_eq!(CidRegistry::<Test>::get(&cid_number), record_before);
        assert_eq!(AccountIdByCid::<Test>::get(&cid_number), Some(1));
        assert_eq!(CidByAccountId::<Test>::get(1), Some(cid_number.clone()));
        assert_eq!(CidByAccountId::<Test>::get(2), None);
        assert_eq!(BindingRevisionByCid::<Test>::get(&cid_number), Some(2));
    });
}

#[test]
fn permanent_cid_update_keeps_registry_record_active() {
    new_test_ext().execute_with(|| {
        occupy_tag_as("CHG-A", 1);
        occupy_tag_as("CHG-B", 2);
        assert_ok!(CitizenIdentity::register_voting_identity(
            RuntimeOrigin::signed(100),
            registrar_cid_number(),
            registrar_role_code(),
            voting_payload(1, &citizen_cid_number("CHG-A")),
            identity_version(&citizen_cid_number("CHG-A")),
            identity_expires_at(),
            valid_signature(),
        ));
        let mut updated = voting_payload(1, &citizen_cid_number("CHG-A"));
        updated.residence_town_code = code(b"4301002");
        assert_ok!(CitizenIdentity::update_voting_identity(
            RuntimeOrigin::signed(100),
            registrar_cid_number(),
            registrar_role_code(),
            updated.clone(),
            voting_version(&updated),
            identity_expires_at(),
            valid_signature(),
        ));
        assert_eq!(
            CidRegistry::<Test>::get(cid(&citizen_cid_number("CHG-A")))
                .expect("permanent record kept")
                .status,
            CidRecordStatus::Active
        );
        assert_eq!(
            CidRegistry::<Test>::get(cid(&citizen_cid_number("CHG-B")))
                .expect("unrelated occupied record kept")
                .status,
            CidRecordStatus::Active
        );
    });
}

// ─── CidCount 有效 CID 计数 ─────────────────────────────────────────

/// 创世登记条数直接就是初始计数：挖矿页读到的第一个数字必须等于创世公民数。
#[test]
fn cid_count_starts_at_genesis_binding_len() {
    let registrar = registrar_cid_number();
    new_test_ext_with_cid_bindings(vec![
        (
            cid(&citizen_cid_number("count-genesis-a")),
            41,
            registrar.clone(),
        ),
        (cid(&citizen_cid_number("count-genesis-b")), 42, registrar),
    ])
    .execute_with(|| {
        assert_eq!(CidCount::<Test>::get(), 2);
    });
}

/// 无创世登记时计数为 0，不是 `ValueQuery` 默认值巧合，而是没有任何 Active 记录。
#[test]
fn cid_count_is_zero_without_any_occupied_cid() {
    new_test_ext().execute_with(|| {
        assert_eq!(CidCount::<Test>::get(), 0);
        assert_eq!(CidRegistry::<Test>::iter().count(), 0);
    });
}

/// 自助占号每成功一个 +1。
#[test]
fn cid_count_increments_on_self_occupy() {
    new_test_ext().execute_with(|| {
        assert_eq!(CidCount::<Test>::get(), 0);
        assert_ok!(CitizenIdentity::self_occupy_cid(
            RuntimeOrigin::signed(1),
            cid(&citizen_cid_number("count-self-1")),
        ));
        assert_eq!(CidCount::<Test>::get(), 1);
        assert_ok!(CitizenIdentity::self_occupy_cid(
            RuntimeOrigin::signed(2),
            cid(&citizen_cid_number("count-self-2")),
        ));
        assert_eq!(CidCount::<Test>::get(), 2);
    });
}

/// 注册局占即绑同样 +1，与自助口径一致。
#[test]
fn cid_count_increments_on_registrar_occupy() {
    new_test_ext().execute_with(|| {
        occupy_tag_as("count-reg-1", 1);
        assert_eq!(CidCount::<Test>::get(), 1);
        occupy_tag_as("count-reg-2", 2);
        assert_eq!(CidCount::<Test>::get(), 2);
    });
}

/// 幂等重入不得加 1：同账户同 CID 重复提交只是恢复路径，链上没有新登记记录。
/// 少了这条守卫，用户建档失败重试一次就把计数虚增一个，且永远无法回落。
#[test]
fn cid_count_stays_flat_on_idempotent_reoccupy() {
    new_test_ext().execute_with(|| {
        let cid_bytes = citizen_cid_number("count-idem");
        assert_ok!(CitizenIdentity::self_occupy_cid(
            RuntimeOrigin::signed(1),
            cid(&cid_bytes),
        ));
        assert_eq!(CidCount::<Test>::get(), 1);
        // 同账户同 CID 重放：do_occupy_cid 走幂等分支，不写库也不计数。
        assert_ok!(CitizenIdentity::self_occupy_cid(
            RuntimeOrigin::signed(1),
            cid(&cid_bytes),
        ));
        assert_eq!(CidCount::<Test>::get(), 1);
        assert_eq!(CidRegistry::<Test>::iter().count(), 1);
    });
}

/// 注册局吊销 −1，且墓碑仍留在登记表里：计数只跟 Active，不跟记录条数。
#[test]
fn cid_count_decrements_on_revoke_cid_while_tombstone_remains() {
    new_test_ext().execute_with(|| {
        occupy_tag_as("count-revoke", 1);
        assert_eq!(CidCount::<Test>::get(), 1);

        assert_ok!(CitizenIdentity::revoke_cid(
            RuntimeOrigin::signed(100),
            registrar_cid_number(),
            registrar_role_code(),
            cid(&citizen_cid_number("count-revoke")),
        ));
        assert_eq!(CidCount::<Test>::get(), 0);
        // 墓碑不删除：登记表仍有 1 条记录，但它已不是有效 CID。
        assert_eq!(CidRegistry::<Test>::iter().count(), 1);
        assert_eq!(
            CidRegistry::<Test>::get(cid(&citizen_cid_number("count-revoke")))
                .expect("tombstone kept")
                .status,
            CidRecordStatus::Revoked
        );
    });
}

/// 身份吊销联动路径（`revoke_identity`）同样 −1：两条吊销入口共用 tombstone helper，
/// 任何一条漏减都会让计数永久偏高。
#[test]
fn cid_count_decrements_on_revoke_identity() {
    new_test_ext().execute_with(|| {
        occupy_tag_as("count-revoke-id", 1);
        assert_ok!(CitizenIdentity::register_voting_identity(
            RuntimeOrigin::signed(100),
            registrar_cid_number(),
            registrar_role_code(),
            voting_payload(1, &citizen_cid_number("count-revoke-id")),
            identity_version(&citizen_cid_number("count-revoke-id")),
            identity_expires_at(),
            valid_signature(),
        ));
        assert_eq!(CidCount::<Test>::get(), 1);

        assert_ok!(CitizenIdentity::revoke_identity(
            RuntimeOrigin::signed(100),
            registrar_cid_number(),
            registrar_role_code(),
            cid(&citizen_cid_number("count-revoke-id")),
        ));
        assert_eq!(CidCount::<Test>::get(), 0);
    });
}

// ───────────────── 身份写入防重放：版本 / 时间窗 ─────────────────

/// 历史载荷重放被拒：迁居换照后，旧签名不得把居住地与护照窗口回滚。
#[test]
fn replaying_an_old_voting_payload_is_rejected_by_identity_version() {
    new_test_ext().execute_with(|| {
        occupy_tag("REPLAY-OLD");
        let cid_bytes = citizen_cid_number("REPLAY-OLD");
        let original = voting_payload(1, &cid_bytes);
        let original_version = identity_version(&cid_bytes);
        assert_ok!(CitizenIdentity::register_voting_identity(
            RuntimeOrigin::signed(100),
            registrar_cid_number(),
            registrar_role_code(),
            original.clone(),
            original_version,
            identity_expires_at(),
            valid_signature(),
        ));

        // 公民迁往另一乡镇并换发护照，有效期延长。
        let mut moved = voting_payload(1, &cid_bytes);
        moved.residence_town_code = code(b"4301002");
        moved.passport_valid_until = 20400630;
        assert_ok!(CitizenIdentity::update_voting_identity(
            RuntimeOrigin::signed(100),
            registrar_cid_number(),
            registrar_role_code(),
            moved,
            identity_version(&cid_bytes),
            identity_expires_at(),
            valid_signature(),
        ));

        // 原载荷 + 原版本号原样重提：版本已推进，拒绝。
        assert_noop!(
            CitizenIdentity::update_voting_identity(
                RuntimeOrigin::signed(100),
                registrar_cid_number(),
                registrar_role_code(),
                original,
                original_version,
                identity_expires_at(),
                valid_signature(),
            ),
            Error::<Test>::IdentityVersionMismatch
        );

        // 链上仍是迁居后的状态，未被回滚。
        let current = VotingIdentityByCid::<Test>::get(cid(&cid_bytes)).expect("identity");
        assert_eq!(current.residence_town_code.as_slice(), b"4301002");
        assert_eq!(current.passport_valid_until, 20400630);
    });
}

/// 竞选身份写入同样受版本保护：旧版本号提交即拒。
#[test]
fn replaying_an_old_candidate_payload_is_rejected_by_identity_version() {
    new_test_ext().execute_with(|| {
        occupy_tag("REPLAY-CAND");
        let cid_bytes = citizen_cid_number("REPLAY-CAND");
        let payload = candidate_payload(1, &cid_bytes);
        let stale_version = identity_version(&cid_bytes);
        assert_ok!(CitizenIdentity::upgrade_to_candidate_identity(
            RuntimeOrigin::signed(100),
            registrar_cid_number(),
            registrar_role_code(),
            payload.clone(),
            stale_version,
            identity_expires_at(),
            valid_signature(),
        ));

        assert_noop!(
            CitizenIdentity::update_candidate_identity(
                RuntimeOrigin::signed(100),
                registrar_cid_number(),
                registrar_role_code(),
                payload,
                stale_version,
                identity_expires_at(),
                valid_signature(),
            ),
            Error::<Test>::IdentityVersionMismatch
        );
    });
}

/// 版本必须精确等于链上当前值，超前同样被拒。
#[test]
fn future_identity_version_is_rejected() {
    new_test_ext().execute_with(|| {
        occupy_tag("VER-AHEAD");
        let cid_bytes = citizen_cid_number("VER-AHEAD");
        assert_noop!(
            CitizenIdentity::register_voting_identity(
                RuntimeOrigin::signed(100),
                registrar_cid_number(),
                registrar_role_code(),
                voting_payload(1, &cid_bytes),
                identity_version(&cid_bytes) + 1,
                identity_expires_at(),
                valid_signature(),
            ),
            Error::<Test>::IdentityVersionMismatch
        );
    });
}

/// 过期授权被拒。
#[test]
fn expired_identity_authorization_is_rejected() {
    new_test_ext().execute_with(|| {
        occupy_tag("IDENT-EXPIRED");
        let cid_bytes = citizen_cid_number("IDENT-EXPIRED");
        assert_noop!(
            CitizenIdentity::register_voting_identity(
                RuntimeOrigin::signed(100),
                registrar_cid_number(),
                registrar_role_code(),
                voting_payload(1, &cid_bytes),
                identity_version(&cid_bytes),
                test_time_secs(),
                valid_signature(),
            ),
            Error::<Test>::CidAuthorizationExpired
        );
    });
}

/// 授权有效期超过 600 秒上限被拒。
#[test]
fn overlong_identity_authorization_lifetime_is_rejected() {
    new_test_ext().execute_with(|| {
        occupy_tag("IDENT-LONG");
        let cid_bytes = citizen_cid_number("IDENT-LONG");
        assert_noop!(
            CitizenIdentity::register_voting_identity(
                RuntimeOrigin::signed(100),
                registrar_cid_number(),
                registrar_role_code(),
                voting_payload(1, &cid_bytes),
                identity_version(&cid_bytes),
                identity_expires_at() + 1,
                valid_signature(),
            ),
            Error::<Test>::CidAuthorizationLifetimeTooLong
        );
    });
}

/// 授权载荷 SCALE 字段序：创世哈希、载荷、版本、过期时间；四端按此构造待签字节。
#[test]
fn citizen_identity_authorization_scale_contract_is_stable() {
    new_test_ext().execute_with(|| {
        let payload = voting_payload(1, &citizen_cid_number("SCALE-ORDER"));
        let authorization = CitizenIdentityAuthorization {
            genesis_hash: frame_system::Pallet::<Test>::block_hash(0u64),
            payload: payload.clone(),
            expected_identity_version: 7,
            expires_at: 1_700_000_000,
        };

        let mut expected = frame_system::Pallet::<Test>::block_hash(0u64).encode();
        expected.extend_from_slice(&payload.encode());
        expected.extend_from_slice(&7u64.encode());
        expected.extend_from_slice(&1_700_000_000u64.encode());
        assert_eq!(authorization.encode(), expected);
    });
}
