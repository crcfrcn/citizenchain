#![cfg(test)]

//! 立法院模块第1步单测的 mock runtime。
//!
//! legislation-yuan 业务壳通过 `votingengine::Config` 复用投票引擎核心,
//! 通过自身 `Config::LegislationVoteEngine` 接立法投票引擎(第1步装 `()`)。
//! mock 里:System + VotingEngine + InternalVote(供引擎 finalizer)+ LegislationYuan,
//! LegislationVoteEngine 装 `()`,InternalAdminProvider 用 TestInternalAdminProvider。

use super::*;
use crate::pallet::{Article, Chapter, ChaptersOf, Houses, LawProposalSummary, Section};
use frame_support::{
    derive_impl, parameter_types,
    traits::{ConstU32, ConstU64},
    BoundedVec,
};
use frame_system as system;
use primitives::cid::code::InstitutionCode;
use sp_runtime::{traits::IdentityLookup, AccountId32, BuildStorage};

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

    #[runtime::pallet_index(3)]
    pub type Timestamp = pallet_timestamp;

    #[runtime::pallet_index(99)]
    pub type InternalVote = internal_vote;

    #[runtime::pallet_index(2)]
    pub type LegislationYuan = super;
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl system::Config for Test {
    type Block = Block;
    type AccountId = AccountId32;
    type Lookup = IdentityLookup<Self::AccountId>;
}

impl pallet_timestamp::Config for Test {
    type Moment = u64;
    type OnTimestampSet = ();
    type MinimumPeriod = ConstU64<1>;
    type WeightInfo = ();
}

// ───────── 测试身份常量 ─────────
/// 现任议员/委员（机构身份由 actor CID 表达）。
pub fn legislator() -> AccountId32 {
    AccountId32::new([1u8; 32])
}
/// 非议员账户。
pub fn outsider() -> AccountId32 {
    AccountId32::new([2u8; 32])
}
/// 测试默认使用国家众议会作为发起机构。
pub const OWNER_CODE: InstitutionCode = *b"NRP\0";

pub struct TestCitizenIdentityReader;
impl votingengine::CitizenIdentityReader<AccountId32> for TestCitizenIdentityReader {
    fn voting_subject(
        _who: &AccountId32,
        _scope: &votingengine::PopulationScope,
    ) -> Option<votingengine::CitizenSubject<AccountId32>> {
        None
    }

    fn candidate_subject(
        _who: &AccountId32,
        _scope: &votingengine::PopulationScope,
    ) -> Option<votingengine::CitizenSubject<AccountId32>> {
        None
    }
}

pub struct TestInternalAdminProvider;
pub struct TestInstitutionRoleProvider;
pub struct TestInstitutionRoleAuthorization;

const PROPOSER_ROLE: &[u8] = b"LAW_PROPOSER";
const VOTER_ROLE: &[u8] = b"LAW_VOTER";

/// legislator() 是测试发起机构 CID 的唯一管理员；其它一律不是。
impl votingengine::InternalAdminProvider<AccountId32> for TestInternalAdminProvider {
    fn is_institution_admin(
        institution_code: InstitutionCode,
        cid_number: &[u8],
        who: &AccountId32,
    ) -> bool {
        matches!(institution_code, code if code == *b"NRP\0" || code == *b"CSLF" || code == *b"CEDU")
            && TestInstitutionCidQuery::cid_matches(institution_code, cid_number)
            && *who == legislator()
    }
}

impl votingengine::InstitutionRoleProvider<AccountId32> for TestInstitutionRoleProvider {
    fn is_active_assignment(cid_number: &[u8], who: &AccountId32, role_code: &[u8]) -> bool {
        *who == legislator()
            && role_code == PROPOSER_ROLE
            && [*b"NRP\0", *b"CSLF", *b"CEDU", *b"CLEG"]
                .into_iter()
                .any(|code| TestInstitutionCidQuery::cid_matches(code, cid_number))
    }

    fn active_accounts_for_role(_cid_number: &[u8], _role_code: &[u8]) -> Vec<AccountId32> {
        vec![legislator()]
    }
}

impl entity_primitives::InstitutionRoleAuthorizationQuery<AccountId32>
    for TestInstitutionRoleAuthorization
{
    fn role_has_permission(
        role_subject: &entity_primitives::RoleSubject<Vec<u8>, Vec<u8>>,
        action: &entity_primitives::BusinessActionId<Vec<u8>>,
        operation: entity_primitives::RolePermissionOperation,
    ) -> bool {
        action.module_tag == MODULE_TAG
            && matches!(action.action_code, 0..=2)
            && operation == entity_primitives::RolePermissionOperation::Vote
            && (role_subject.role_code == VOTER_ROLE
                || role_subject.role_code
                    == primitives::institution_constraints::ROLE_CODE_LEGAL_REPRESENTATIVE
                || role_subject.role_code
                    == primitives::governance_skeleton::ROLE_CODE_CONSTITUTION_GUARD)
    }

    fn is_authorized(
        admin: &AccountId32,
        role_subject: &entity_primitives::RoleSubject<Vec<u8>, Vec<u8>>,
        action: &entity_primitives::BusinessActionId<Vec<u8>>,
        operation: entity_primitives::RolePermissionOperation,
    ) -> bool {
        *admin == legislator()
            && role_subject.role_code == PROPOSER_ROLE
            && action.module_tag == MODULE_TAG
            && matches!(action.action_code, 0..=2)
            && operation == entity_primitives::RolePermissionOperation::Propose
    }

    fn role_subjects_with_permission(
        cid_number: &[u8],
        action: &entity_primitives::BusinessActionId<Vec<u8>>,
        operation: entity_primitives::RolePermissionOperation,
    ) -> Vec<entity_primitives::RoleSubject<Vec<u8>, Vec<u8>>> {
        if action.module_tag != MODULE_TAG
            || !matches!(action.action_code, 0..=2)
            || operation != entity_primitives::RolePermissionOperation::Vote
        {
            return Vec::new();
        }
        let is_house = [*b"NRP\0", *b"NSN\0", *b"CLEG"]
            .into_iter()
            .any(|code| TestInstitutionCidQuery::cid_matches(code, cid_number));
        if !is_house {
            return Vec::new();
        }
        vec![
            entity_primitives::RoleSubject {
                cid_number: cid_number.to_vec(),
                role_code: VOTER_ROLE.to_vec(),
            },
            entity_primitives::RoleSubject {
                cid_number: cid_number.to_vec(),
                role_code: primitives::institution_constraints::ROLE_CODE_LEGAL_REPRESENTATIVE
                    .to_vec(),
            },
        ]
    }
}

/// 立法路由机构 CID 查询夹具；市级机构统一落在 GD002，确保同市校验能真实覆盖。
pub struct TestInstitutionCidQuery;

impl TestInstitutionCidQuery {
    fn cid(code: InstitutionCode) -> Vec<u8> {
        let code_text =
            primitives::cid::code::institution_code_text(&code).expect("test institution code");
        let regional =
            matches!(code, c if c == *b"CSLF" || c == *b"CLEG" || c == *b"CGOV" || c == *b"CEDU");
        primitives::cid::generator::generate_cid_number(
            primitives::cid::generator::GenerateCidNumberInput {
                public_key: "0x1234",
                p1: "0",
                province_code: if regional { "GD" } else { "ZS" },
                province_name: if regional { "广东省" } else { "中枢省" },
                city_code: if regional { "002" } else { "001" },
                city_name: "测试市",
                year: "2026",
                institution: code_text,
            },
        )
        .expect("test cid")
        .into_bytes()
    }

    fn bounded_cid(code: InstitutionCode) -> votingengine::types::CidNumber {
        Self::cid(code).try_into().expect("test CID should fit")
    }

    fn cid_matches(code: InstitutionCode, cid_number: &[u8]) -> bool {
        Self::cid(code).as_slice() == cid_number
    }
}

impl entity_primitives::InstitutionCidQuery<votingengine::types::CidNumber>
    for TestInstitutionCidQuery
{
    fn cid_exists(cid_number: &votingengine::types::CidNumber) -> bool {
        [
            *b"NRP\0", *b"NSN\0", *b"PRS\0", *b"NLG\0", *b"CSLF", *b"CLEG", *b"CGOV", *b"CEDU",
        ]
        .into_iter()
        .any(|code| Self::cid_matches(code, cid_number.as_slice()))
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
    type InternalAdminProvider = TestInternalAdminProvider;
    type MaxAdminsPerInstitution = ConstU32<32>;
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

parameter_types! {
    pub const MaxTitleLen: u32 = 256;
    pub const MaxTextLen: u32 = 8192;
    pub const MaxClausesPerArticle: u32 = 50;
    pub const MaxArticlesPerSection: u32 = 200;
    pub const MaxSectionsPerChapter: u32 = 50;
    pub const MaxChaptersPerLaw: u32 = 50;
    pub const MaxLawsPerScope: u32 = 1000;
    pub const MaxPendingActivations: u32 = 100;
}

impl crate::pallet::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    // 立法投票引擎单测装配为 ()(NotConfigured);端到端见 legislation-vote 测试。
    type LegislationVoteEngine = ();
    type InstitutionCidQuery = TestInstitutionCidQuery;
    type InstitutionRoleAuthorization = TestInstitutionRoleAuthorization;
    type MaxTitleLen = MaxTitleLen;
    type MaxTextLen = MaxTextLen;
    type MaxClausesPerArticle = MaxClausesPerArticle;
    type MaxArticlesPerSection = MaxArticlesPerSection;
    type MaxSectionsPerChapter = MaxSectionsPerChapter;
    type MaxChaptersPerLaw = MaxChaptersPerLaw;
    type MaxLawsPerScope = MaxLawsPerScope;
    type MaxPendingActivations = MaxPendingActivations;
    type WeightInfo = ();
}

pub fn new_test_ext() -> sp_io::TestExternalities {
    let storage = frame_system::GenesisConfig::<Test>::default()
        .build_storage()
        .expect("test storage should build");
    let mut ext = sp_io::TestExternalities::new(storage);
    ext.execute_with(|| {
        System::set_block_number(1);
        Timestamp::set_timestamp(1_000);
    });
    ext
}

// ───────── 测试数据构造 helper(章>节>条>款)─────────
/// 构造一条无款条文(body = 正文)。
pub fn article(number: u32, body: &[u8]) -> Article<Test> {
    Article::<Test> {
        number,
        title: BoundedVec::try_from(format!("第{number}条").into_bytes())
            .expect("title within bound"),
        title_en: None,
        body: BoundedVec::try_from(body.to_vec()).expect("body within bound"),
        body_en: None,
        clauses: BoundedVec::default(),
    }
}

/// 构造指定节号及其条文，供宪法三层编号唯一性测试复用。
pub fn section(number: u32, articles: Vec<Article<Test>>) -> Section<Test> {
    Section::<Test> {
        number,
        title: title(format!("第{number}节").as_bytes()),
        title_en: None,
        articles: BoundedVec::try_from(articles).expect("articles within bound"),
    }
}

/// 构造指定章号及其各节，供宪法三层编号唯一性测试复用。
pub fn chapter(number: u32, sections: Vec<Section<Test>>) -> Chapter<Test> {
    Chapter::<Test> {
        number,
        title: title(format!("第{number}章").as_bytes()),
        title_en: None,
        sections: BoundedVec::try_from(sections).expect("sections within bound"),
    }
}

/// 构造任意章结构的有界宪法全文。
pub fn structured_chapters(chapters: Vec<Chapter<Test>>) -> ChaptersOf<Test> {
    BoundedVec::try_from(chapters).expect("chapters within bound")
}

/// 把若干条包成「1 章 1 节」的法律全文(测试用)。
pub fn chapters_of(articles: Vec<Article<Test>>) -> ChaptersOf<Test> {
    structured_chapters(vec![chapter(1, vec![section(1, articles)])])
}

/// 构造两章宪法:核心章(第一章总则)+ 一般章(第二章),供第十九条章→档位测试。
/// 核心章(`chapters[0]`)条款改动要求特别案,一般章条款改动要求重要案。
pub fn chapters_core_general(
    core_articles: Vec<Article<Test>>,
    general_articles: Vec<Article<Test>>,
) -> ChaptersOf<Test> {
    let make_chapter = |number: u32, ch_title: &str, articles: Vec<Article<Test>>| Chapter::<Test> {
        number,
        title: title(ch_title.as_bytes()),
        title_en: None,
        sections: BoundedVec::try_from(vec![Section::<Test> {
            number: 1,
            title: title("第一节".as_bytes()),
            title_en: None,
            articles: BoundedVec::try_from(articles).expect("articles within bound"),
        }])
        .expect("sections within bound"),
    };
    BoundedVec::try_from(vec![
        make_chapter(1, "第一章", core_articles),
        make_chapter(2, "第二章", general_articles),
    ])
    .expect("chapters within bound")
}

pub fn title(s: &[u8]) -> BoundedVec<u8, MaxTitleLen> {
    BoundedVec::try_from(s.to_vec()).expect("title within bound")
}

/// 国家两院序列：国家众议会 → 国家参议会。
pub fn houses() -> Houses {
    BoundedVec::try_from(vec![
        TestInstitutionCidQuery::bounded_cid(OWNER_CODE),
        TestInstitutionCidQuery::bounded_cid(*b"NSN\0"),
    ])
    .expect("houses within bound")
}

// 国家行政签署机构(法定代表人=签署人)。
pub const EXEC_CODE: InstitutionCode = *b"PRS\0";
/// 提案机构 CID；legislator() 是其管理员。
pub fn actor_cid_number() -> votingengine::types::CidNumber {
    TestInstitutionCidQuery::bounded_cid(OWNER_CODE)
}
pub fn proposer_role_code() -> votingengine::types::RoleCode {
    PROPOSER_ROLE.to_vec().try_into().expect("test role fits")
}
/// 行政签署机构 CID。
pub fn executive_cid_number() -> votingengine::types::CidNumber {
    TestInstitutionCidQuery::bounded_cid(EXEC_CODE)
}

pub fn legislature_cid_number() -> Option<votingengine::types::CidNumber> {
    Some(TestInstitutionCidQuery::bounded_cid(*b"NLG\0"))
}

pub fn municipal_houses() -> Houses {
    BoundedVec::try_from(vec![TestInstitutionCidQuery::bounded_cid(*b"CLEG")])
        .expect("municipal houses within bound")
}

pub fn municipal_actor_cid_number() -> votingengine::types::CidNumber {
    TestInstitutionCidQuery::bounded_cid(*b"CSLF")
}

pub fn municipal_education_actor_cid_number() -> votingengine::types::CidNumber {
    TestInstitutionCidQuery::bounded_cid(*b"CEDU")
}

pub fn municipal_executive_cid_number() -> votingengine::types::CidNumber {
    TestInstitutionCidQuery::bounded_cid(*b"CGOV")
}

/// 直接构造一个 Enact 提案摘要(用于直调 write_law_version 预置法律)。
pub fn enact_summary(
    tier: Tier,
    scope_code: u32,
    vote_type: VoteType,
    title_bytes: &[u8],
) -> LawProposalSummary<Test> {
    let (houses, actor_cid_number, executive_cid_number, legislature_cid_number) = match tier {
        Tier::Municipal => (
            municipal_houses(),
            municipal_actor_cid_number(),
            municipal_executive_cid_number(),
            None,
        ),
        _ => (
            houses(),
            actor_cid_number(),
            executive_cid_number(),
            legislature_cid_number(),
        ),
    };
    LawProposalSummary::<Test> {
        action: LawAction::Enact,
        law_id: 0,
        tier,
        scope_code,
        houses,
        actor_cid_number,
        proposer_role_code: proposer_role_code(),
        executive_cid_number,
        legislature_cid_number,
        vote_type,
        title: title(title_bytes),
        title_en: None,
        content_hash: [0u8; 32],
        effective_at: 0,
    }
}

pub use crate::pallet::{
    Law, LawVersion, LawVersions, Laws, LawsByScope, NextLawId, PendingActivations,
};
pub type Lib = crate::pallet::Pallet<Test>;

mod cases;
