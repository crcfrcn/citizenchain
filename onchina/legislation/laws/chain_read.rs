//! 法律链读:链上 `Law` / `LawVersion` 的 SCALE 解码镜像 + → 展示 DTO 转换。
//!
//! 复用 onchina 既有读链范式——`kv.value.encoded()` 原始字节 → 本地 `#[derive(Decode)]`
//! 镜像结构解码(见 `onchina/platform/chain/runtime.rs::OnChainAdminAccount`)。镜像字段顺序锁死链端
//! `legislation-yuan` 的 `Law` / `LawVersion`;`Tier`/`LawStatus`/`VoteType`
//! 作单字节枚举解码为 u8(取值已在 `chain_propose` 交叉校验)。章节复用 `chain_propose::ChapterArg`(同
//! SCALE 双向 Encode/Decode)。
//!
//! 本文件包含真实链读、严格解码和展示转换；地域读取保留省代码与省内市编号。
//!

use super::chain_propose::ChapterArg;
use super::model::{house_ref, to_law_chapters, LawView};
use crate::core::chain_url;
use crate::core::db::Db;
use codec::Decode;
use primitives::genesis::LawRegion;
use subxt::dynamic;

const TIER_CONSTITUTION: u8 = 0;

/// 链上 `Law<T>` 解码镜像(字段顺序锁死 legislation-yuan::Law)。
#[derive(Debug, Decode)]
pub struct OnChainLaw {
    pub law_id: u64,
    /// Tier 单字节枚举(0 宪法 / 1 国家 / 2 省 / 3 市)。
    pub tier: u8,
    pub region: LawRegion,
    /// `BoundedVec<CidNumber>`；账户不得充当立法机构身份。
    pub houses: Vec<Vec<u8>>,
    /// 当前真正生效的版本。新法待生效时为 None。
    pub effective_version: Option<u32>,
    /// 已写入链上的最新版本。
    pub latest_version: u32,
    /// 已通过但未到生效时间的版本。
    pub pending_version: Option<u32>,
    /// LawStatus 单字节枚举(0 待生效 / 1 生效 / 2 废止)。
    pub status: u8,
}

/// 链上 `LawVersion<T>` 解码镜像(字段顺序锁死 legislation-yuan::LawVersion)。
#[derive(Debug, Decode)]
pub struct OnChainLawVersion {
    pub law_id: u64,
    pub version: u32,
    pub title: Vec<u8>,
    pub title_en: Option<Vec<u8>>,
    pub chapters: Vec<ChapterArg>,
    pub content_hash: [u8; 32],
    /// VoteType 单字节枚举。
    pub vote_type: u8,
    pub proposal_id: u64,
    pub published_at: u64,
    pub effective_at: u64,
}

/// 链上 `LawVersionLabel<T>` 解码镜像。
#[derive(Debug, Decode)]
pub struct OnChainLawVersionLabel {
    pub title: Vec<u8>,
    pub title_en: Option<Vec<u8>>,
}

/// 链上 `ConstitutionImmutableManifest` 解码镜像。
#[derive(Debug, Decode)]
struct OnChainImmutableManifest {
    article_numbers: Vec<u32>,
    #[allow(dead_code)] // 展示端只需要条号,摘要仍由链端和节点守卫校验。
    article_hashes: Vec<[u8; 32]>,
}

/// 解码链上 `Law` 原始字节。
pub fn decode_law(bytes: &[u8]) -> Result<OnChainLaw, String> {
    let mut input = bytes;
    let law = OnChainLaw::decode(&mut input).map_err(|e| format!("decode Law failed: {e}"))?;
    if !input.is_empty() || !law.region.matches_tier(law.tier) {
        return Err("invalid law region or trailing bytes".into());
    }
    Ok(law)
}

/// 解码链上 `LawVersion` 原始字节。
pub fn decode_law_version(bytes: &[u8]) -> Result<OnChainLawVersion, String> {
    OnChainLawVersion::decode(&mut &bytes[..]).map_err(|e| format!("decode LawVersion failed: {e}"))
}

/// 解码链上 `LawVersionLabel` 原始字节。
pub fn decode_law_version_label(bytes: &[u8]) -> Result<OnChainLawVersionLabel, String> {
    OnChainLawVersionLabel::decode(&mut &bytes[..])
        .map_err(|e| format!("decode LawVersionLabel failed: {e}"))
}

/// 链上 `Law` + 办理端展示版本 → 展示用 `LawView`(字节→String、账户→0x hex、章节→可读)。
pub fn build_law_view(
    law: &OnChainLaw,
    version: &OnChainLawVersion,
    version_label: Option<&OnChainLawVersionLabel>,
    immutable_article_numbers: &[u32],
) -> LawView {
    LawView {
        law_id: law.law_id,
        version: version.version,
        version_title: version_label
            .map(|label| String::from_utf8_lossy(&label.title).into_owned()),
        version_title_en: version_label.and_then(|label| {
            label
                .title_en
                .as_ref()
                .map(|b| String::from_utf8_lossy(b).into_owned())
        }),
        effective_version: law.effective_version,
        latest_version: law.latest_version,
        pending_version: law.pending_version,
        tier: law.tier,
        region: law.region.into(),
        status: law.status,
        vote_type: version.vote_type,
        title: String::from_utf8_lossy(&version.title).into_owned(),
        title_en: version
            .title_en
            .as_ref()
            .map(|b| String::from_utf8_lossy(b).into_owned()),
        content_hash: format!("0x{}", hex::encode(version.content_hash)),
        proposal_id: version.proposal_id,
        published_at: version.published_at,
        effective_at: version.effective_at,
        houses: law
            .houses
            .iter()
            .map(|cid_number| house_ref(cid_number))
            .collect(),
        immutable_article_numbers: if law.tier == TIER_CONSTITUTION {
            immutable_article_numbers.to_vec()
        } else {
            Vec::new()
        },
        chapters: to_law_chapters(&version.chapters),
    }
}

/// 办理端默认展示版本:有待生效版时优先展示待生效全文,否则展示当前生效版。
pub fn operator_display_version(law: &OnChainLaw) -> Option<u32> {
    law.pending_version
        .or(law.effective_version)
        .or_else(|| (law.latest_version > 0).then_some(law.latest_version))
}

// ──────────────── 机构 CID 解析 + subxt 链取数 ────────────────

/// 机构码 + 行政区(china code)→ 唯一机构 CID。
///
/// `institution_code` = 文本码(如 `NRP`);`province_code`/`city_code` = china.sqlite 码
/// (国家机构两者空,省机构仅省码,市机构省+市)。解不出或结果不唯一时返回 None,发起层 fail-closed。
/// 自开连接,故可作 `Fn` 闭包在提案组织里按院逐个解析。
pub(crate) fn resolve_institution_cid_number(
    db: &Db,
    institution_code: &str,
    province_code: &str,
    city_code: &str,
) -> Option<String> {
    let institution_code = institution_code.to_string();
    let province_code = province_code.to_string();
    let city_code = city_code.to_string();
    let cid_number = db
        .with_client(move |conn| {
            let rows = conn
                .query(
                    "SELECT cid_number FROM subjects \
                     WHERE institution_code = $1 AND province_code = $2 AND city_code = $3 \
                     ORDER BY cid_number LIMIT 2",
                    &[&institution_code, &province_code, &city_code],
                )
                .map_err(|e| format!("query institution cid_number failed: {e}"))?;
            if rows.len() != 1 {
                return Ok(None);
            }
            Ok(Some(rows[0].get::<_, String>(0)))
        })
        .ok()
        .flatten()?;
    (!cid_number.is_empty()
        && cid_number.len() <= primitives::core_const::CID_NUMBER_MAX_BYTES as usize)
        .then_some(cid_number)
}

/// 读取链上全部 `Law`(iterate + 镜像 decode,复用 chain_runtime 读链范式)。
///
/// ADR-018——整表扫描一次 + 客户端按已解码字段过滤(law_id/tier/region 均在 value 内,
/// 无需 storage key 反解)。网络读取失败必须向调用方返回错误。
async fn fetch_all_laws() -> Result<Vec<OnChainLaw>, String> {
    let ws_url = chain_url::chain_ws_url()?;
    let client = chain_url::online_client(ws_url.as_str())
        .await
        .map_err(|e| format!("connect chain ws for laws failed: {e}"))?;
    let storage = client
        .storage()
        .at_latest()
        .await
        .map_err(|e| format!("get latest chain storage failed: {e}"))?;
    let query = dynamic::storage("LegislationYuan", "Laws", Vec::<dynamic::Value>::new());
    let mut iter = storage
        .iter(query)
        .await
        .map_err(|e| format!("iterate Laws failed: {e}"))?;
    let mut laws = Vec::new();
    while let Some(item) = iter.next().await {
        let kv = item.map_err(|e| format!("read Law failed: {e}"))?;
        laws.push(decode_law(kv.value.encoded())?);
    }
    Ok(laws)
}

/// 读取链上全部 `LawVersion`(iterate + 镜像 decode)。
async fn fetch_all_law_versions() -> Result<Vec<OnChainLawVersion>, String> {
    let ws_url = chain_url::chain_ws_url()?;
    let client = chain_url::online_client(ws_url.as_str())
        .await
        .map_err(|e| format!("connect chain ws for law versions failed: {e}"))?;
    let storage = client
        .storage()
        .at_latest()
        .await
        .map_err(|e| format!("get latest chain storage failed: {e}"))?;
    let query = dynamic::storage(
        "LegislationYuan",
        "LawVersions",
        Vec::<dynamic::Value>::new(),
    );
    let mut iter = storage
        .iter(query)
        .await
        .map_err(|e| format!("iterate LawVersions failed: {e}"))?;
    let mut versions = Vec::new();
    while let Some(item) = iter.next().await {
        let kv = item.map_err(|e| format!("read LawVersion failed: {e}"))?;
        versions.push(decode_law_version(kv.value.encoded())?);
    }
    Ok(versions)
}

/// 读取宪法不可修改条款号清单。缺失时返回空,由展示层降级为不显示徽章。
pub async fn fetch_immutable_article_numbers() -> Result<Vec<u32>, String> {
    let ws_url = chain_url::chain_ws_url()?;
    let client = chain_url::online_client(ws_url.as_str())
        .await
        .map_err(|e| format!("connect chain ws for immutable manifest failed: {e}"))?;
    let storage = client
        .storage()
        .at_latest()
        .await
        .map_err(|e| format!("get latest chain storage failed: {e}"))?;
    let address = dynamic::storage(
        "LegislationYuan",
        "ConstitutionImmutableManifest",
        Vec::<dynamic::Value>::new(),
    );
    let Some(thunk) = storage
        .fetch(&address)
        .await
        .map_err(|e| format!("fetch ConstitutionImmutableManifest failed: {e}"))?
    else {
        return Ok(Vec::new());
    };
    let mut raw = thunk.encoded();
    let manifest = OnChainImmutableManifest::decode(&mut raw)
        .map_err(|e| format!("decode ConstitutionImmutableManifest failed: {e}"))?;
    Ok(manifest.article_numbers)
}

/// 按 law_id 取单部法律主体记录。
pub async fn fetch_law(law_id: u64) -> Result<Option<OnChainLaw>, String> {
    Ok(fetch_all_laws()
        .await?
        .into_iter()
        .find(|law| law.law_id == law_id))
}

/// 读取地域保留既有省代码和省内市编号，不将省码转数字或另编合成编码。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LawScope {
    pub(crate) tier: u8,
    pub(crate) province_code: String,
    pub(crate) city_code: String,
}

pub(crate) fn law_matches_scope(law: &OnChainLaw, scope: &LawScope) -> bool {
    if law.tier != scope.tier { return false; }
    let Ok(region) = LawRegion::from_codes(scope.tier, scope.province_code.as_bytes(), scope.city_code.as_bytes()) else { return false; };
    if law.region != region { return false; }
    if scope.tier <= 1 { return true; }
    if scope.province_code.is_empty() || (scope.tier == 3 && scope.city_code.is_empty()) || law.houses.is_empty() { return false; }
    // 市号在省内唯一；按法律已有机构 CID 的原省码与市号共同匹配。
    law.houses.iter().all(|cid| primitives::cid::number::cid_scope_codes(cid).is_ok_and(|(province, city)| {
        province.as_slice() == scope.province_code.as_bytes() && (scope.tier == 2 || city.as_slice() == scope.city_code.as_bytes())
    }))
}

pub(crate) async fn list_laws_by_scope(scope: &LawScope) -> Result<Vec<OnChainLaw>, String> {
    Ok(fetch_all_laws().await?.into_iter().filter(|law| law_matches_scope(law, scope)).collect())
}

/// 按 (law_id, version) 取单个法律版本全文。
pub async fn fetch_law_version(
    law_id: u64,
    version: u32,
) -> Result<Option<OnChainLawVersion>, String> {
    Ok(fetch_all_law_versions()
        .await?
        .into_iter()
        .find(|v| v.law_id == law_id && v.version == version))
}

/// 按 (law_id, version) 取单个法律版本标签。
pub async fn fetch_law_version_label(
    law_id: u64,
    version: u32,
) -> Result<Option<OnChainLawVersionLabel>, String> {
    let ws_url = chain_url::chain_ws_url()?;
    let client = chain_url::online_client(ws_url.as_str())
        .await
        .map_err(|e| format!("connect chain ws for law version label failed: {e}"))?;
    let storage = client
        .storage()
        .at_latest()
        .await
        .map_err(|e| format!("get latest chain storage failed: {e}"))?;
    let address = dynamic::storage(
        "LegislationYuan",
        "LawVersionLabels",
        vec![
            dynamic::Value::u128(law_id as u128),
            dynamic::Value::u128(version as u128),
        ],
    );
    let Some(thunk) = storage
        .fetch(&address)
        .await
        .map_err(|e| format!("fetch LawVersionLabels failed: {e}"))?
    else {
        return Ok(None);
    };
    Ok(Some(decode_law_version_label(thunk.encoded())?))
}

#[cfg(test)]
// 法律链上读取夹具用于锁定 SCALE 契约，失败时应立即中止测试。
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::super::chain_propose::{ArticleArg, ClauseArg, SectionArg};
    use super::*;
    use codec::Encode;
    use legislation_yuan::{LawStatus, Tier, VoteType};

    fn sample_chapters() -> Vec<ChapterArg> {
        vec![ChapterArg {
            number: 1,
            title: "总则".as_bytes().to_vec(),
            title_en: Some("General".as_bytes().to_vec()),
            sections: vec![SectionArg {
                number: 1,
                title: "定义".as_bytes().to_vec(),
                title_en: None,
                articles: vec![ArticleArg {
                    number: 1,
                    title: "第一条".as_bytes().to_vec(),
                    title_en: None,
                    body: "正文".as_bytes().to_vec(),
                    body_en: None,
                    clauses: vec![ClauseArg {
                        number: 1,
                        text: "第一款".as_bytes().to_vec(),
                        text_en: None,
                    }],
                }],
            }],
        }]
    }

    /// 范围夹具复用既有 CID 生成规则，只用于法律地域读取回归。
    fn scoped_law(tier: u8, province: &str, city: &str) -> OnChainLaw {
        let cid = primitives::cid::generator::generate_cid_number(
            primitives::cid::generator::GenerateCidNumberInput {
                public_key: "0x1234",
                p1: "0",
                province_code: province,
                province_name: "测试省",
                city_code: city,
                city_name: "测试市",
                year: "2026",
                institution: if tier == 2 { "PRP" } else { "CLEG" },
            },
        ).unwrap();
        OnChainLaw {
            law_id: 1, tier, region: LawRegion::from_codes(tier, province.as_bytes(), city.as_bytes()).unwrap(), houses: vec![cid.into_bytes()],
            effective_version: Some(1), latest_version: 1, pending_version: None, status: 1,
        }
    }

    #[test]
    fn province_and_local_city_numbers_remain_distinct_and_unmodified() {
        let scope = LawScope { tier:3,province_code:"GD".into(),city_code:"001".into() };
        assert!(law_matches_scope(&scoped_law(3, "GD", "001"), &scope));
        assert!(!law_matches_scope(&scoped_law(3, "GZ", "001"), &scope));
        assert!(!law_matches_scope(&scoped_law(3, "GD", "002"), &scope));
        assert_eq!(scope.province_code,"GD"); assert_eq!(scope.city_code,"001");
        let mut malformed = scoped_law(3, "GD", "001");
        malformed.houses = vec![b"invalid-cid".to_vec()];
        assert!(!law_matches_scope(&malformed,&scope));
        let province = LawScope { tier:2,province_code:"GD".into(),city_code:String::new() };
        let mut law = scoped_law(2, "GD", "002");
        assert!(law_matches_scope(&law,&province));
        law.houses.clear(); assert!(!law_matches_scope(&law,&province));
    }

    #[test]
    fn all_saved_house_cids_must_match_the_requested_region() {
        let scope = LawScope { tier:3, province_code:"GD".into(), city_code:"001".into() };
        let mut law = scoped_law(3, "GD", "001");
        law.houses.extend(scoped_law(3, "GZ", "001").houses);
        assert!(!law_matches_scope(&law, &scope));
    }

    #[test]
    fn constitution_scope_keeps_national_legislature_ownership() {
        let nlg = primitives::cid::china::china_lf::CHINA_LF[0].cid_number.as_bytes().to_vec();
        let law = OnChainLaw {
            law_id: 0, tier: 0, region: LawRegion::NATIONAL, houses: vec![nlg.clone()],
            effective_version: Some(1), latest_version: 1, pending_version: None, status: 1,
        };
        let scope = LawScope { tier:0, province_code:String::new(), city_code:String::new() };
        assert!(law_matches_scope(&law, &scope));
        assert_eq!(law.houses, vec![nlg]);
        assert_eq!(primitives::cid::code::COUNTRY_CN, *b"CN");
    }

    #[test]
    fn stored_region_must_agree_with_original_house_cids() {
        let scope = LawScope { tier: 3, province_code: "GD".into(), city_code: "001".into() };
        let mut law = scoped_law(3, "GD", "001");
        law.region = LawRegion::from_codes(3, b"GZ", b"001").unwrap();
        assert!(!law_matches_scope(&law, &scope));
    }

    /// 用链端现有 `Tier`/`LawStatus` 编码 golden，镜像解码字段保持一致。
    #[test]
    fn law_decodes_from_runtime_encoded_bytes() {
        let houses = vec![
            b"LN001-NRP0G-000000001-2026".to_vec(),
            b"LN001-NSN0G-000000001-2026".to_vec(),
        ];
        let mut golden = Vec::new();
        golden.extend(7u64.encode());
        golden.extend(Tier::National.encode());
        golden.extend(0u32.encode()); // 合法全国地域固定四零字节
        golden.extend(houses.encode());
        golden.extend(Some(3u32).encode());
        golden.extend(3u32.encode());
        golden.extend(Option::<u32>::None.encode());
        golden.extend(LawStatus::Effective.encode());

        let law = decode_law(&golden).expect("decode Law");
        assert_eq!(law.law_id, 7);
        assert_eq!(law.tier, 1); // National
        assert_eq!(law.region, LawRegion::NATIONAL);
        assert_eq!(law.houses.len(), 2);
        assert_eq!(law.houses[0], b"LN001-NRP0G-000000001-2026");
        assert_eq!(law.effective_version, Some(3));
        assert_eq!(law.latest_version, 3);
        assert_eq!(law.pending_version, None);
        assert_eq!(law.status, 1); // Effective
        let mut trailing = golden.clone();
        trailing.push(0);
        assert!(decode_law(&trailing).is_err());
        let mut invalid = golden.clone();
        invalid[8] = 2; // 省级不能使用全国零地域。
        assert!(decode_law(&invalid).is_err());
        for (province, city) in [(b"GD", b"001"), (b"GZ", b"001")] {
            let region = LawRegion::from_codes(3, province, city).unwrap();
            let mut regional = golden.clone();
            regional[8] = 3;
            regional[9..13].copy_from_slice(&region.encode());
            let law = decode_law(&regional).expect("regional law");
            assert_eq!(law.region, region);
            assert_eq!(law.latest_version, 3);
        }
    }

    /// LawVersion 镜像解码 + 组装 LawView(章节字节→String、houses→CID、hash→0x hex)。
    #[test]
    fn law_version_decodes_and_builds_view() {
        let chapters = sample_chapters();
        let mut golden = Vec::new();
        golden.extend(7u64.encode());
        golden.extend(2u32.encode());
        golden.extend("道路交通安全法".as_bytes().to_vec().encode());
        golden.extend(Some("Road".as_bytes().to_vec()).encode());
        golden.extend(chapters.encode());
        golden.extend([9u8; 32].encode());
        golden.extend(VoteType::Major.encode());
        golden.extend(11u64.encode());
        golden.extend(500u64.encode());
        golden.extend(1000u64.encode());

        let version = decode_law_version(&golden).expect("decode LawVersion");
        assert_eq!(version.version, 2);
        assert_eq!(version.vote_type, 2); // Major
        assert_eq!(version.chapters.len(), 1);
        assert_eq!(version.title, "道路交通安全法".as_bytes());

        let label = decode_law_version_label(&{
            let mut bytes = Vec::new();
            "创世版".as_bytes().to_vec().encode_to(&mut bytes);
            Some("Genesis Edition".as_bytes().to_vec()).encode_to(&mut bytes);
            bytes
        })
        .expect("decode LawVersionLabel");
        assert_eq!(label.title, "创世版".as_bytes());

        let law = OnChainLaw {
            law_id: 7,
            tier: 1,
            region: LawRegion::NATIONAL,
            houses: vec![
                b"LN001-NRP0G-000000001-2026".to_vec(),
                b"LN001-NSN0G-000000001-2026".to_vec(),
            ],
            effective_version: Some(2),
            latest_version: 2,
            pending_version: None,
            status: 1,
        };
        let view = build_law_view(&law, &version, Some(&label), &[1]);
        assert_eq!(view.effective_version, Some(2));
        assert_eq!(view.latest_version, 2);
        assert_eq!(view.pending_version, None);
        assert_eq!(view.version_title.as_deref(), Some("创世版"));
        assert_eq!(view.version_title_en.as_deref(), Some("Genesis Edition"));
        assert_eq!(view.title, "道路交通安全法");
        assert_eq!(view.houses.len(), 2);
        assert_eq!(view.houses[0].cid_number, "LN001-NRP0G-000000001-2026");
        assert!(view.immutable_article_numbers.is_empty());
        assert!(view.content_hash.starts_with("0x"));
        assert_eq!(view.chapters[0].title, "总则");
        assert_eq!(
            view.chapters[0].sections[0].articles[0].clauses[0].text,
            "第一款"
        );

        let constitution_law = OnChainLaw { tier: 0, ..law };
        let constitution_view = build_law_view(&constitution_law, &version, None, &[1, 2]);
        assert_eq!(constitution_view.immutable_article_numbers, vec![1, 2]);
        assert!(constitution_view.version_title.is_none());
    }
}
