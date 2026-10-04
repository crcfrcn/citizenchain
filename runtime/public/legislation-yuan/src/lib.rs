//! # 立法院模块 (legislation-yuan)
//!
//! 法律结构化上链 + 修法一律走投票引擎(ADR-027)。本 pallet 是「业务壳」:
//! 只承载法律数据(Law / LawVersion)、状态机、提案入口(立法/修法/废法)、
//! 投票通过回调写入、不可修改条款硬拒与查询;表决规则、计票、两院顺序、强制公投
//! 全部归属投票引擎 `legislation-vote` sub-pallet。
//!
//! 解耦:`Config::LegislationVoteEngine` 注入立法投票引擎(runtime 装配为 `LegislationVote`);
//! 业务壳通过它创建立法投票提案,投票终态经核心 `LegislationVoteResultCallback` 回调写回本壳。

#![cfg_attr(not(feature = "std"), no_std)]

pub mod types;
pub mod weights;

pub use pallet::*;
pub use types::{LawAction, LawStatus, Tier, VoteType};

/// 模块标识前缀,用于在 votingengine `ProposalData` 中区分本模块提案,防止跨模块误解码。
pub const MODULE_TAG: &[u8] = b"leg-yuan";

/// 法律全文大对象类型标记(写入 votingengine `ProposalObject`)。
pub const PROPOSAL_OBJECT_KIND_LAW_TEXT: u8 = 2;

/// 内置公民宪法(章>节>条>款)SCALE 创世种子。
/// 宪法运行态唯一真源 = 本模块链上法律(创世注入为 law_id=0);旧 HTML 真源和解析脚本已删除。
pub const CONSTITUTION_SCALE: &[u8] = include_bytes!("constitution.scale");

/// 国家立法院机构码(立法权最高机构,宪法 houses[0])。
pub const NATIONAL_LEGISLATURE_CODE: primitives::cid::code::InstitutionCode = *b"NLG\0";

/// 不可修改条款 manifest 的最大容量(清单现 8 条,留余量)。
pub const MAX_IMMUTABLE_ARTICLES: u32 = 32;

#[frame_support::pallet]
pub mod pallet {
    // FRAME 宏在本模块层生成 Call 分发代码,其参数数由 extrinsic 载荷决定;
    // per-fn 与 call 块级 allow 都够不到那段生成代码,故放宽范围收敛到本 pallet 模块,
    // 不扩到整个 crate —— 模块外的辅助函数参数过多仍会被 lint 抓出。
    #![allow(clippy::too_many_arguments)]
    use super::*;
    use crate::weights::WeightInfo;
    use entity_primitives::{
        BusinessActionId, InstitutionCidQuery, InstitutionRoleAuthorizationQuery,
        RolePermissionOperation, RoleSubject,
    };
    use frame_support::pallet_prelude::*;
    use frame_support::traits::Time;
    use frame_system::pallet_prelude::*;
    use legislation_vote::{
        types::RepresentativeBody, LegislationProcedureConfig, LegislationVoteEngine,
        RepresentativeBodies, RepresentativeRoute,
    };
    use primitives::cid::china::{china_lf::CHINA_LF, china_sf::CHINA_SF};
    use primitives::cid::code::InstitutionCode;
    use primitives::constitution::{self, AmendmentScope, CONSTITUTION_CORE_CHAPTER_INDEX};
    use primitives::count_const::IMMUTABLE_CONSTITUTION_ARTICLES;
    use primitives::genesis::GENESIS_LAW_VERSION_LABELS;
    use sp_runtime::sp_std::{collections::btree_set::BTreeSet, vec::Vec};
    use sp_runtime::DispatchError;
    use votingengine::{ProposalExecutionOutcome, VotePlanOf, VotingEngineKind};

    // 受 Config 常量约束的有界字符串别名。
    pub type TitleOf<T> = BoundedVec<u8, <T as Config>::MaxTitleLen>;
    pub type TextOf<T> = BoundedVec<u8, <T as Config>::MaxTextLen>;

    /// 院序列只保存机构 CID；机构码由 CID 解析，账户不得充当立法机构身份。
    /// 单院(市立法会)= 1 项;两院(国家/省立法院)= `[众议会, 参议会]`;教委会模式 = `[教委会, 参议会]`。
    pub type Houses = BoundedVec<
        votingengine::types::CidNumber,
        ConstU32<{ legislation_vote::MAX_REPRESENTATIVE_BODIES }>,
    >;

    /// 法律内容统一结构:章 > 节 > 条 > 款(ADR-027)。
    /// 章/节/条做目录,条款做正文;所有法律统一此结构(章/节/条必有,款可空)。
    /// 宪法双语(`_en` 全填),其他法律单语(`_en` 为 None)。
    /// 条文款(第 N 款,正文)。
    #[derive(
        Encode,
        Decode,
        DecodeWithMemTracking,
        CloneNoBound,
        PartialEqNoBound,
        EqNoBound,
        DebugNoBound,
        TypeInfo,
        MaxEncodedLen,
    )]
    #[scale_info(skip_type_params(T))]
    pub struct Clause<T: Config> {
        /// 款序号(数字)
        pub number: u32,
        /// 款正文(中文)
        pub text: TextOf<T>,
        /// 款正文(英文;宪法填,普通法律 None)
        pub text_en: Option<TextOf<T>>,
    }

    /// 法律条文(第 N 条,目录叶 + 正文)。宪法的 `number` 在全文中全局唯一。
    #[derive(
        Encode,
        Decode,
        DecodeWithMemTracking,
        CloneNoBound,
        PartialEqNoBound,
        EqNoBound,
        DebugNoBound,
        TypeInfo,
        MaxEncodedLen,
    )]
    #[scale_info(skip_type_params(T))]
    pub struct Article<T: Config> {
        /// 条序号(数字；宪法内全局唯一),如第一条 → 1
        pub number: u32,
        /// 条标题(中文,如「第一条」)
        pub title: TitleOf<T>,
        /// 条标题(英文)
        pub title_en: Option<TitleOf<T>>,
        /// 条正文(中文,必填;无款的条放此,有款的条作总述)
        pub body: TextOf<T>,
        /// 条正文(英文)
        pub body_en: Option<TextOf<T>>,
        /// 条下属各款(可空,不是每条都有款)
        pub clauses: BoundedVec<Clause<T>, <T as Config>::MaxClausesPerArticle>,
    }

    /// 法律节(第 N 节,目录)。
    #[derive(
        Encode,
        Decode,
        DecodeWithMemTracking,
        CloneNoBound,
        PartialEqNoBound,
        EqNoBound,
        DebugNoBound,
        TypeInfo,
        MaxEncodedLen,
    )]
    #[scale_info(skip_type_params(T))]
    pub struct Section<T: Config> {
        /// 节序号(数字；宪法内同一章唯一，不同章允许复用)
        pub number: u32,
        /// 节标题(中文)
        pub title: TitleOf<T>,
        /// 节标题(英文)
        pub title_en: Option<TitleOf<T>>,
        /// 节下属各条
        pub articles: BoundedVec<Article<T>, <T as Config>::MaxArticlesPerSection>,
    }

    /// 法律章(第 N 章,目录)。
    #[derive(
        Encode,
        Decode,
        DecodeWithMemTracking,
        CloneNoBound,
        PartialEqNoBound,
        EqNoBound,
        DebugNoBound,
        TypeInfo,
        MaxEncodedLen,
    )]
    #[scale_info(skip_type_params(T))]
    pub struct Chapter<T: Config> {
        /// 章序号(数字；宪法内全局唯一)
        pub number: u32,
        /// 章标题(中文)
        pub title: TitleOf<T>,
        /// 章标题(英文)
        pub title_en: Option<TitleOf<T>>,
        /// 章下属各节
        pub sections: BoundedVec<Section<T>, <T as Config>::MaxSectionsPerChapter>,
    }

    /// 法律全文章节别名:章 > 节 > 条 > 款。
    pub type ChaptersOf<T> = BoundedVec<Chapter<T>, <T as Config>::MaxChaptersPerLaw>;

    /// 法律主体记录(状态 + 版本指针 + 归属立法机构)。
    #[derive(
        Encode, Decode, DecodeWithMemTracking, Clone, PartialEq, Eq, Debug, TypeInfo, MaxEncodedLen,
    )]
    pub struct Law {
        pub law_id: u64,
        pub tier: Tier,
        /// 行政区 code(0 = 全国;省/市用 china.sqlite code,遵守 ADR-021)
        pub scope_code: u32,
        /// 归属立法机构院序列(houses[0] = 发起院,其 admins = 现任议员/委员)。
        /// 单院(市立法会)= 1 项;两院(国家/省立法院)= [众议会, 参议会]。
        pub houses: Houses,
        /// 当前真正生效的版本。新法通过但未到生效时间时为 None。
        pub effective_version: Option<u32>,
        /// 已写入链上的最新版本。
        pub latest_version: u32,
        /// 已通过但未到生效时间的版本。同一法律同一时间只允许一个待生效版本。
        pub pending_version: Option<u32>,
        pub status: LawStatus,
    }

    /// 法律单版本(整部全文快照)。
    #[derive(
        Encode,
        Decode,
        DecodeWithMemTracking,
        CloneNoBound,
        PartialEqNoBound,
        EqNoBound,
        DebugNoBound,
        TypeInfo,
        MaxEncodedLen,
    )]
    #[scale_info(skip_type_params(T))]
    pub struct LawVersion<T: Config> {
        pub law_id: u64,
        pub version: u32,
        pub title: TitleOf<T>,
        pub title_en: Option<TitleOf<T>>,
        /// 法律全文:章 > 节 > 条 > 款
        pub chapters: ChaptersOf<T>,
        /// blake2_256(规范化 SCALE 全文),完整性 + 公投/签名绑定
        pub content_hash: [u8; 32],
        pub vote_type: VoteType,
        pub proposal_id: u64,
        /// 发布时间戳(毫秒)。投票通过写入版本时记录链上时间。
        pub published_at: u64,
        /// 生效时间戳(毫秒)。未到时间的新版本进入待生效队列。
        pub effective_at: u64,
    }

    /// 法律版本展示标签。版本号仍以 `LawVersion.version` 为排序真源;本表只承载特定版本的语义名称。
    #[derive(
        Encode,
        Decode,
        DecodeWithMemTracking,
        CloneNoBound,
        PartialEqNoBound,
        EqNoBound,
        DebugNoBound,
        TypeInfo,
        MaxEncodedLen,
    )]
    #[scale_info(skip_type_params(T))]
    pub struct LawVersionLabel<T: Config> {
        pub title: TitleOf<T>,
        pub title_en: Option<TitleOf<T>>,
    }

    /// 提案摘要:序列化后(带 MODULE_TAG 前缀)存入 votingengine `ProposalData`;
    /// 法律全文条文单独写入 `ProposalObject`,通过回调读回组装新版本。
    #[derive(
        Encode,
        Decode,
        DecodeWithMemTracking,
        CloneNoBound,
        PartialEqNoBound,
        EqNoBound,
        DebugNoBound,
        TypeInfo,
        MaxEncodedLen,
    )]
    #[scale_info(skip_type_params(T))]
    pub struct LawProposalSummary<T: Config> {
        pub action: LawAction,
        /// Enact 时为 0(执行时分配);Amend/Repeal 为目标 law_id
        pub law_id: u64,
        pub tier: Tier,
        pub scope_code: u32,
        /// 归属立法机构院序列(houses[0] = 发起院)。
        pub houses: Houses,
        /// 实际发起机构。与表决院分离，市教育案等场景不一定等于 houses[0]。
        pub actor_cid_number: votingengine::types::CidNumber,
        /// 发起账户在发起机构内实际任职的岗位码；管理员身份本身不产生权限。
        pub proposer_role_code: votingengine::types::RoleCode,
        /// 行政签署机构。
        pub executive_cid_number: votingengine::types::CidNumber,
        /// 两院级三人会签归口立法机构；市级单院必须为空。
        pub legislature_cid_number: Option<votingengine::types::CidNumber>,
        pub vote_type: VoteType,
        pub title: TitleOf<T>,
        pub title_en: Option<TitleOf<T>>,
        pub content_hash: [u8; 32],
        pub effective_at: u64,
    }

    #[pallet::config]
    pub trait Config:
        frame_system::Config + votingengine::Config + pallet_timestamp::Config<Moment = u64>
    {
        #[allow(deprecated)]
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

        /// 立法投票引擎(runtime 装配为 `LegislationVote`),业务壳通过它创建立法投票提案。
        type LegislationVoteEngine: LegislationVoteEngine<Self::AccountId>;

        /// 机构存在性只按 CID 查询，不允许通过某个账户反推机构身份。
        type InstitutionCidQuery: entity_primitives::InstitutionCidQuery<
            votingengine::types::CidNumber,
        >;

        /// 机构岗位权限、有效任职与 CID 顶层能力的统一查询入口。
        type InstitutionRoleAuthorization: InstitutionRoleAuthorizationQuery<Self::AccountId>;

        #[pallet::constant]
        type MaxTitleLen: Get<u32>;
        /// 条/款正文最大字节(body 与 clause text 共用)。
        #[pallet::constant]
        type MaxTextLen: Get<u32>;
        #[pallet::constant]
        type MaxClausesPerArticle: Get<u32>;
        #[pallet::constant]
        type MaxArticlesPerSection: Get<u32>;
        #[pallet::constant]
        type MaxSectionsPerChapter: Get<u32>;
        #[pallet::constant]
        type MaxChaptersPerLaw: Get<u32>;
        #[pallet::constant]
        type MaxLawsPerScope: Get<u32>;
        #[pallet::constant]
        type MaxPendingActivations: Get<u32>;

        type WeightInfo: crate::weights::WeightInfo;
    }

    #[pallet::pallet]
    pub struct Pallet<T>(_);

    /// 不可修改条款 manifest(ADR-027 §6.1,L3 创世锚)。
    /// `article_numbers` 与 `article_hashes` 平行:条号 + 该条规范 SCALE 的 blake2_256 摘要。
    /// 仅 `genesis_build` 写入,创世后永不可变;节点启动据此与二进制清单/创世条文交叉校验。
    #[derive(Clone, PartialEq, Eq, Encode, Decode, MaxEncodedLen, TypeInfo, Debug)]
    pub struct ImmutableManifest {
        pub article_numbers: BoundedVec<u32, ConstU32<{ MAX_IMMUTABLE_ARTICLES }>>,
        pub article_hashes: BoundedVec<[u8; 32], ConstU32<{ MAX_IMMUTABLE_ARTICLES }>>,
    }

    /// 法律自增 ID。
    #[pallet::storage]
    pub type NextLawId<T> = StorageValue<_, u64, ValueQuery>;

    /// 法律主表:law_id → Law。
    #[pallet::storage]
    pub type Laws<T: Config> = StorageMap<_, Blake2_128Concat, u64, Law, OptionQuery>;

    /// 法律全版本历史:(law_id, version) → LawVersion。
    #[pallet::storage]
    pub type LawVersions<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        u64,
        Blake2_128Concat,
        u32,
        LawVersion<T>,
        OptionQuery,
    >;

    /// 法律版本语义标签:(law_id, version) → 标签。无标签时展示层继续显示 `v{version}`。
    #[pallet::storage]
    pub type LawVersionLabels<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        u64,
        Blake2_128Concat,
        u32,
        LawVersionLabel<T>,
        OptionQuery,
    >;

    /// 列表索引:(tier, scope_code) → [law_id]。供客户端按层级/行政区列出法律。
    #[pallet::storage]
    pub type LawsByScope<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        Tier,
        Blake2_128Concat,
        u32,
        BoundedVec<u64, <T as Config>::MaxLawsPerScope>,
        ValueQuery,
    >;

    /// 待生效版本队列。每个区块用链上时间戳扫描,到时间即翻为 Effective。
    #[pallet::storage]
    pub type PendingActivations<T: Config> =
        StorageValue<_, BoundedVec<(u64, u32), <T as Config>::MaxPendingActivations>, ValueQuery>;

    /// 不可修改条款 manifest(创世冻结,无 setter,见 [`ImmutableManifest`])。
    #[pallet::storage]
    pub type ConstitutionImmutableManifest<T: Config> =
        StorageValue<_, ImmutableManifest, OptionQuery>;

    /// 核心修宪(第一章总则核心条款,走特别案)的**永久公投凭据**:`version → (eligible, yes, no)`。
    /// 宪法(law_id=0)专用;`write_law_version` 对核心章改动版本写入(需过公投口径
    /// `primitives::constitution::referendum_passed`),供节点守卫逐块背书(第十九条,ADR-027 §6.3)。
    /// 永久保留(votingengine 90 天清理不涉及本表),故节点可对生效/待生效版本随时校验。
    #[pallet::storage]
    pub type ConstitutionAmendmentProof<T: Config> =
        StorageMap<_, Blake2_128Concat, u32, (u64, u64, u64), OptionQuery>;

    /// 修宪的**永久护宪终审凭据**:`version → 护宪大法官赞成票数`。
    /// **所有** tier=宪法 的 Amend 版本(含一般章重要案)写入(需过 4/7 口径
    /// `primitives::constitution::guard_review_passed`),供节点守卫逐块背书(第21条,ADR-027 §6.3)。
    /// 永久保留(votingengine 90 天清理不涉及本表)。
    #[pallet::storage]
    pub type ConstitutionGuardVoteProof<T: Config> =
        StorageMap<_, Blake2_128Concat, u32, u32, OptionQuery>;

    /// 创世配置:注入内置公民宪法作为 `tier=宪法`、`law_id=0` 的链上法律(宪法唯一真源)。
    #[pallet::genesis_config]
    pub struct GenesisConfig<T: Config> {
        /// 宪法归属立法机构院序列(默认 [国家立法院]);为空则不注入宪法。
        pub constitution_houses: Houses,
        pub _phantom: core::marker::PhantomData<T>,
    }

    impl<T: Config> Default for GenesisConfig<T> {
        fn default() -> Self {
            let nlg_cid_number = CHINA_LF[0]
                .cid_number
                .as_bytes()
                .to_vec()
                .try_into()
                .expect("国家立法院 CID 必须满足协议长度");
            let constitution_houses =
                BoundedVec::try_from(sp_runtime::sp_std::vec![nlg_cid_number,])
                    .expect("constitution houses within bound");
            Self {
                constitution_houses,
                _phantom: Default::default(),
            }
        }
    }

    #[pallet::genesis_build]
    impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
        fn build(&self) {
            if self.constitution_houses.is_empty() {
                return;
            }
            let chapters = ChaptersOf::<T>::decode(&mut &CONSTITUTION_SCALE[..])
                .expect("内置 constitution.scale 必须解码为 ChaptersOf");
            Pallet::<T>::ensure_unique_constitution_numbers(&chapters)
                .expect("内置 constitution.scale 的章号、同章节号、条号必须唯一");

            // L3 创世锚:逐条断言不可修改条款存在(缺即 panic,烤不出非法创世),
            // 并把条号 + 内容摘要冻结进链上 manifest,供节点启动期交叉校验(ADR-027 §6.1)。
            let mut article_numbers =
                BoundedVec::<u32, ConstU32<{ MAX_IMMUTABLE_ARTICLES }>>::new();
            let mut article_hashes =
                BoundedVec::<[u8; 32], ConstU32<{ MAX_IMMUTABLE_ARTICLES }>>::new();
            for &n in IMMUTABLE_CONSTITUTION_ARTICLES.iter() {
                let article = Pallet::<T>::find_article(&chapters, n)
                    .unwrap_or_else(|| panic!("不可修改条款第 {n} 条必须存在于创世宪法"));
                article_numbers
                    .try_push(n)
                    .expect("MAX_IMMUTABLE_ARTICLES 必须 >= 不可修改条款数");
                article_hashes
                    .try_push(sp_io::hashing::blake2_256(&article.encode()))
                    .expect("MAX_IMMUTABLE_ARTICLES 必须 >= 不可修改条款数");
            }
            ConstitutionImmutableManifest::<T>::put(ImmutableManifest {
                article_numbers,
                article_hashes,
            });

            let title = BoundedVec::try_from("公民宪法".as_bytes().to_vec())
                .expect("constitution title within bound");
            let title_en = BoundedVec::try_from("Citizen Constitution".as_bytes().to_vec())
                .expect("constitution title_en within bound");
            let now = 0u64;
            let law_id = NextLawId::<T>::mutate(|n| {
                let id = *n;
                *n = n.saturating_add(1);
                id
            });
            let version = 1u32;
            let lv = LawVersion::<T> {
                law_id,
                version,
                title,
                title_en: Some(title_en),
                content_hash: Pallet::<T>::hash_chapters(&chapters),
                chapters,
                vote_type: VoteType::Special,
                proposal_id: 0,
                published_at: now,
                effective_at: now,
            };
            LawVersions::<T>::insert(law_id, version, lv);
            for label in GENESIS_LAW_VERSION_LABELS.iter() {
                assert_eq!(
                    label.law_id, law_id,
                    "创世法律版本标签 law_id 必须匹配创世宪法"
                );
                assert_eq!(
                    label.version, version,
                    "创世法律版本标签 version 必须匹配创世宪法"
                );
                let label_title: TitleOf<T> = BoundedVec::try_from(label.title.as_bytes().to_vec())
                    .expect("genesis law version label title within bound");
                let label_title_en: TitleOf<T> =
                    BoundedVec::try_from(label.title_en.as_bytes().to_vec())
                        .expect("genesis law version label title_en within bound");
                LawVersionLabels::<T>::insert(
                    label.law_id,
                    label.version,
                    LawVersionLabel::<T> {
                        title: label_title,
                        title_en: Some(label_title_en),
                    },
                );
            }
            let law = Law {
                law_id,
                tier: Tier::Constitution,
                scope_code: 0,
                houses: self.constitution_houses.clone(),
                effective_version: Some(version),
                latest_version: version,
                pending_version: None,
                status: LawStatus::Effective,
            };
            Laws::<T>::insert(law_id, law);
            let _ = LawsByScope::<T>::try_mutate(Tier::Constitution, 0, |v| v.try_push(law_id));
        }
    }

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        /// 立法/修法/废法提案已创建并进入投票。
        LawProposalCreated {
            proposal_id: u64,
            action: LawAction,
            law_id: Option<u64>,
            proposer_account_id: T::AccountId,
        },
        /// 提案被否决。
        LawProposalRejected { proposal_id: u64 },
        /// 新法已立(待生效)。
        LawEnacted { law_id: u64, version: u32 },
        /// 法律已修订(新版本待生效)。
        LawAmended { law_id: u64, version: u32 },
        /// 法律已废止。
        LawRepealed { law_id: u64 },
        /// 法律版本已生效。
        LawEffective { law_id: u64, version: u32 },
    }

    #[pallet::error]
    pub enum Error<T> {
        /// 标题为空
        EmptyTitle,
        /// 法律全文(章节)为空
        EmptyChapters,
        /// 院序列为空
        EmptyHouses,
        /// 发起人不是该立法机构的现任议员/委员(admins)
        NotLegislator,
        /// 表决、签署或护宪阶段没有唯一合法岗位权限主体。
        InvalidLegislationRole,
        /// 提案机构/表决类型/院结构/签署机构不符合宪法路由(第45/46/75/79/100/106条)
        RoutingMismatch,
        /// 宪法修改的表决类型不合法(只能特别案或重要案)
        InvalidVoteTypeForConstitution,
        /// 同一宪法版本中出现重复章号。
        DuplicateChapterNumber,
        /// 同一章中出现重复节号；不同章允许使用相同节号。
        DuplicateSectionNumber,
        /// 同一宪法版本中出现重复条号。
        DuplicateArticleNumber,
        /// 命中宪法不可修改条款(第 1/2/3/17/19/24/34/42 条)
        ImmutableArticleViolation,
        /// 修改第一章总则核心条款必须走特别案表决(宪法第十九条)
        CoreClauseRequiresSpecial,
        /// 修改第一章以外的一般条款必须走重要案表决(宪法第十九条)
        GeneralClauseRequiresMajor,
        /// 修宪提案未改动任何条文(空提案)
        EmptyAmendment,
        /// 核心修宪写入时取不到该提案的强制公投结果(第十九条:核心章改动须经公投)
        ReferendumProofMissing,
        /// 核心修宪的公投未达通过口径(≥70% 参与 + ≥70% 赞成)
        ReferendumNotPassed,
        /// 修宪写入时取不到该提案的护宪大法官终审结果(第21条:一切修宪须经护宪终审)
        GuardReviewProofMissing,
        /// 修宪的护宪大法官终审未达通过口径(4 名及以上赞成)
        GuardReviewNotPassed,
        /// 宪法不可整体废止
        CannotRepealConstitution,
        /// 宪法唯一真源 = 创世 law_id=0,不可经立法入口新立第二部宪法
        CannotEnactConstitution,
        /// 该法律已有未生效(Pending)修订,生效前不得再次提交修订
        AmendmentAlreadyPending,
        /// 法律不存在
        LawNotFound,
        /// 法律版本不存在
        LawVersionNotFound,
        /// 法律已废止,不能再修改
        LawAlreadyRepealed,
        /// 立法投票引擎建提案失败
        VoteEngineCreateFailed,
        /// votingengine 提案载荷缺失或解码失败
        ProposalPayloadInvalid,
        /// 该 (tier, scope) 下法律数量超上限
        TooManyLawsInScope,
        /// 待生效版本队列超上限
        TooManyActivations,
    }

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
        /// 用链上时间戳扫描待生效队列,到时间后自动切换生效版本。
        fn on_initialize(_now: BlockNumberFor<T>) -> Weight {
            let now_ms = Self::now_ms();
            let pending = PendingActivations::<T>::take();
            let mut remain = BoundedVec::<(u64, u32), <T as Config>::MaxPendingActivations>::new();
            let mut activated = 0u64;
            let mut retained = 0u64;
            for (law_id, version) in pending.into_iter() {
                let should_activate = LawVersions::<T>::get(law_id, version)
                    .map(|v| v.effective_at <= now_ms)
                    .unwrap_or(false);
                if should_activate {
                    Self::set_effective(law_id, version);
                    activated = activated.saturating_add(1);
                } else if remain.try_push((law_id, version)).is_ok() {
                    retained = retained.saturating_add(1);
                }
            }
            if !remain.is_empty() {
                PendingActivations::<T>::put(remain);
            }
            T::DbWeight::get().reads_writes(
                activated.saturating_add(retained).saturating_add(2),
                activated.saturating_add(2),
            )
        }
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        /// 立法(新法):立法机构议员/委员发起,走立法投票。
        #[pallet::call_index(0)]
        #[pallet::weight(<T as Config>::WeightInfo::propose_enact_law())]
        pub fn propose_enact_law(
            origin: OriginFor<T>,
            tier: Tier,
            scope_code: u32,
            houses: Houses,
            actor_cid_number: votingengine::types::CidNumber,
            proposer_role_code: votingengine::types::RoleCode,
            executive_cid_number: votingengine::types::CidNumber,
            legislature_cid_number: Option<votingengine::types::CidNumber>,
            vote_type: VoteType,
            title: TitleOf<T>,
            title_en: Option<TitleOf<T>>,
            chapters: ChaptersOf<T>,
            effective_at: u64,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            // 宪法唯一真源 = 创世注入的 law_id=0,立法入口永不能新立第二部宪法(ADR-027 §6.1)。
            ensure!(
                tier != Tier::Constitution,
                Error::<T>::CannotEnactConstitution
            );
            ensure!(!title.is_empty(), Error::<T>::EmptyTitle);
            ensure!(!chapters.is_empty(), Error::<T>::EmptyChapters);
            Self::ensure_legislator(
                &actor_cid_number,
                &proposer_role_code,
                &who,
                entity_primitives::business_action::ACTION_ENACT_LAW,
            )?;
            Self::ensure_tier_vote_type(tier, vote_type)?;
            Self::ensure_routing(
                tier,
                scope_code,
                &actor_cid_number,
                &houses,
                &executive_cid_number,
                vote_type,
                &legislature_cid_number,
            )?;

            let summary = LawProposalSummary::<T> {
                action: LawAction::Enact,
                law_id: 0,
                tier,
                scope_code,
                houses: houses.clone(),
                actor_cid_number,
                proposer_role_code,
                executive_cid_number,
                legislature_cid_number,
                vote_type,
                title,
                title_en,
                content_hash: Self::hash_chapters(&chapters),
                effective_at,
            };
            let proposal_id =
                Self::dispatch_to_engine(&who, &houses, vote_type, &summary, &chapters)?;
            Self::deposit_event(Event::<T>::LawProposalCreated {
                proposal_id,
                action: LawAction::Enact,
                law_id: None,
                proposer_account_id: who,
            });
            Ok(())
        }

        /// 修法:针对既有法律提交新版本(整部全文快照),走立法投票。
        #[pallet::call_index(1)]
        #[pallet::weight(<T as Config>::WeightInfo::propose_amend_law())]
        pub fn propose_amend_law(
            origin: OriginFor<T>,
            law_id: u64,
            actor_cid_number: votingengine::types::CidNumber,
            proposer_role_code: votingengine::types::RoleCode,
            executive_cid_number: votingengine::types::CidNumber,
            legislature_cid_number: Option<votingengine::types::CidNumber>,
            vote_type: VoteType,
            title: TitleOf<T>,
            title_en: Option<TitleOf<T>>,
            chapters: ChaptersOf<T>,
            effective_at: u64,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            ensure!(!title.is_empty(), Error::<T>::EmptyTitle);
            ensure!(!chapters.is_empty(), Error::<T>::EmptyChapters);
            let law = Laws::<T>::get(law_id).ok_or(Error::<T>::LawNotFound)?;
            ensure!(
                law.status != LawStatus::Repealed,
                Error::<T>::LawAlreadyRepealed
            );
            // 至多一个待生效版本:有未生效版本时不得再修,避免新版本互相覆盖。
            ensure!(
                law.pending_version.is_none(),
                Error::<T>::AmendmentAlreadyPending
            );
            Self::ensure_legislator(
                &actor_cid_number,
                &proposer_role_code,
                &who,
                entity_primitives::business_action::ACTION_AMEND_LAW,
            )?;
            Self::ensure_tier_vote_type(law.tier, vote_type)?;
            Self::ensure_routing(
                law.tier,
                law.scope_code,
                &actor_cid_number,
                &law.houses,
                &executive_cid_number,
                vote_type,
                &legislature_cid_number,
            )?;
            if law.tier == Tier::Constitution {
                // 宪法目录身份先按章/节/条三层唯一性校验，避免后续按条号查找时
                // `.find()` 或集合去重掩盖重复条文。
                Self::ensure_unique_constitution_numbers(&chapters)?;
                let effective_version = law
                    .effective_version
                    .ok_or(Error::<T>::LawVersionNotFound)?;
                // 第十九条章→档位强制 + 不可修改条款冻结(提案入口)。
                Self::ensure_constitution_amend_ok(
                    law_id,
                    effective_version,
                    vote_type,
                    &chapters,
                )?;
            }

            let summary = LawProposalSummary::<T> {
                action: LawAction::Amend,
                law_id,
                tier: law.tier,
                scope_code: law.scope_code,
                houses: law.houses.clone(),
                actor_cid_number,
                proposer_role_code,
                executive_cid_number,
                legislature_cid_number,
                vote_type,
                title,
                title_en,
                content_hash: Self::hash_chapters(&chapters),
                effective_at,
            };
            let proposal_id =
                Self::dispatch_to_engine(&who, &law.houses, vote_type, &summary, &chapters)?;
            Self::deposit_event(Event::<T>::LawProposalCreated {
                proposal_id,
                action: LawAction::Amend,
                law_id: Some(law_id),
                proposer_account_id: who,
            });
            Ok(())
        }

        /// 废法:废止既有法律,走立法投票。宪法不可整体废止。
        #[pallet::call_index(2)]
        #[pallet::weight(<T as Config>::WeightInfo::propose_repeal_law())]
        pub fn propose_repeal_law(
            origin: OriginFor<T>,
            law_id: u64,
            actor_cid_number: votingengine::types::CidNumber,
            proposer_role_code: votingengine::types::RoleCode,
            executive_cid_number: votingengine::types::CidNumber,
            legislature_cid_number: Option<votingengine::types::CidNumber>,
            vote_type: VoteType,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            let law = Laws::<T>::get(law_id).ok_or(Error::<T>::LawNotFound)?;
            ensure!(
                law.status != LawStatus::Repealed,
                Error::<T>::LawAlreadyRepealed
            );
            ensure!(
                law.tier != Tier::Constitution,
                Error::<T>::CannotRepealConstitution
            );
            Self::ensure_legislator(
                &actor_cid_number,
                &proposer_role_code,
                &who,
                entity_primitives::business_action::ACTION_REPEAL_LAW,
            )?;
            Self::ensure_tier_vote_type(law.tier, vote_type)?;
            Self::ensure_routing(
                law.tier,
                law.scope_code,
                &actor_cid_number,
                &law.houses,
                &executive_cid_number,
                vote_type,
                &legislature_cid_number,
            )?;

            let summary = LawProposalSummary::<T> {
                action: LawAction::Repeal,
                law_id,
                tier: law.tier,
                scope_code: law.scope_code,
                houses: law.houses.clone(),
                actor_cid_number,
                proposer_role_code,
                executive_cid_number,
                legislature_cid_number,
                vote_type,
                title: Default::default(),
                title_en: None,
                content_hash: [0u8; 32],
                effective_at: Default::default(),
            };
            let empty: ChaptersOf<T> = Default::default();
            let proposal_id =
                Self::dispatch_to_engine(&who, &law.houses, vote_type, &summary, &empty)?;
            Self::deposit_event(Event::<T>::LawProposalCreated {
                proposal_id,
                action: LawAction::Repeal,
                law_id: Some(law_id),
                proposer_account_id: who,
            });
            Ok(())
        }
    }

    // ──────────────── 内部 helper:校验 / 编排 / 执行器 / 查询 ────────────────
    impl<T: Config> Pallet<T> {
        /// 校验发起人是提案机构(proposer_body)的现任管理员(议员/委员)。
        /// ADR-027 修订:提案方与表决院解耦——市行政区 市自治会/市教委会 委员可提案,
        /// 但表决院恒为 houses[0]=市立法会,故 auth 校验对 proposer_body 而非 houses[0]。
        fn ensure_legislator(
            actor_cid_number: &votingengine::types::CidNumber,
            proposer_role_code: &votingengine::types::RoleCode,
            who: &T::AccountId,
            action_code: u32,
        ) -> DispatchResult {
            let _ = Self::institution_code_for_cid(actor_cid_number)?;
            let business_action_id = BusinessActionId {
                module_tag: MODULE_TAG.to_vec(),
                action_code,
            };
            ensure!(
                T::InstitutionRoleAuthorization::is_authorized(
                    who,
                    &RoleSubject {
                        cid_number: actor_cid_number.to_vec(),
                        role_code: proposer_role_code.to_vec(),
                    },
                    &business_action_id,
                    RolePermissionOperation::Propose,
                ),
                Error::<T>::NotLegislator
            );
            Ok(())
        }

        fn institution_code_for_cid(
            cid_number: &votingengine::types::CidNumber,
        ) -> Result<InstitutionCode, DispatchError> {
            let text = core::str::from_utf8(cid_number.as_slice())
                .map_err(|_| Error::<T>::RoutingMismatch)?;
            votingengine::types::institution_code_from_cid_number(text)
                .ok_or_else(|| Error::<T>::RoutingMismatch.into())
        }

        /// 校验一项路由机构 CID 确实存在，且 CID 中的机构码符合该法律路线。
        /// 返回 CID 的 R5 行政区段，供省/市级整条路由做同域绑定。
        fn ensure_route_institution(
            cid_number: &votingengine::types::CidNumber,
            expected_code: InstitutionCode,
        ) -> Result<Vec<u8>, DispatchError> {
            ensure!(
                T::InstitutionCidQuery::cid_exists(cid_number),
                Error::<T>::RoutingMismatch
            );
            ensure!(
                Self::institution_code_for_cid(cid_number)? == expected_code,
                Error::<T>::RoutingMismatch
            );
            let parts =
                primitives::cid::number::parse_cid_number_parts_bytes(cid_number.as_slice())
                    .map_err(|_| Error::<T>::RoutingMismatch)?;
            ensure!(
                parts.institution == expected_code,
                Error::<T>::RoutingMismatch
            );
            Ok(parts.r5.into_bytes())
        }

        /// 校验机构属于允许的发起机构码之一，并返回行政区段。
        fn ensure_route_institution_one_of(
            cid_number: &votingengine::types::CidNumber,
            expected_codes: &[InstitutionCode],
        ) -> Result<Vec<u8>, DispatchError> {
            ensure!(
                T::InstitutionCidQuery::cid_exists(cid_number),
                Error::<T>::RoutingMismatch
            );
            let actual = Self::institution_code_for_cid(cid_number)?;
            ensure!(
                expected_codes.contains(&actual),
                Error::<T>::RoutingMismatch
            );
            let parts =
                primitives::cid::number::parse_cid_number_parts_bytes(cid_number.as_slice())
                    .map_err(|_| Error::<T>::RoutingMismatch)?;
            ensure!(parts.institution == actual, Error::<T>::RoutingMismatch);
            Ok(parts.r5.into_bytes())
        }

        /// 路由校验(ADR-027,宪法第45/46/75/79/100/106条)。
        ///
        /// 调用方仍传原有字段，但链端只接受固定的层级×表决类型路由；所有机构账户
        /// 必须由 entity 真源解析为预期机构码。省、市路由还必须共享同一 CID R5，
        /// 防止把不同省市的院、政府或提案机构拼进同一法律案。
        fn ensure_routing(
            tier: Tier,
            scope_code: u32,
            actor_cid_number: &votingengine::types::CidNumber,
            houses: &Houses,
            executive_cid_number: &votingengine::types::CidNumber,
            vote_type: VoteType,
            legislature_cid_number: &Option<votingengine::types::CidNumber>,
        ) -> DispatchResult {
            let education = vote_type.is_education();
            let (expected_proposers, expected_houses, expected_executive, expected_legislature): (
                &[InstitutionCode],
                &[InstitutionCode],
                InstitutionCode,
                Option<InstitutionCode>,
            ) = match (tier, education) {
                (Tier::Constitution, false) | (Tier::National, false) => (
                    &[*b"NRP\0"],
                    &[*b"NRP\0", *b"NSN\0"],
                    *b"PRS\0",
                    Some(*b"NLG\0"),
                ),
                (Tier::National, true) => (
                    &[*b"NED\0"],
                    &[*b"NED\0", *b"NSN\0"],
                    *b"PRS\0",
                    Some(*b"NLG\0"),
                ),
                (Tier::Provincial, false) => (
                    &[*b"PRP\0"],
                    &[*b"PRP\0", *b"PSN\0"],
                    *b"PGV\0",
                    Some(*b"PLG\0"),
                ),
                (Tier::Municipal, false) => (&[*b"CSLF", *b"CLEG"], &[*b"CLEG"], *b"CGOV", None),
                (Tier::Municipal, true) => (&[*b"CEDU", *b"CLEG"], &[*b"CLEG"], *b"CGOV", None),
                // 省没有省教委会，宪法也不存在教育案路由。
                _ => return Err(Error::<T>::RoutingMismatch.into()),
            };

            ensure!(
                houses.len() == expected_houses.len(),
                Error::<T>::RoutingMismatch
            );
            let regional = matches!(tier, Tier::Provincial | Tier::Municipal);
            if regional {
                ensure!(scope_code != 0, Error::<T>::RoutingMismatch);
            } else {
                ensure!(scope_code == 0, Error::<T>::RoutingMismatch);
            }

            let route_scope =
                Self::ensure_route_institution_one_of(actor_cid_number, expected_proposers)?;
            for (house, expected_code) in houses.iter().zip(expected_houses.iter()) {
                let scope = Self::ensure_route_institution(house, *expected_code)?;
                ensure!(
                    !regional || scope == route_scope,
                    Error::<T>::RoutingMismatch
                );
            }
            let executive_scope =
                Self::ensure_route_institution(executive_cid_number, expected_executive)?;
            ensure!(
                !regional || executive_scope == route_scope,
                Error::<T>::RoutingMismatch
            );
            match (legislature_cid_number.as_ref(), expected_legislature) {
                (Some(actual), Some(expected_code)) => {
                    let scope = Self::ensure_route_institution(actual, expected_code)?;
                    ensure!(
                        !regional || scope == route_scope,
                        Error::<T>::RoutingMismatch
                    );
                }
                (None, None) => {}
                _ => return Err(Error::<T>::RoutingMismatch.into()),
            }
            Ok(())
        }

        /// 宪法修改只能走特别案或重要案(宪法第十九条);教育变体不适用于宪法。
        fn ensure_tier_vote_type(tier: Tier, vt: VoteType) -> DispatchResult {
            if tier == Tier::Constitution {
                ensure!(
                    matches!(vt, VoteType::Special | VoteType::Major),
                    Error::<T>::InvalidVoteTypeForConstitution
                );
            }
            Ok(())
        }

        /// 在章>节>条嵌套结构里按条号查找条文。
        fn find_article(chapters: &ChaptersOf<T>, number: u32) -> Option<&Article<T>> {
            chapters
                .iter()
                .flat_map(|c| c.sections.iter())
                .flat_map(|s| s.articles.iter())
                .find(|a| a.number == number)
        }

        /// 校验单一宪法版本的三层编号唯一性。
        ///
        /// - 章号在全文中全局唯一；
        /// - 节号只在所属章内唯一，不同章允许复用；
        /// - 条号在全文中全局唯一。
        ///
        /// 本规则只约束唯一性，不要求编号连续，也不要求从 1 开始。
        pub(crate) fn ensure_unique_constitution_numbers(
            chapters: &ChaptersOf<T>,
        ) -> DispatchResult {
            let mut chapter_numbers = BTreeSet::new();
            let mut article_numbers = BTreeSet::new();
            for chapter in chapters {
                ensure!(
                    chapter_numbers.insert(chapter.number),
                    Error::<T>::DuplicateChapterNumber
                );
                let mut section_numbers = BTreeSet::new();
                for section in &chapter.sections {
                    ensure!(
                        section_numbers.insert(section.number),
                        Error::<T>::DuplicateSectionNumber
                    );
                    for article in &section.articles {
                        ensure!(
                            article_numbers.insert(article.number),
                            Error::<T>::DuplicateArticleNumber
                        );
                    }
                }
            }
            Ok(())
        }

        /// 宪法不可修改条款必须逐字保持一致(增/改/删任一即违规)。
        /// 遍历 章>节>条 按条号比对当前生效全文与提案全文。
        fn ensure_immutable_articles_unchanged(
            current_chapters: &ChaptersOf<T>,
            new_chapters: &ChaptersOf<T>,
        ) -> DispatchResult {
            for &n in IMMUTABLE_CONSTITUTION_ARTICLES.iter() {
                ensure!(
                    Self::find_article(current_chapters, n) == Self::find_article(new_chapters, n),
                    Error::<T>::ImmutableArticleViolation
                );
            }
            Ok(())
        }

        /// 收集「章>节>条」结构里全部条号(升序去重)。
        fn all_article_numbers(chapters: &ChaptersOf<T>) -> Vec<u32> {
            let mut ns: Vec<u32> = chapters
                .iter()
                .flat_map(|c| c.sections.iter())
                .flat_map(|s| s.articles.iter())
                .map(|a| a.number)
                .collect();
            ns.sort_unstable();
            ns.dedup();
            ns
        }

        /// 收集核心章(第一章总则,`chapters[CONSTITUTION_CORE_CHAPTER_INDEX]`)的全部条号。
        fn core_chapter_article_numbers(chapters: &ChaptersOf<T>) -> Vec<u32> {
            chapters
                .get(CONSTITUTION_CORE_CHAPTER_INDEX)
                .into_iter()
                .flat_map(|c| c.sections.iter())
                .flat_map(|s| s.articles.iter())
                .map(|a| a.number)
                .collect()
        }

        /// 判定宪法修改的改动范围(第十九条章→档位):对新旧全文逐条 diff 得变更条号,
        /// 取核心章条号(旧∪新,覆盖增/删/改核心条),交
        /// [`primitives::constitution::classify`] 判定。runtime 与节点守卫共用该判定单源。
        pub(crate) fn constitution_amendment_scope(
            current_chapters: &ChaptersOf<T>,
            new_chapters: &ChaptersOf<T>,
        ) -> AmendmentScope {
            // 变更条号 = 新旧全部条号并集里,find_article 结果不等者(条内容含 clauses 变化亦算)。
            let mut all = Self::all_article_numbers(current_chapters);
            all.extend(Self::all_article_numbers(new_chapters));
            all.sort_unstable();
            all.dedup();
            let changed: Vec<u32> = all
                .into_iter()
                .filter(|&n| {
                    Self::find_article(current_chapters, n) != Self::find_article(new_chapters, n)
                })
                .collect();
            // 核心章条号 = 第一章总则(旧∪新):新增条落入首章亦视为触碰核心章。
            let mut core = Self::core_chapter_article_numbers(current_chapters);
            core.extend(Self::core_chapter_article_numbers(new_chapters));
            let article_scope =
                constitution::classify(&changed, &core, &IMMUTABLE_CONSTITUTION_ARTICLES);
            let title_scope = Self::title_amendment_scope(current_chapters, new_chapters);
            Self::stricter_scope(article_scope, title_scope)
        }

        /// 章/节标题变更单独判定的档位。
        ///
        /// 章节标题是承载条文的目录结构,本身没有条号,故不能并入逐条 diff 的 `changed`
        /// 交由 [`constitution::classify`] 判定——那样会因所辖条号命中禁改清单而误判为
        /// [`AmendmentScope::ImmutableViolation`]。第十九条禁改保护的对象是**条**本身,
        /// 不是它所在的标题:只要禁改条内容逐字未变(由
        /// [`Self::ensure_immutable_articles_unchanged`] 在档位判定之前独立守卫),改动其
        /// 所在章节标题只按所属章走档位。
        ///
        /// 判定:核心章(第一章总则)的章标题或其下任一节标题变更 → `CoreChapter`;
        /// 第二章及以后 → `GeneralOnly`;均未变更 → `NoChange`。
        /// 章、节按 `number` 对齐而非下标,避免增删章节导致错位比对。
        fn title_amendment_scope(
            current_chapters: &ChaptersOf<T>,
            new_chapters: &ChaptersOf<T>,
        ) -> AmendmentScope {
            let core_number = current_chapters
                .get(CONSTITUTION_CORE_CHAPTER_INDEX)
                .or_else(|| new_chapters.get(CONSTITUTION_CORE_CHAPTER_INDEX))
                .map(|c| c.number);
            let mut general = false;
            let mut numbers: Vec<u32> = current_chapters.iter().map(|c| c.number).collect();
            numbers.extend(new_chapters.iter().map(|c| c.number));
            numbers.sort_unstable();
            numbers.dedup();
            for n in numbers {
                let old = current_chapters.iter().find(|c| c.number == n);
                let new = new_chapters.iter().find(|c| c.number == n);
                let changed = match (old, new) {
                    // 章新增或删除:目录结构变更,按该章位置计入。
                    (None, Some(_)) | (Some(_), None) => true,
                    (Some(o), Some(w)) => {
                        o.title != w.title
                            || o.title_en != w.title_en
                            || Self::section_titles_changed(o, w)
                    }
                    (None, None) => false,
                };
                if !changed {
                    continue;
                }
                if Some(n) == core_number {
                    return AmendmentScope::CoreChapter;
                }
                general = true;
            }
            if general {
                AmendmentScope::GeneralOnly
            } else {
                AmendmentScope::NoChange
            }
        }

        /// 同一章内节标题(含英文)是否变更;节按 `number` 对齐,增删节亦记为变更。
        fn section_titles_changed(old: &Chapter<T>, new: &Chapter<T>) -> bool {
            let mut numbers: Vec<u32> = old.sections.iter().map(|s| s.number).collect();
            numbers.extend(new.sections.iter().map(|s| s.number));
            numbers.sort_unstable();
            numbers.dedup();
            numbers.into_iter().any(|n| {
                let o = old.sections.iter().find(|s| s.number == n);
                let w = new.sections.iter().find(|s| s.number == n);
                match (o, w) {
                    (None, Some(_)) | (Some(_), None) => true,
                    (Some(a), Some(b)) => a.title != b.title || a.title_en != b.title_en,
                    (None, None) => false,
                }
            })
        }

        /// 取两个档位中更严的一档。严格性:
        /// `ImmutableViolation` > `CoreChapter` > `GeneralOnly` > `NoChange`。
        /// 同批改动只要触及更严的一档即按更严处理,与
        /// [`constitution::classify`] 内部的优先级语义一致。
        fn stricter_scope(a: AmendmentScope, b: AmendmentScope) -> AmendmentScope {
            fn rank(s: AmendmentScope) -> u8 {
                match s {
                    AmendmentScope::NoChange => 0,
                    AmendmentScope::GeneralOnly => 1,
                    AmendmentScope::CoreChapter => 2,
                    AmendmentScope::ImmutableViolation => 3,
                }
            }
            if rank(a) >= rank(b) {
                a
            } else {
                b
            }
        }

        /// 宪法修改专用校验(第十九条章→档位强制),供提案入口与提交层复校验共用,
        /// 防回调/内部路径绕过。语义:
        ///   ① 不可修改条款逐字冻结(权威 byte-for-byte);
        ///   ② 核心章条款改动 → 必须特别案 Special;一般章改动 → 必须重要案 Major;空改动 → 拒。
        fn ensure_constitution_amend_ok(
            law_id: u64,
            effective_version: u32,
            vote_type: VoteType,
            new_chapters: &ChaptersOf<T>,
        ) -> DispatchResult {
            let current = LawVersions::<T>::get(law_id, effective_version)
                .ok_or(Error::<T>::LawVersionNotFound)?;
            Self::ensure_immutable_articles_unchanged(&current.chapters, new_chapters)?;
            match Self::constitution_amendment_scope(&current.chapters, new_chapters) {
                AmendmentScope::NoChange => Err(Error::<T>::EmptyAmendment.into()),
                // 已被 ensure_immutable_articles_unchanged 拦截,此处双保险。
                AmendmentScope::ImmutableViolation => {
                    Err(Error::<T>::ImmutableArticleViolation.into())
                }
                AmendmentScope::CoreChapter => {
                    ensure!(
                        vote_type == VoteType::Special,
                        Error::<T>::CoreClauseRequiresSpecial
                    );
                    Ok(())
                }
                AmendmentScope::GeneralOnly => {
                    ensure!(
                        vote_type == VoteType::Major,
                        Error::<T>::GeneralClauseRequiresMajor
                    );
                    Ok(())
                }
            }
        }

        /// 核心修宪(tier=宪法 且改动落第一章总则核心条款)写入**永久公投凭据**。
        /// 取投票引擎的公投结果 `(eligible, yes, no)`、过通过口径 `referendum_passed`,
        /// 存 `ConstitutionAmendmentProof[new_version]`,供节点守卫逐块背书(第十九条,ADR-027 §6.3)。
        /// 非宪法 / 非核心章改动(一般章走重要案、无公投)→ 无操作。由 `write_law_version` Amend 分支调用。
        fn record_constitution_amendment_proof(
            tier: Tier,
            law_id: u64,
            current_effective_version: Option<u32>,
            new_version: u32,
            new_chapters: &ChaptersOf<T>,
            proposal_id: u64,
        ) -> DispatchResult {
            if tier != Tier::Constitution {
                return Ok(());
            }
            let eff = current_effective_version.ok_or(Error::<T>::LawVersionNotFound)?;
            let current =
                LawVersions::<T>::get(law_id, eff).ok_or(Error::<T>::LawVersionNotFound)?;
            if Self::constitution_amendment_scope(&current.chapters, new_chapters)
                == AmendmentScope::CoreChapter
            {
                let (eligible, yes, no) = T::LegislationVoteEngine::referendum_result(proposal_id)
                    .ok_or(Error::<T>::ReferendumProofMissing)?;
                ensure!(
                    constitution::referendum_passed(eligible, yes, no),
                    Error::<T>::ReferendumNotPassed
                );
                ConstitutionAmendmentProof::<T>::insert(new_version, (eligible, yes, no));
            }
            Ok(())
        }

        /// 修宪(tier=宪法,**任意章**)写入**永久护宪终审凭据**(第21条:一切修宪须经护宪大法官 4/7 终审)。
        /// 取护宪大法官赞成票数、过口径 `guard_review_passed`,存 `ConstitutionGuardVoteProof[new_version]`,
        /// 供节点守卫逐块背书(ADR-027 §6.3)。非宪法 → 无操作。由 `write_law_version` Amend 分支调用。
        fn record_constitution_guard_proof(
            tier: Tier,
            new_version: u32,
            proposal_id: u64,
        ) -> DispatchResult {
            if tier != Tier::Constitution {
                return Ok(());
            }
            let approve = T::LegislationVoteEngine::guard_review_result(proposal_id)
                .ok_or(Error::<T>::GuardReviewProofMissing)?;
            ensure!(
                constitution::guard_review_passed(approve),
                Error::<T>::GuardReviewNotPassed
            );
            ConstitutionGuardVoteProof::<T>::insert(new_version, approve);
            Ok(())
        }

        /// 规范化全文哈希(blake2_256(章节条款 SCALE))。
        pub fn hash_chapters(chapters: &ChaptersOf<T>) -> [u8; 32] {
            sp_io::hashing::blake2_256(&chapters.encode())
        }

        fn action_code(action: LawAction) -> u32 {
            match action {
                LawAction::Enact => entity_primitives::business_action::ACTION_ENACT_LAW,
                LawAction::Amend => entity_primitives::business_action::ACTION_AMEND_LAW,
                LawAction::Repeal => entity_primitives::business_action::ACTION_REPEAL_LAW,
            }
        }

        fn bounded_role_subject(
            cid_number: &[u8],
            role_code: &[u8],
        ) -> Result<RepresentativeBody, DispatchError> {
            Ok(RoleSubject {
                cid_number: cid_number
                    .to_vec()
                    .try_into()
                    .map_err(|_| Error::<T>::InvalidLegislationRole)?,
                role_code: role_code
                    .to_vec()
                    .try_into()
                    .map_err(|_| Error::<T>::InvalidLegislationRole)?,
            })
        }

        fn business_action(action_code: u32) -> BusinessActionId<Vec<u8>> {
            BusinessActionId {
                module_tag: MODULE_TAG.to_vec(),
                action_code,
            }
        }

        /// 每个代表机构阶段必须解析为一个非 LR 表决岗位。岗位码来自 entity 权限真源，
        /// 不是客户端或投票引擎硬编码；LR 的同动作 Vote 仅用于后续签署阶段。
        fn representative_vote_subject(
            cid_number: &votingengine::types::CidNumber,
            action_code: u32,
        ) -> Result<RepresentativeBody, DispatchError> {
            let action = Self::business_action(action_code);
            let mut subjects = T::InstitutionRoleAuthorization::role_subjects_with_permission(
                cid_number.as_slice(),
                &action,
                RolePermissionOperation::Vote,
            );
            subjects.retain(|subject| {
                subject.role_code.as_slice()
                    != primitives::institution_constraints::ROLE_CODE_LEGAL_REPRESENTATIVE
            });
            ensure!(subjects.len() == 1, Error::<T>::InvalidLegislationRole);
            let subject = subjects.pop().ok_or(Error::<T>::InvalidLegislationRole)?;
            Self::bounded_role_subject(&subject.cid_number, &subject.role_code)
        }

        /// 固定岗位签署主体也必须真实持有 underlying action 的 Vote 权限。
        fn fixed_vote_subject(
            cid_number: &[u8],
            role_code: &[u8],
            action_code: u32,
        ) -> Result<RepresentativeBody, DispatchError> {
            let subject = RoleSubject {
                cid_number: cid_number.to_vec(),
                role_code: role_code.to_vec(),
            };
            ensure!(
                T::InstitutionRoleAuthorization::role_has_permission(
                    &subject,
                    &Self::business_action(action_code),
                    RolePermissionOperation::Vote,
                ),
                Error::<T>::InvalidLegislationRole
            );
            Self::bounded_role_subject(cid_number, role_code)
        }

        /// 编码载荷并调立法投票引擎建提案,返回真实提案 ID。
        /// 代表机构序列由提案携带；单机构与顺序多机构路由均由立法投票引擎执行。
        fn dispatch_to_engine(
            who: &T::AccountId,
            houses: &Houses,
            vote_type: VoteType,
            summary: &LawProposalSummary<T>,
            chapters: &ChaptersOf<T>,
        ) -> Result<u64, DispatchError> {
            let mut data = sp_runtime::sp_std::vec::Vec::from(MODULE_TAG);
            data.extend_from_slice(&summary.encode());
            let object = chapters.encode();
            let action_code = Self::action_code(summary.action);
            // 修宪(tier=宪法)走护宪大法官终审(宪法第21条)。
            let needs_guard = summary.tier == Tier::Constitution;
            let bodies: RepresentativeBodies = houses
                .iter()
                .map(|cid| Self::representative_vote_subject(cid, action_code))
                .collect::<Result<Vec<_>, _>>()?
                .try_into()
                .map_err(|_| Error::<T>::VoteEngineCreateFailed)?;
            let route = if bodies.len() == 1 {
                RepresentativeRoute::Single(
                    bodies
                        .first()
                        .cloned()
                        .ok_or(Error::<T>::VoteEngineCreateFailed)?,
                )
            } else {
                RepresentativeRoute::Sequential(bodies)
            };
            // 特别案代表阶段后直接进入公投，再进入护宪终审；不进入行政签署或否决救济。
            let executive = if vote_type == VoteType::Special {
                None
            } else {
                Some(Self::fixed_vote_subject(
                    summary.executive_cid_number.as_slice(),
                    primitives::institution_constraints::ROLE_CODE_LEGAL_REPRESENTATIVE,
                    action_code,
                )?)
            };
            let mut override_signers = Vec::new();
            if vote_type != VoteType::Special {
                if let Some(legislature) = summary.legislature_cid_number.as_ref() {
                    override_signers.push(Self::fixed_vote_subject(
                        legislature.as_slice(),
                        primitives::institution_constraints::ROLE_CODE_LEGAL_REPRESENTATIVE,
                        action_code,
                    )?);
                    for house in houses {
                        override_signers.push(Self::fixed_vote_subject(
                            house.as_slice(),
                            primitives::institution_constraints::ROLE_CODE_LEGAL_REPRESENTATIVE,
                            action_code,
                        )?);
                    }
                }
            }
            let override_signers: BoundedVec<RepresentativeBody, ConstU32<3>> = override_signers
                .try_into()
                .map_err(|_| Error::<T>::InvalidLegislationRole)?;
            let guard = if needs_guard {
                let njd = CHINA_SF
                    .iter()
                    .find(|entry| {
                        votingengine::types::institution_code_from_cid_number(entry.cid_number)
                            == Some(votingengine::types::NJD)
                    })
                    .ok_or(Error::<T>::InvalidLegislationRole)?;
                Some(Self::fixed_vote_subject(
                    njd.cid_number.as_bytes(),
                    primitives::governance_skeleton::ROLE_CODE_CONSTITUTION_GUARD,
                    action_code,
                )?)
            } else {
                None
            };
            let procedure = LegislationProcedureConfig {
                executive: executive.clone(),
                override_signers: override_signers.clone(),
                needs_guard,
                guard: guard.clone(),
            };

            let proposer_subject = Self::bounded_role_subject(
                summary.actor_cid_number.as_slice(),
                summary.proposer_role_code.as_slice(),
            )?;
            let mut voter_subjects = route
                .bodies()
                .into_iter()
                .map(votingengine::types::AuthorizationSubject::Institution)
                .collect::<Vec<_>>();
            voter_subjects.extend(
                executive
                    .into_iter()
                    .chain(override_signers)
                    .chain(guard)
                    .map(votingengine::types::AuthorizationSubject::Institution),
            );
            let owner: BoundedVec<
                u8,
                ConstU32<{ entity_primitives::BUSINESS_MODULE_TAG_MAX_BYTES }>,
            > = MODULE_TAG
                .to_vec()
                .try_into()
                .map_err(|_| Error::<T>::VoteEngineCreateFailed)?;
            let vote_plan = VotePlanOf::<T::AccountId>::try_new(
                BusinessActionId {
                    module_tag: owner.clone(),
                    action_code,
                },
                owner,
                votingengine::types::AuthorizationSubject::Institution(proposer_subject),
                voter_subjects,
                VotingEngineKind::Legislation,
                sp_io::hashing::blake2_256(&(data.clone(), object.clone()).encode()),
            )
            .map_err(|_| Error::<T>::VoteEngineCreateFailed)?;
            let proposal_id = T::LegislationVoteEngine::create_legislation_vote(
                who.clone(),
                summary.actor_cid_number.clone(),
                vote_plan,
                route,
                vote_type.representative_rule(),
                procedure,
                MODULE_TAG,
                data,
                object,
            )
            .map_err(|_| Error::<T>::VoteEngineCreateFailed)?;
            Ok(proposal_id)
        }

        /// 当前链上时间戳(毫秒)。法律生效时间统一使用时间戳,不再暴露区块号给业务端。
        fn now_ms() -> u64 {
            pallet_timestamp::Pallet::<T>::now()
        }

        /// 把某法律版本置为生效;若不是待生效版本则忽略。
        fn set_effective(law_id: u64, version: u32) {
            Laws::<T>::mutate(law_id, |maybe| {
                if let Some(law) = maybe {
                    if law.pending_version == Some(version) {
                        law.effective_version = Some(version);
                        law.pending_version = None;
                        law.status = LawStatus::Effective;
                        Self::deposit_event(Event::<T>::LawEffective { law_id, version });
                    }
                }
            });
        }

        /// 到时间即生效,否则排入待生效队列。
        fn activate_or_schedule(law_id: u64, version: u32, effective_at: u64) -> DispatchResult {
            if effective_at <= Self::now_ms() {
                Self::set_effective(law_id, version);
            } else {
                PendingActivations::<T>::try_mutate(|v| v.try_push((law_id, version)))
                    .map_err(|_| Error::<T>::TooManyActivations)?;
            }
            Ok(())
        }

        /// 最终写入层复校验:回调载荷来自投票引擎,这里在任何 storage 写入前再次校验
        /// 宪法唯一性、不可废止、不可修改条款与 Pending 单飞规则,防未来内部入口绕过 `propose_*`。
        fn ensure_write_law_version_allowed(
            summary: &LawProposalSummary<T>,
            chapters: &ChaptersOf<T>,
        ) -> DispatchResult {
            match summary.action {
                LawAction::Enact => {
                    ensure!(
                        summary.tier != Tier::Constitution,
                        Error::<T>::CannotEnactConstitution
                    );
                    ensure!(!summary.title.is_empty(), Error::<T>::EmptyTitle);
                    ensure!(!chapters.is_empty(), Error::<T>::EmptyChapters);
                    ensure!(!summary.houses.is_empty(), Error::<T>::EmptyHouses);
                    Self::ensure_tier_vote_type(summary.tier, summary.vote_type)?;
                    Self::ensure_routing(
                        summary.tier,
                        summary.scope_code,
                        &summary.actor_cid_number,
                        &summary.houses,
                        &summary.executive_cid_number,
                        summary.vote_type,
                        &summary.legislature_cid_number,
                    )?;
                }
                LawAction::Amend => {
                    ensure!(!summary.title.is_empty(), Error::<T>::EmptyTitle);
                    ensure!(!chapters.is_empty(), Error::<T>::EmptyChapters);
                    let law = Laws::<T>::get(summary.law_id).ok_or(Error::<T>::LawNotFound)?;
                    ensure!(
                        law.status != LawStatus::Repealed,
                        Error::<T>::LawAlreadyRepealed
                    );
                    ensure!(
                        law.pending_version.is_none(),
                        Error::<T>::AmendmentAlreadyPending
                    );
                    ensure!(
                        summary.tier == law.tier
                            && summary.scope_code == law.scope_code
                            && summary.houses == law.houses,
                        Error::<T>::RoutingMismatch
                    );
                    Self::ensure_tier_vote_type(law.tier, summary.vote_type)?;
                    Self::ensure_routing(
                        summary.tier,
                        summary.scope_code,
                        &summary.actor_cid_number,
                        &summary.houses,
                        &summary.executive_cid_number,
                        summary.vote_type,
                        &summary.legislature_cid_number,
                    )?;
                    if law.tier == Tier::Constitution {
                        // 最终写入层独立复核三层编号唯一性，禁止异常回调载荷绕过提案入口。
                        Self::ensure_unique_constitution_numbers(chapters)?;
                        let effective_version = law
                            .effective_version
                            .ok_or(Error::<T>::LawVersionNotFound)?;
                        // 第十九条章→档位强制 + 不可修改条款冻结(提交层复校验,防回调绕过)。
                        Self::ensure_constitution_amend_ok(
                            summary.law_id,
                            effective_version,
                            summary.vote_type,
                            chapters,
                        )?;
                    }
                }
                LawAction::Repeal => {
                    let law = Laws::<T>::get(summary.law_id).ok_or(Error::<T>::LawNotFound)?;
                    ensure!(
                        law.status != LawStatus::Repealed,
                        Error::<T>::LawAlreadyRepealed
                    );
                    ensure!(
                        law.tier != Tier::Constitution,
                        Error::<T>::CannotRepealConstitution
                    );
                    ensure!(
                        summary.tier == law.tier
                            && summary.scope_code == law.scope_code
                            && summary.houses == law.houses,
                        Error::<T>::RoutingMismatch
                    );
                    Self::ensure_tier_vote_type(law.tier, summary.vote_type)?;
                    Self::ensure_routing(
                        summary.tier,
                        summary.scope_code,
                        &summary.actor_cid_number,
                        &summary.houses,
                        &summary.executive_cid_number,
                        summary.vote_type,
                        &summary.legislature_cid_number,
                    )?;
                }
            }
            Ok(())
        }

        /// 投票通过/否决回调的内部写入逻辑(由 legislation-vote 投票终态经核心回调触发)。
        pub fn apply_legislation_vote_result(
            proposal_id: u64,
            approved: bool,
        ) -> Result<ProposalExecutionOutcome, DispatchError> {
            if !votingengine::Pallet::<T>::is_proposal_owner(proposal_id, MODULE_TAG) {
                return Ok(ProposalExecutionOutcome::Ignored);
            }
            if !approved {
                Self::deposit_event(Event::<T>::LawProposalRejected { proposal_id });
                return Ok(ProposalExecutionOutcome::Executed);
            }
            let summary = Self::load_summary(proposal_id)?;
            let chapters = Self::load_chapters(proposal_id)?;
            let now = Self::now_ms();
            Self::write_law_version(proposal_id, summary, chapters, now)?;
            Ok(ProposalExecutionOutcome::Executed)
        }

        /// 从 votingengine ProposalData 读回并解码本模块提案摘要(先校验 MODULE_TAG 前缀)。
        fn load_summary(proposal_id: u64) -> Result<LawProposalSummary<T>, DispatchError> {
            let raw = votingengine::Pallet::<T>::get_proposal_data(proposal_id)
                .ok_or(Error::<T>::ProposalPayloadInvalid)?;
            let tag = MODULE_TAG;
            if raw.len() < tag.len() || &raw[..tag.len()] != tag {
                return Err(Error::<T>::ProposalPayloadInvalid.into());
            }
            LawProposalSummary::<T>::decode(&mut &raw[tag.len()..])
                .map_err(|_| Error::<T>::ProposalPayloadInvalid.into())
        }

        /// 从 votingengine ProposalObject 读回并解码法律全文(章>节>条>款)。
        fn load_chapters(proposal_id: u64) -> Result<ChaptersOf<T>, DispatchError> {
            let raw = votingengine::Pallet::<T>::get_proposal_object(proposal_id)
                .ok_or(Error::<T>::ProposalPayloadInvalid)?;
            ChaptersOf::<T>::decode(&mut &raw[..])
                .map_err(|_| Error::<T>::ProposalPayloadInvalid.into())
        }

        /// 把通过的提案写入法律存储(立法新增 / 修法升版 / 废法置废)。
        pub fn write_law_version(
            proposal_id: u64,
            summary: LawProposalSummary<T>,
            chapters: ChaptersOf<T>,
            now: u64,
        ) -> DispatchResult {
            Self::ensure_write_law_version_allowed(&summary, &chapters)?;
            match summary.action {
                LawAction::Enact => {
                    let law_id = NextLawId::<T>::mutate(|n| {
                        let id = *n;
                        *n = n.saturating_add(1);
                        id
                    });
                    let version = 1u32;
                    let lv = LawVersion::<T> {
                        law_id,
                        version,
                        title: summary.title,
                        title_en: summary.title_en,
                        chapters,
                        content_hash: summary.content_hash,
                        vote_type: summary.vote_type,
                        proposal_id,
                        published_at: now,
                        effective_at: summary.effective_at,
                    };
                    LawVersions::<T>::insert(law_id, version, lv);
                    let law = Law {
                        law_id,
                        tier: summary.tier,
                        scope_code: summary.scope_code,
                        houses: summary.houses,
                        effective_version: None,
                        latest_version: version,
                        pending_version: Some(version),
                        status: LawStatus::Pending,
                    };
                    Laws::<T>::insert(law_id, law);
                    LawsByScope::<T>::try_mutate(summary.tier, summary.scope_code, |v| {
                        v.try_push(law_id)
                    })
                    .map_err(|_| Error::<T>::TooManyLawsInScope)?;
                    Self::deposit_event(Event::<T>::LawEnacted { law_id, version });
                    Self::activate_or_schedule(law_id, version, summary.effective_at)?;
                }
                LawAction::Amend => {
                    let mut law = Laws::<T>::get(summary.law_id).ok_or(Error::<T>::LawNotFound)?;
                    let version = law.latest_version.saturating_add(1);
                    // 核心修宪(第一章总则核心条款)落永久公投凭据 —— 须在 chapters 被移动前算。
                    Self::record_constitution_amendment_proof(
                        summary.tier,
                        summary.law_id,
                        law.effective_version,
                        version,
                        &chapters,
                        proposal_id,
                    )?;
                    // 一切修宪(任意章)落永久护宪大法官终审凭据(第21条)。
                    Self::record_constitution_guard_proof(summary.tier, version, proposal_id)?;
                    let lv = LawVersion::<T> {
                        law_id: summary.law_id,
                        version,
                        title: summary.title,
                        title_en: summary.title_en,
                        chapters,
                        content_hash: summary.content_hash,
                        vote_type: summary.vote_type,
                        proposal_id,
                        published_at: now,
                        effective_at: summary.effective_at,
                    };
                    LawVersions::<T>::insert(summary.law_id, version, lv);
                    law.latest_version = version;
                    law.pending_version = Some(version);
                    law.status = LawStatus::Pending;
                    Laws::<T>::insert(summary.law_id, law);
                    Self::deposit_event(Event::<T>::LawAmended {
                        law_id: summary.law_id,
                        version,
                    });
                    Self::activate_or_schedule(summary.law_id, version, summary.effective_at)?;
                }
                LawAction::Repeal => {
                    Laws::<T>::mutate(summary.law_id, |maybe| {
                        if let Some(law) = maybe {
                            law.status = LawStatus::Repealed;
                        }
                    });
                    Self::deposit_event(Event::<T>::LawRepealed {
                        law_id: summary.law_id,
                    });
                }
            }
            Ok(())
        }

        // ───────── 查询(供 runtime API 调用)─────────
        /// 读取法律主体。
        pub fn law(law_id: u64) -> Option<Law> {
            Laws::<T>::get(law_id)
        }

        /// 读取法律指定版本。
        pub fn law_version(law_id: u64, version: u32) -> Option<LawVersion<T>> {
            LawVersions::<T>::get(law_id, version)
        }

        /// 读取法律版本展示标签。
        pub fn law_version_label(law_id: u64, version: u32) -> Option<LawVersionLabel<T>> {
            LawVersionLabels::<T>::get(law_id, version)
        }

        /// 列出某层级 + 行政区下的法律 ID。
        pub fn list_laws(tier: Tier, scope_code: u32) -> sp_runtime::sp_std::vec::Vec<u64> {
            LawsByScope::<T>::get(tier, scope_code).into_inner()
        }
    }
}

/// 立法投票终态回调接入:投票引擎在立法提案达终态时按 kind 广播到此,
/// 由本业务壳认领并写入法律(runtime 装配 `votingengine::Config::LegislationVoteResultCallback = LegislationYuan`)。
impl<T: pallet::Config> votingengine::LegislationVoteResultCallback for pallet::Pallet<T> {
    fn on_legislation_vote_finalized(
        vote_proposal_id: u64,
        approved: bool,
    ) -> Result<votingengine::ProposalExecutionOutcome, sp_runtime::DispatchError> {
        pallet::Pallet::<T>::apply_legislation_vote_result(vote_proposal_id, approved)
    }
}

#[cfg(test)]
mod tests;
