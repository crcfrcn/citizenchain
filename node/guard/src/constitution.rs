use codec::Decode;
use codec::Encode;
use primitives::count_const::IMMUTABLE_CONSTITUTION_ARTICLES;
use runtime::opaque::Block;
use sc_client_api::backend::Backend as _;
use sc_client_api::backend::TrieCacheContext;
use sc_client_api::StorageProvider;

use sc_consensus::ImportedState;
use sc_consensus::StateAction;
use sc_consensus::StorageChanges;
use sp_api::ApiExt;
use sp_api::Core;
use sp_api::ProvideRuntimeApi;
use sp_blockchain::HeaderBackend;

use sp_runtime::traits::Block as BlockT;
use sp_runtime::traits::Header as HeaderT;

use std::collections::BTreeMap;
use std::collections::BTreeSet;

/// 宪法在立法院模块固定为 `law_id = 0`(创世注入)。
pub const CONSTITUTION_LAW_ID: u64 = 0;

/// 创世注入的宪法版本号,作为不可修改条款的内容基准来源。
const GENESIS_CONSTITUTION_VERSION: u32 = 1;

/// Law 元数据判别值(SCALE 变体索引,与 `legislation::types` 声明序一致:
/// `Tier{Constitution=0,..}`、`LawStatus{Pending=0,Effective=1,Repealed=2}`)。
/// 由 legislation 测试 `enum_discriminants_match_node_guard` 交叉钉死,防漂移。
const TIER_CONSTITUTION: u8 = 0;

const LAW_STATUS_PENDING: u8 = 0;

const LAW_STATUS_EFFECTIVE: u8 = 1;

const LAW_STATUS_REPEALED: u8 = 2;

/// 表决类型「特别案」的 wire 值(`legislation::VoteType::Special.as_u8()`)。
/// 由 legislation 测试 `enum_discriminants_match_node_guard` 交叉钉死,防漂移。
/// 用途:核心章(第一章总则)条款改动必须记录为特别案(宪法第十九条 node 背书)。
const LAW_VOTE_TYPE_SPECIAL: u8 = 4;

// ───────── 链上结构镜像 ─────────
// 字段序必须与 legislation 链端 `LawVersion / Chapter / Section / Article / Clause` 一致。
// SCALE 按声明序顺序解码,解到所需字段即停 —— 尾部字段无需镜像。Encode 仅用于不可修改条款的
// 规范字节比对(同一逻辑内容 → 同一字节)。

#[derive(Encode, Decode, PartialEq)]
#[allow(dead_code)] // 款号 number 不参与渲染/比对(text 已含「第N款」前缀),仅占位保持字段序。
struct MClause {
    number: u32,
    text: Vec<u8>,
    text_en: Option<Vec<u8>>,
}

#[derive(Encode, Decode, PartialEq)]
struct MArticle {
    number: u32,
    title: Vec<u8>,
    title_en: Option<Vec<u8>>,
    body: Vec<u8>,
    body_en: Option<Vec<u8>>,
    clauses: Vec<MClause>,
}

#[derive(Encode, Decode, PartialEq)]
struct MSection {
    number: u32,
    title: Vec<u8>,
    title_en: Option<Vec<u8>>,
    articles: Vec<MArticle>,
}

#[derive(Encode, Decode, PartialEq)]
struct MChapter {
    number: u32,
    title: Vec<u8>,
    title_en: Option<Vec<u8>>,
    sections: Vec<MSection>,
}

/// 只解码 `LawVersion` 前缀(law_id..vote_type);其后字段(proposal_id..effective_at)顺序解码到
/// vote_type 即停。chapters 用于条文比对,vote_type 用于核心章档位背书(第十九条)。
#[derive(Decode, Encode)]
#[allow(dead_code)] // law_id/version/title/title_en/content_hash 仅占位保持字段序。
pub(crate) struct MLawVersionHead {
    law_id: u64,
    version: u32,
    title: Vec<u8>,
    title_en: Option<Vec<u8>>,
    chapters: Vec<MChapter>,
    content_hash: [u8; 32],
    vote_type: u8,
}

/// 链上 `LawVersionLabel` 镜像。版本号排序仍来自 `LawVersion.version`,本结构只负责展示名。
#[derive(Decode)]
struct MLawVersionLabel {
    title: Vec<u8>,
    title_en: Option<Vec<u8>>,
}

/// 解码 `Law`(到 status 即停)。houses = `Vec<CidNumber>`,机构账户不参与身份表达；
/// tier/status 为枚举变体索引(u8)。守卫据此校验宪法元数据不变式。
#[derive(Decode)]
#[allow(dead_code)] // law_id 占位保持字段序。
pub(crate) struct MLawHead {
    law_id: u64,
    tier: u8,
    scope_code: u32,
    houses: Vec<Vec<u8>>,
    effective_version: Option<u32>,
    latest_version: u32,
    pending_version: Option<u32>,
    status: u8,
}

/// 解码宪法 `Law` 记录(失败 → `LawDecodeFailed`)。
fn decode_law_head(law_scale: &[u8]) -> Result<MLawHead, GuardError> {
    crate::constitution::MLawHead::decode(&mut &law_scale[..])
        .map_err(|_| crate::constitution::GuardError::LawDecodeFailed)
}

/// 在 章>节>条 嵌套结构里按条号查找条文。
fn find_article(chapters: &[MChapter], number: u32) -> Option<&MArticle> {
    chapters
        .iter()
        .flat_map(|c| c.sections.iter())
        .flat_map(|s| s.articles.iter())
        .find(|a| a.number == number)
}

// ═════════════════════════════════════════════════════════════════════════
// 二、不可修改条款守卫(L2 共识层)
// ═════════════════════════════════════════════════════════════════════════

/// 立法院模块在 `construct_runtime` 中的 pallet 名(twox128 前缀据此推导)。
/// 硬编码,绝不读链上 metadata —— metadata 属可升级 runtime,会被恶意升级伪造。
const PALLET_NAME: &[u8] = b"LegislationYuan";

/// 链上不可修改条款 manifest 镜像(与 `legislation::ImmutableManifest` 字段序一致)。
#[derive(Decode, Encode)]
struct MImmutableManifest {
    article_numbers: Vec<u32>,
    article_hashes: Vec<[u8; 32]>,
}

fn decode_full_exact<T: Decode>(bytes: &[u8]) -> Result<T, ()> {
    let mut input = bytes;
    let value = T::decode(&mut input).map_err(|_| ())?;
    if !input.is_empty() {
        return Err(());
    }
    Ok(value)
}

/// 启动期交叉校验:创世 manifest 的清单必须 == 节点二进制单源,且逐条摘要 == 基准条文摘要。
/// 任一不符 → 返回 `Err`(节点应拒绝启动)。纯函数,便于单测。
fn verify_manifest(manifest_bytes: &[u8], reference: &ImmutableReference) -> Result<(), String> {
    let manifest = crate::constitution::decode_full_exact::<MImmutableManifest>(manifest_bytes)
        .map_err(|_| "manifest 解码失败或存在尾随字节".to_string())?;

    // 1. 清单一致(双锚:创世 manifest ↔ 节点二进制常量)。
    let mut on_chain = manifest.article_numbers.clone();
    on_chain.sort_unstable();
    let mut binary: Vec<u32> = IMMUTABLE_CONSTITUTION_ARTICLES.to_vec();
    binary.sort_unstable();
    if on_chain != binary {
        return Err(format!(
            "创世 manifest 清单 {on_chain:?} 与节点二进制 {binary:?} 不一致"
        ));
    }
    if manifest.article_hashes.len() != manifest.article_numbers.len() {
        return Err("创世 manifest 条号与摘要数量不一致".to_string());
    }

    // 2. 逐条摘要 == 节点从 block#0 派生的条文摘要(防 manifest 谎报)。
    for (number, reference_bytes) in reference.articles.iter() {
        let idx = manifest
            .article_numbers
            .iter()
            .position(|x| x == number)
            .ok_or_else(|| format!("manifest 缺第 {number} 条"))?;
        let want = sp_crypto_hashing::blake2_256(reference_bytes);
        if manifest.article_hashes.get(idx) != Some(&want) {
            return Err(format!("manifest 第 {number} 条摘要与创世条文不符"));
        }
    }
    Ok(())
}

/// 从目标状态读取并复核 manifest。除语义三方一致外，编码也必须与 block#0 基准逐字一致。
fn verify_manifest_from_reader<F>(
    read_raw: &F,
    reference: &ImmutableReference,
) -> Result<(), String>
where
    F: Fn(&[u8]) -> Option<Vec<u8>>,
{
    let current = read_raw(&crate::constitution::storage_key::manifest())
        .ok_or_else(|| "目标状态缺不可修改条款 manifest".to_string())?;
    verify_manifest(&current, reference)?;
    if current != reference.manifest {
        return Err("目标状态 manifest 与 block#0 基准编码不一致".to_string());
    }
    Ok(())
}

/// 不可修改条款守卫的判定失败原因(全部一律拒块/拒启,fail-safe 方向恒为「拒绝」)。
#[derive(Debug, PartialEq)]
pub(crate) enum GuardError {
    /// 宪法版本或永久凭据 RAW key 的 Blake2_128Concat/长度/尾部编码不合法。
    StorageKeyMalformed,
    /// 宪法 `Law(0)` 在目标状态缺失(存储被改名/删除)。
    ConstitutionLawMissing,
    /// `Law` 解码失败。
    LawDecodeFailed,
    /// `Laws[0]` 值内部的 `law_id` 不再为 0。
    ConstitutionLawIdChanged,
    /// 当前版本 `LawVersion` 缺失。
    LawVersionMissing,
    /// `LawVersion` 解码失败。
    VersionDecodeFailed,
    /// `LawVersion[0][version]` 值内部 `law_id` 不再为 0。
    VersionLawIdChanged(u32),
    /// `LawVersion[0][version]` 值内部版本号与 RAW key 不一致。
    VersionNumberChanged { expected: u32, found: u32 },
    /// `LawVersion.content_hash` 与章节规范 SCALE 哈希不一致。
    VersionContentHashChanged(u32),
    /// 存在不在 `1..=latest_version` 声明范围内的隐藏宪法版本。
    VersionOutsideDeclaredRange(u32),
    /// 同一版本内出现重复章号。
    DuplicateChapterNumber(u32),
    /// 同一章内出现重复节号；不同章允许使用相同节号。
    DuplicateSectionNumber { chapter: u32, section: u32 },
    /// 同一版本内出现重复条号，可能隐藏第二份恶意条文。
    DuplicateArticleNumber(u32),
    /// 某不可修改条款在状态中缺失(被删/改号)。
    ImmutableArticleMissing(u32),
    /// 基准中缺该不可修改条款(创世派生异常)。
    ReferenceMissing(u32),
    /// 某不可修改条款内容被改动(与创世基准不一致)。
    ImmutableArticleMutated(u32),
    /// 宪法 `tier` 被改(不再是 Constitution)。
    ConstitutionTierChanged,
    /// 宪法 `scope_code` 被改(不再是全国 0)。
    ConstitutionScopeChanged,
    /// 宪法被置为 Repealed(违反不可废止)。
    ConstitutionRepealed,
    /// 宪法状态不是 Pending/Effective，或版本指针组合不符合状态机。
    ConstitutionVersionStateInvalid,
    /// 宪法可修订机构(`houses`)被改(与创世不一致)。
    ConstitutionHousesChanged,
    /// 宪法层级唯一性被破坏(`LawsByScope[宪法][0]` 不再恰为 `[0]`:多出第二部宪法或 law_id=0 被隐藏)。
    ConstitutionNotUnique,
    /// manifest 缺失、被改写或与二进制/创世条文不一致。
    ConstitutionManifestChanged,
    /// 核心章(第一章总则,创世口径)某条被修改/删除/移出核心章,但该版本未记录为特别案表决
    /// (违反宪法第十九条:核心章条款修改须走特别案 + 强制公投)。参数为条号。
    CoreClauseNotSpecial(u32),
    /// 核心章条款有改动的版本缺失强制公投凭据 `ConstitutionAmendmentProof[version]`
    /// (第十九条:核心修宪须经公投)。参数为版本号。
    CoreClauseReferendumMissing(u32),
    /// 核心章条款有改动的版本,其公投凭据未达通过口径(≥70% 参与 + ≥70% 赞成)。参数为版本号。
    CoreClauseReferendumNotPassed(u32),
    /// 修宪版本(v>创世)缺失护宪大法官终审凭据 `ConstitutionGuardVoteProof[version]`
    /// (第21条:一切修宪须经护宪终审)。参数为版本号。
    GuardReviewMissing(u32),
    /// 修宪版本的护宪终审凭据未达通过口径(4 名及以上赞成)。参数为版本号。
    GuardReviewNotPassed(u32),
}

/// 不可修改基准(创世/block#0):
/// - `articles`:不可修改条款(第 1/2/3/17/19/24/34/42 条)的规范 SCALE 字节;
/// - `core_articles`:核心章(第一章总则)**非禁改**条款的创世规范字节(条号→字节);
/// - `houses`:宪法可修订机构。
///
/// `core_articles` 用于第十九条章→档位的 node 背书:任一核心条款相对创世被
/// 修改/删除/移出核心章,则承载它的版本必须记录为特别案表决(见 [`check_core_chapter_tier`])。
pub(crate) struct ImmutableReference {
    articles: BTreeMap<u32, Vec<u8>>,
    core_articles: BTreeMap<u32, Vec<u8>>,
    houses: Vec<Vec<u8>>,
    manifest: Vec<u8>,
}

impl ImmutableReference {
    /// 从一个 RAW 存储读取闭包(应指向 block#0 创世状态)派生基准:
    /// 读 `Laws[0]` 取 `houses`,读创世版本 `LawVersions[0][1]` 取不可修改条款与核心章条款规范字节。
    /// 任一缺失/不合法 → 返回错误(创世不合法,调用方应拒绝启动)。
    pub fn from_raw_reader<F>(read_raw: F) -> Result<Self, GuardError>
    where
        F: Fn(&[u8]) -> Option<Vec<u8>>,
    {
        let law_bytes = read_raw(&crate::constitution::storage_key::law(CONSTITUTION_LAW_ID))
            .ok_or(crate::constitution::GuardError::ConstitutionLawMissing)?;
        let law = decode_law_head(&law_bytes)?;
        validate_law_identity_and_state(&law)?;

        let version_bytes = read_raw(&crate::constitution::storage_key::law_version(
            CONSTITUTION_LAW_ID,
            GENESIS_CONSTITUTION_VERSION,
        ))
        .ok_or(crate::constitution::GuardError::LawVersionMissing)?;
        let head = crate::constitution::MLawVersionHead::decode(&mut &version_bytes[..])
            .map_err(|_| crate::constitution::GuardError::VersionDecodeFailed)?;
        validate_version_identity_and_structure(&head, GENESIS_CONSTITUTION_VERSION)?;

        // 不可修改条款基准:逐字冻结。
        let mut articles = BTreeMap::new();
        for &n in IMMUTABLE_CONSTITUTION_ARTICLES.iter() {
            let article = find_article(&head.chapters, n)
                .ok_or(crate::constitution::GuardError::ImmutableArticleMissing(n))?;
            articles.insert(n, article.encode());
        }
        // 核心章基准:创世第一章总则里的**非禁改**条款(可经特别案修订,故存字节供 diff)。
        let mut core_articles = BTreeMap::new();
        if let Some(core_chapter) = head.chapters.first() {
            for article in core_chapter.sections.iter().flat_map(|s| s.articles.iter()) {
                if !IMMUTABLE_CONSTITUTION_ARTICLES.contains(&article.number) {
                    core_articles.insert(article.number, article.encode());
                }
            }
        }
        let manifest = read_raw(&crate::constitution::storage_key::manifest())
            .ok_or(crate::constitution::GuardError::ConstitutionManifestChanged)?;
        let reference = Self {
            articles,
            core_articles,
            houses: law.houses,
            manifest,
        };
        verify_manifest(&reference.manifest, &reference)
            .map_err(|_| crate::constitution::GuardError::ConstitutionManifestChanged)?;
        Ok(reference)
    }
}

/// 校验 `Law` 自描述身份与版本状态机。节点只承认固定 law_id=0 的单一宪法记录。
fn validate_law_identity_and_state(law: &MLawHead) -> Result<(), GuardError> {
    if law.law_id != CONSTITUTION_LAW_ID {
        return Err(crate::constitution::GuardError::ConstitutionLawIdChanged);
    }
    if law.tier != TIER_CONSTITUTION {
        return Err(crate::constitution::GuardError::ConstitutionTierChanged);
    }
    if law.scope_code != 0 {
        return Err(crate::constitution::GuardError::ConstitutionScopeChanged);
    }
    if law.status == LAW_STATUS_REPEALED {
        return Err(crate::constitution::GuardError::ConstitutionRepealed);
    }
    let pointers_valid = match law.status {
        LAW_STATUS_PENDING => matches!(
            (law.effective_version, law.pending_version),
            (Some(effective), Some(pending))
                if effective < pending && pending == law.latest_version
        ),
        LAW_STATUS_EFFECTIVE => {
            law.effective_version == Some(law.latest_version) && law.pending_version.is_none()
        }
        _ => false,
    };
    if law.latest_version < GENESIS_CONSTITUTION_VERSION || !pointers_valid {
        return Err(crate::constitution::GuardError::ConstitutionVersionStateInvalid);
    }
    Ok(())
}

/// 校验版本值与 RAW key 的身份一致性、全文哈希和章/节/条三层编号唯一性。
fn validate_version_identity_and_structure(
    head: &MLawVersionHead,
    expected_version: u32,
) -> Result<(), GuardError> {
    if head.law_id != CONSTITUTION_LAW_ID {
        return Err(crate::constitution::GuardError::VersionLawIdChanged(
            expected_version,
        ));
    }
    if head.version != expected_version {
        return Err(crate::constitution::GuardError::VersionNumberChanged {
            expected: expected_version,
            found: head.version,
        });
    }
    if head.content_hash != sp_crypto_hashing::blake2_256(&head.chapters.encode()) {
        return Err(crate::constitution::GuardError::VersionContentHashChanged(
            expected_version,
        ));
    }
    let mut chapter_numbers = BTreeSet::new();
    let mut article_numbers = BTreeSet::new();
    for chapter in &head.chapters {
        if !chapter_numbers.insert(chapter.number) {
            return Err(crate::constitution::GuardError::DuplicateChapterNumber(
                chapter.number,
            ));
        }
        let mut section_numbers = BTreeSet::new();
        for section in &chapter.sections {
            if !section_numbers.insert(section.number) {
                return Err(crate::constitution::GuardError::DuplicateSectionNumber {
                    chapter: chapter.number,
                    section: section.number,
                });
            }
            for article in &section.articles {
                if !article_numbers.insert(article.number) {
                    return Err(crate::constitution::GuardError::DuplicateArticleNumber(
                        article.number,
                    ));
                }
            }
        }
    }
    Ok(())
}

/// 拒绝 delta 或完整下载态中超出 `latest_version` 的隐藏宪法版本。
fn check_version_key_range<'a, I>(keys: I, latest_version: u32) -> Result<(), GuardError>
where
    I: IntoIterator<Item = &'a Vec<u8>>,
{
    for key in keys {
        if crate::constitution::storage_key::is_constitution_version_key_candidate(key)
            && crate::constitution::storage_key::constitution_version_from_key(key).is_none()
        {
            return Err(crate::constitution::GuardError::StorageKeyMalformed);
        }
        if crate::constitution::storage_key::is_constitution_proof_key_candidate(key)
            && crate::constitution::storage_key::constitution_proof_version_from_key(key).is_none()
        {
            return Err(crate::constitution::GuardError::StorageKeyMalformed);
        }
        if let Some(version) = crate::constitution::storage_key::constitution_version_from_key(key)
        {
            if !(GENESIS_CONSTITUTION_VERSION..=latest_version).contains(&version) {
                return Err(crate::constitution::GuardError::VersionOutsideDeclaredRange(version));
            }
        }
        if let Some(version) =
            crate::constitution::storage_key::constitution_proof_version_from_key(key)
        {
            if !(GENESIS_CONSTITUTION_VERSION..=latest_version).contains(&version) {
                return Err(crate::constitution::GuardError::VersionOutsideDeclaredRange(version));
            }
        }
    }
    Ok(())
}

/// 冻结父状态中已经存在的历史宪法版本和修宪凭据。
///
/// 合法修宪可以在同一区块新增下一个版本及其凭据；记录一旦出现在父状态，后续区块只能原样
/// 保留，不能修改、删除，也不能为已经存在的历史版本事后补写凭据。这样“历史记录合法”与
/// “历史记录未被篡改”是两层独立检查，恶意 runtime 无法用另一份仍过阈值的数据替换旧记录。
fn check_frozen_constitution_records<F>(
    delta: &BTreeMap<Vec<u8>, Option<Vec<u8>>>,
    read_parent: F,
) -> Result<(), String>
where
    F: Fn(&[u8]) -> Result<Option<Vec<u8>>, String>,
{
    for (key, after) in delta {
        if crate::constitution::storage_key::is_constitution_version_key_candidate(key) {
            let version = crate::constitution::storage_key::constitution_version_from_key(key)
                .ok_or_else(|| "宪法历史版本 RAW key 非法".to_string())?;
            if let Some(before) = read_parent(key)? {
                if after.as_deref() != Some(before.as_slice()) {
                    return Err(format!("宪法历史版本 {version} 已被冻结，不得修改或删除"));
                }
            }
            continue;
        }

        if crate::constitution::storage_key::is_constitution_proof_key_candidate(key) {
            let version =
                crate::constitution::storage_key::constitution_proof_version_from_key(key)
                    .ok_or_else(|| "修宪凭据 RAW key 非法".to_string())?;
            match read_parent(key)? {
                Some(before) if after.as_deref() != Some(before.as_slice()) => {
                    return Err(format!("修宪凭据 {version} 已被冻结，不得修改或删除"));
                }
                None if after.is_some() => {
                    let historical_version =
                        crate::constitution::storage_key::law_version(CONSTITUTION_LAW_ID, version);
                    if read_parent(&historical_version)?.is_some() {
                        return Err(format!(
                            "宪法历史版本 {version} 已存在，不得事后补写修宪凭据"
                        ));
                    }
                }
                _ => {}
            }
        }
    }
    Ok(())
}

/// 启动和完整状态导入必须携带连续的 `1..=latest_version` 版本集合。
/// 只遍历真实存在的 key，不按不可信 `latest_version` 做超大范围循环，避免恶意状态制造 CPU DoS。
fn declared_constitution_versions<'a, I>(
    keys: I,
    latest_version: u32,
) -> Result<BTreeSet<u32>, GuardError>
where
    I: IntoIterator<Item = &'a Vec<u8>>,
{
    let versions: BTreeSet<u32> = keys
        .into_iter()
        .filter_map(|key| crate::constitution::storage_key::constitution_version_from_key(key))
        .collect();
    if versions.len() as u64 != u64::from(latest_version)
        || versions.first().copied() != Some(GENESIS_CONSTITUTION_VERSION)
        || versions.last().copied() != Some(latest_version)
        || versions
            .iter()
            .copied()
            .scan(0u32, |previous, version| {
                let continuous = version == previous.saturating_add(1);
                *previous = version;
                Some(continuous)
            })
            .any(|continuous| !continuous)
    {
        return Err(crate::constitution::GuardError::LawVersionMissing);
    }
    Ok(versions)
}

/// 纯判定:给定一个指向**目标区块后置状态**的 RAW 读取闭包,校验宪法全部不变式:
/// ① Law 元数据(tier=Constitution、scope=0、status≠Repealed、houses=创世);
/// ② 层级唯一性(`LawsByScope[宪法][0] == [0]`);③ 不可修改条款逐字 == 创世基准;
/// ④ 核心章(第一章总则)条款改动须记录为特别案表决(第十九条,见 [`check_core_chapter_tier`])
///    且挂一份过公投口径的永久凭据(见 [`check_core_referendum_proof`]);
/// ⑤ 一切修宪版本(v>创世)须挂一份过 4/7 口径的护宪大法官终审凭据(第21条,见 [`check_guard_review_proof`])。
/// 任一缺失/解码失败/不一致 → 返回 `Err`(拒块)。
pub(crate) fn check_immutable_articles<F>(
    read_raw: F,
    reference: &ImmutableReference,
) -> Result<(), GuardError>
where
    F: Fn(&[u8]) -> Option<Vec<u8>>,
{
    // ── ① Law 元数据不变式 ──
    let law_bytes = read_raw(&crate::constitution::storage_key::law(CONSTITUTION_LAW_ID))
        .ok_or(crate::constitution::GuardError::ConstitutionLawMissing)?;
    let law = decode_law_head(&law_bytes)?;
    validate_law_identity_and_state(&law)?;
    if law.houses != reference.houses {
        return Err(crate::constitution::GuardError::ConstitutionHousesChanged);
    }

    // manifest 不只是启动提示；任何正常块、runtime 升级或完整状态导入都不得改写它。
    verify_manifest_from_reader(&read_raw, reference)
        .map_err(|_| crate::constitution::GuardError::ConstitutionManifestChanged)?;

    // ── ② 层级唯一性:LawsByScope[宪法][0] 必须恰为 [0] ──
    let scope_bytes = read_raw(&crate::constitution::storage_key::laws_by_scope_constitution())
        .ok_or(crate::constitution::GuardError::ConstitutionNotUnique)?;
    let scope_list = crate::constitution::decode_full_exact::<Vec<u64>>(&scope_bytes)
        .map_err(|_| crate::constitution::GuardError::ConstitutionNotUnique)?;
    if scope_list != [CONSTITUTION_LAW_ID] {
        return Err(crate::constitution::GuardError::ConstitutionNotUnique);
    }

    // ── ③ 当前有效状态逐一复核 ──
    // 历史版本在普通块中按 delta 精确复核；启动和 warp 则枚举真实 key 做全历史复核。
    let effective_version = law
        .effective_version
        .ok_or(crate::constitution::GuardError::LawVersionMissing)?;
    check_immutable_version(&read_raw, reference, effective_version)?;
    if let Some(pending_version) = law.pending_version {
        check_immutable_version(&read_raw, reference, pending_version)?;
    }
    Ok(())
}

/// 校验指定宪法版本里的不可修改条款是否仍与创世基准逐字一致。
fn check_immutable_version<F>(
    read_raw: &F,
    reference: &ImmutableReference,
    version: u32,
) -> Result<(), GuardError>
where
    F: Fn(&[u8]) -> Option<Vec<u8>>,
{
    let version_bytes = read_raw(&crate::constitution::storage_key::law_version(
        CONSTITUTION_LAW_ID,
        version,
    ))
    .ok_or(crate::constitution::GuardError::LawVersionMissing)?;
    let head = crate::constitution::MLawVersionHead::decode(&mut &version_bytes[..])
        .map_err(|_| crate::constitution::GuardError::VersionDecodeFailed)?;
    validate_version_identity_and_structure(&head, version)?;
    for &n in IMMUTABLE_CONSTITUTION_ARTICLES.iter() {
        let current = find_article(&head.chapters, n)
            .ok_or(crate::constitution::GuardError::ImmutableArticleMissing(n))?;
        let baseline = reference
            .articles
            .get(&n)
            .ok_or(crate::constitution::GuardError::ReferenceMissing(n))?;
        if &current.encode() != baseline {
            return Err(crate::constitution::GuardError::ImmutableArticleMutated(n));
        }
    }
    // 核心章条款(第一章总则非禁改条)改动:① 必须记录为特别案;② 必须挂合格公投凭据(宪法第十九条 node 背书)。
    let core_changed = check_core_chapter_tier(&head, reference)?;
    if core_changed {
        check_core_referendum_proof(read_raw, version)?;
    }
    // 一切修宪版本(v > 创世版本)都须挂合格护宪大法官终审凭据(宪法第21条 node 背书,含一般章重要案)。
    if version > GENESIS_CONSTITUTION_VERSION {
        check_guard_review_proof(read_raw, version)?;
    }
    Ok(())
}

/// 修宪版本(生效/待生效,`version > 创世`)必须挂一份**通过口径**的永久护宪终审凭据
/// `ConstitutionGuardVoteProof[version]`(第21条 node 背书:一切修宪须经护宪大法官 4/7 终审,含一般章)。
/// 凭据是 legislation **永久存储**(不受 votebase 90 天清理影响),故可对任意修宪版本随时校验。
/// 口径复用 `primitives::constitution::guard_review_passed`(与链端结算共用单源)。
///
/// 信任上限：当前值只是 runtime 写入的赞成票计数，不含节点可独立验签的成员签名集合。
/// 本守卫能冻结凭据存在性、编码和阈值口径，但不能把恶意 runtime 伪造的计数转化为密码学证明。
fn check_guard_review_proof<F>(read_raw: &F, version: u32) -> Result<(), GuardError>
where
    F: Fn(&[u8]) -> Option<Vec<u8>>,
{
    let bytes = read_raw(&crate::constitution::storage_key::constitution_guard_vote_proof(version))
        .ok_or(crate::constitution::GuardError::GuardReviewMissing(version))?;
    let approve = crate::constitution::decode_full_exact::<u32>(&bytes)
        .map_err(|_| crate::constitution::GuardError::GuardReviewMissing(version))?;
    if !primitives::constitution::guard_review_passed(approve) {
        return Err(crate::constitution::GuardError::GuardReviewNotPassed(
            version,
        ));
    }
    Ok(())
}

/// 核心章(第一章总则,创世口径)条款的档位背书(宪法第十九条 node 侧强制)。
/// 对每个创世核心条款:若其相对创世基准被**修改/删除/移出核心章**,则本版本必须记录为
/// 特别案表决（`vote_type == LAW_VOTE_TYPE_SPECIAL`），否则拒块；核心条款未变则不约束档位。
/// (一般章条款可走重要案)。仅盯创世核心集且按条号定位,故不受章节重排影响。
///
/// 返回 `true` 表示本版本相对创世核心基准**有改动**(调用方据此再校验公投凭据)。
/// 本函数盯的是版本**记录的** `vote_type`(使 setCode 无法静默降级为重要案);
/// 「公投是否真的通过」由 [`check_core_referendum_proof`] 读永久凭据补上。
fn check_core_chapter_tier(
    head: &MLawVersionHead,
    reference: &ImmutableReference,
) -> Result<bool, GuardError> {
    // 当前版本核心章(chapters[0])的条号集,用于判定核心条款是否被移出核心章。
    let core_chapter_now: Vec<u32> = head
        .chapters
        .first()
        .into_iter()
        .flat_map(|c| c.sections.iter())
        .flat_map(|s| s.articles.iter())
        .map(|a| a.number)
        .collect();
    let mut core_changed = false;
    for (&n, baseline) in reference.core_articles.iter() {
        let current = find_article(&head.chapters, n);
        let content_same = current.map(|a| a.encode()).as_deref() == Some(baseline.as_slice());
        let in_core_chapter = core_chapter_now.contains(&n);
        // 修改(内容变)/删除(找不到)/移出核心章(不在 chapters[0])任一 → 有改动,且须特别案。
        if !content_same || !in_core_chapter {
            core_changed = true;
            if head.vote_type != LAW_VOTE_TYPE_SPECIAL {
                return Err(crate::constitution::GuardError::CoreClauseNotSpecial(n));
            }
        }
    }
    Ok(core_changed)
}

/// 核心章有改动的版本必须挂一份**通过口径**的强制公投凭据 `ConstitutionAmendmentProof[version]`
/// (第十九条 node 背书的公投凭据层:不止记录 `vote_type=Special`,还须有过公投口径的计票)。
/// 凭据是 legislation **永久存储**(不受 votebase 90 天清理影响),故可对生效/待生效版本随时校验,
/// 无转移块检测。口径复用 `primitives::constitution::referendum_passed`(与链端结算共用单源)。
/// 当前三元组不携带公民签名或人口快照证明，节点只背书记录与阈值口径的一致性，不宣称独立验票。
fn check_core_referendum_proof<F>(read_raw: &F, version: u32) -> Result<(), GuardError>
where
    F: Fn(&[u8]) -> Option<Vec<u8>>,
{
    let bytes = read_raw(&crate::constitution::storage_key::constitution_amendment_proof(version))
        .ok_or(crate::constitution::GuardError::CoreClauseReferendumMissing(version))?;
    let (eligible, yes, no) = crate::constitution::decode_full_exact::<(u64, u64, u64)>(&bytes)
        .map_err(|_| crate::constitution::GuardError::CoreClauseReferendumMissing(version))?;
    if !primitives::constitution::referendum_passed(eligible, yes, no) {
        return Err(crate::constitution::GuardError::CoreClauseReferendumNotPassed(version));
    }
    Ok(())
}

/// 是否必须跑完整宪法不变式校验。普通块只要触及立法院存储或 `:code` runtime 升级,
/// 就不能走快路径;其余块按归纳假设跳过。
fn needs_full_invariant_check(delta: &BTreeMap<Vec<u8>, Option<Vec<u8>>>) -> bool {
    let prefix = crate::constitution::storage_key::pallet_prefix();
    delta.keys().any(|k| k.starts_with(&prefix))
        || delta.contains_key(sp_storage::well_known_keys::CODE)
}

/// 从 warp/状态导入的完整下载态中抽出立法院模块键,在提交前执行同一套宪法不变式校验。
fn check_imported_state_key_values<'a, I>(
    pairs: I,
    reference: &ImmutableReference,
) -> Result<(), String>
where
    I: IntoIterator<Item = &'a (Vec<u8>, Vec<u8>)>,
{
    let prefix = crate::constitution::storage_key::pallet_prefix();
    let mut map: BTreeMap<Vec<u8>, Vec<u8>> = BTreeMap::new();
    for (key, value) in pairs {
        if key.starts_with(&prefix) {
            map.insert(key.clone(), value.clone());
        }
    }
    let read = |key: &[u8]| map.get(key).cloned();
    check_immutable_articles(read, reference).map_err(|e| format!("{e:?}"))?;
    let law_bytes =
        read(&crate::constitution::storage_key::law(CONSTITUTION_LAW_ID)).ok_or_else(|| {
            format!(
                "{:?}",
                crate::constitution::GuardError::ConstitutionLawMissing
            )
        })?;
    let law = decode_law_head(&law_bytes).map_err(|e| format!("{e:?}"))?;
    check_version_key_range(map.keys(), law.latest_version).map_err(|e| format!("{e:?}"))?;
    let versions = declared_constitution_versions(map.keys(), law.latest_version)
        .map_err(|e| format!("{e:?}"))?;
    for version in versions {
        check_immutable_version(&read, reference, version).map_err(|e| format!("{e:?}"))?;
    }
    Ok(())
}

/// 只从 SDK 定义的主状态抽出立法院模块键，子 trie 同名键不能覆盖宪法主状态。
fn check_imported_state_immutable(
    imported: &ImportedState<Block>,
    reference: &ImmutableReference,
) -> Result<(), String> {
    check_imported_state_key_values(
        imported
            .state
            .0
            .iter()
            .filter(|level| level.parent_storage_keys.is_empty() && level.state_root.is_empty())
            .flat_map(|level| level.key_values.iter()),
        reference,
    )
}

#[path = "render.rs"]
pub mod render;

/// 守卫推导出的宪法存储 RAW key(`Laws` / `LawVersions`),硬编码 hasher 与链端一致:
/// `Laws: StorageMap<Blake2_128Concat, u64, ..>`、
/// `LawVersions: StorageDoubleMap<Blake2_128Concat u64, Blake2_128Concat u32, ..>`。
pub mod storage_key {
    use crate::constitution::PALLET_NAME;
    use codec::Decode;
    use codec::Encode;
    use sp_crypto_hashing::blake2_128;
    use sp_crypto_hashing::twox_128;

    fn map_prefix(storage: &[u8]) -> Vec<u8> {
        let mut k = Vec::with_capacity(32);
        k.extend_from_slice(&twox_128(PALLET_NAME));
        k.extend_from_slice(&twox_128(storage));
        k
    }

    fn blake2_128_concat(encoded: &[u8]) -> Vec<u8> {
        let mut out = blake2_128(encoded).to_vec();
        out.extend_from_slice(encoded);
        out
    }

    /// `LegislationYuan::Laws[law_id]` 的完整存储 key。
    pub fn law(law_id: u64) -> Vec<u8> {
        let mut k = map_prefix(b"Laws");
        k.extend_from_slice(&blake2_128_concat(&law_id.encode()));
        k
    }

    /// `LegislationYuan::LawVersions[law_id][version]` 的完整存储 key。
    pub fn law_version(law_id: u64, version: u32) -> Vec<u8> {
        let mut k = map_prefix(b"LawVersions");
        k.extend_from_slice(&blake2_128_concat(&law_id.encode()));
        k.extend_from_slice(&blake2_128_concat(&version.encode()));
        k
    }

    /// `LegislationYuan::LawVersions[0]` 的 RAW key 前缀，用于识别未被 `latest_version` 声明的隐藏版本。
    pub fn constitution_versions_prefix() -> Vec<u8> {
        let mut k = map_prefix(b"LawVersions");
        k.extend_from_slice(&blake2_128_concat(
            &crate::constitution::CONSTITUTION_LAW_ID.encode(),
        ));
        k
    }

    pub fn is_constitution_version_key_candidate(key: &[u8]) -> bool {
        key.starts_with(&constitution_versions_prefix())
    }

    /// 从 `LawVersions[0][version]` 完整 RAW key 解出 version；非宪法版本键返回 `None`。
    pub fn constitution_version_from_key(key: &[u8]) -> Option<u32> {
        let prefix = constitution_versions_prefix();
        if !key.starts_with(&prefix) || key.len() != prefix.len() + 16 + 4 {
            return None;
        }
        let encoded = &key[prefix.len() + 16..];
        // Blake2_128Concat 的 hash 部分也属于共识 key 契约；只解尾部 u32 会把畸形 key
        // 误认成规范历史版本，必须先重算 hasher 再接受版本号。
        if blake2_128(encoded) != key[prefix.len()..prefix.len() + 16] {
            return None;
        }
        u32::decode(&mut &encoded[..]).ok()
    }

    fn version_from_single_map_key(key: &[u8], storage: &[u8]) -> Option<u32> {
        let prefix = map_prefix(storage);
        if !key.starts_with(&prefix) || key.len() != prefix.len() + 16 + 4 {
            return None;
        }
        let encoded = &key[prefix.len() + 16..];
        if blake2_128(encoded) != key[prefix.len()..prefix.len() + 16] {
            return None;
        }
        u32::decode(&mut &encoded[..]).ok()
    }

    /// 从两类永久修宪凭据 RAW key 解出版本号；非凭据键返回 `None`。
    pub fn constitution_proof_version_from_key(key: &[u8]) -> Option<u32> {
        version_from_single_map_key(key, b"ConstitutionAmendmentProof")
            .or_else(|| version_from_single_map_key(key, b"ConstitutionGuardVoteProof"))
    }

    pub fn is_constitution_proof_key_candidate(key: &[u8]) -> bool {
        key.starts_with(&map_prefix(b"ConstitutionAmendmentProof"))
            || key.starts_with(&map_prefix(b"ConstitutionGuardVoteProof"))
    }

    /// `LegislationYuan::LawVersionLabels[law_id][version]` 的完整存储 key。
    pub fn law_version_label(law_id: u64, version: u32) -> Vec<u8> {
        let mut k = map_prefix(b"LawVersionLabels");
        k.extend_from_slice(&blake2_128_concat(&law_id.encode()));
        k.extend_from_slice(&blake2_128_concat(&version.encode()));
        k
    }

    /// `LegislationYuan::ConstitutionImmutableManifest`(StorageValue,无 key hash)的完整 key。
    pub fn manifest() -> Vec<u8> {
        map_prefix(b"ConstitutionImmutableManifest")
    }

    /// `LegislationYuan::ConstitutionAmendmentProof[version]`(Blake2_128Concat)的完整 key。
    /// 核心修宪的永久公投凭据 `(eligible, yes, no)`,供守卫背书(第十九条,ADR-027 §6.3)。
    pub fn constitution_amendment_proof(version: u32) -> Vec<u8> {
        let mut k = map_prefix(b"ConstitutionAmendmentProof");
        k.extend_from_slice(&blake2_128_concat(&version.encode()));
        k
    }

    /// `LegislationYuan::ConstitutionGuardVoteProof[version]`(Blake2_128Concat)的完整 key。
    /// 修宪的永久护宪大法官终审凭据(赞成票数),供守卫背书(第21条,ADR-027 §6.3)。
    pub fn constitution_guard_vote_proof(version: u32) -> Vec<u8> {
        let mut k = map_prefix(b"ConstitutionGuardVoteProof");
        k.extend_from_slice(&blake2_128_concat(&version.encode()));
        k
    }

    /// `LegislationYuan::LawsByScope[Tier::Constitution][0]` 的完整 key(宪法层级唯一性校验)。
    /// Tier::Constitution 编码为单字节变体索引 `[0]`,scope_code = 0。
    pub fn laws_by_scope_constitution() -> Vec<u8> {
        let mut k = map_prefix(b"LawsByScope");
        k.extend_from_slice(&blake2_128_concat(&[
            crate::constitution::TIER_CONSTITUTION,
        ]));
        k.extend_from_slice(&blake2_128_concat(&0u32.encode()));
        k
    }

    /// 立法院模块存储的公共前缀(twox128(pallet)),用于快速判断区块是否动过宪法相关存储。
    pub fn pallet_prefix() -> [u8; 16] {
        twox_128(PALLET_NAME)
    }
}

#[path = "amendment.rs"]
pub mod guard;
#[cfg(test)]
#[path = "../tests/constitution.rs"]
mod tests;
