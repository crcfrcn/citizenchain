//! 法律链读:链上 `Law` / `LawVersion` 的 SCALE 解码镜像 + → 展示 DTO 转换。
//!
//! 复用 onchina 既有读链范式——`kv.value.encoded()` 原始字节 → 本地 `#[derive(Decode)]`
//! 镜像结构解码(见 `onchina/platform/chain/runtime.rs::OnChainAdminAccount`)。镜像字段顺序锁死链端
//! `legislation` 的 `Law` / `LawVersion`;`Tier`/`LawStatus`/`VoteType`
//! 作单字节枚举解码为 u8(取值已在 `chain_propose` 交叉校验)。章节复用 `chain_propose::ChapterArg`(同
//! SCALE 双向 Encode/Decode)。
//!
//! 本文件包含真实链读、严格解码和展示转换；地域读取保留省代码与省内市编号。
//!

use super::chain_propose::ChapterArg;
use super::model::{house_ref, to_law_chapters, LawView};
use codec::Decode;
use onchina::chain_url;
use onchina::database::Db;
use primitives::genesis::LawRegion;
use subxt::dynamic;

const TIER_CONSTITUTION: u8 = 0;

/// 链上 `Law<T>` 解码镜像(字段顺序锁死 legislation::Law)。
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

/// 链上 `LawVersion<T>` 解码镜像(字段顺序锁死 legislation::LawVersion)。
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
                    "SELECT cid_number FROM subject \
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
    if law.tier != scope.tier {
        return false;
    }
    let Ok(region) = LawRegion::from_codes(
        scope.tier,
        scope.province_code.as_bytes(),
        scope.city_code.as_bytes(),
    ) else {
        return false;
    };
    if law.region != region {
        return false;
    }
    if scope.tier <= 1 {
        return true;
    }
    if scope.province_code.is_empty()
        || (scope.tier == 3 && scope.city_code.is_empty())
        || law.houses.is_empty()
    {
        return false;
    }
    // 市号在省内唯一；按法律已有机构 CID 的原省码与市号共同匹配。
    law.houses.iter().all(|cid| {
        primitives::cid::number::cid_scope_codes(cid).is_ok_and(|(province, city)| {
            province.as_slice() == scope.province_code.as_bytes()
                && (scope.tier == 2 || city.as_slice() == scope.city_code.as_bytes())
        })
    })
}

pub(crate) async fn list_laws_by_scope(scope: &LawScope) -> Result<Vec<OnChainLaw>, String> {
    Ok(fetch_all_laws()
        .await?
        .into_iter()
        .filter(|law| law_matches_scope(law, scope))
        .collect())
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
#[allow(clippy::expect_used, clippy::unwrap_used)]
#[path = "../tests/laws_read.rs"]
mod tests;
