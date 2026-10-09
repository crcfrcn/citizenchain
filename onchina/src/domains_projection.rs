//! 链→注册局本地库 通用投影逻辑(M1 indexer 增量 / M3 联邦 drill-in 共用)。
//!
//! 行为契约由本模块测试和数据库 schema 共同固定。
//! 本模块只放**纯逻辑**:给定"链上实体数据 + 本地现存行 + 本节点作用域",算出要写回的最终行。
//! 因此保正本(链下正本不被覆盖)、作用域过滤、插入 vs 更新全部可离线单测,无需活链。
//!
//! 关键手法:merge 出的最终行**已把链下正本从 existing 原样带回**,故写库时即使走现有
//! `upsert_citizen_row`(ON CONFLICT DO UPDATE 全列=EXCLUDED)也不会丢正本——EXCLUDED 已是
//! 保留后的正确值。链读与写库在下方 writer 层,merge 纯逻辑离线单测。

use chrono::{DateTime, Utc};

use crate::cid::InstitutionCategory;
use crate::core::chain_runtime::{
    institution_lookup, is_tier1_registry, read_chain_citizen_detail, read_chain_citizen_detail_at,
    OnChainCitizenDetail, OnChainInstitution,
};
use crate::core::db::Db;
use crate::domains::citizens::model::{CitizenRecord, CitizenStatus};
use crate::institution::subjects::model::LegalRepresentative;
use crate::institution::subjects::Institution;

use primitives::cid::code::is_private_legal_code;
use primitives::cid::number::{cid_scope_codes, parse_cid_number_parts};

/// 本节点作用域(省码 + 市码)。公民按链上 residence 归属市;机构按 CID 市码归属市。
#[derive(Debug, Clone)]
pub(crate) struct NodeScope {
    pub(crate) province_code: String,
    pub(crate) city_code: String,
}

/// 链上公民数据(由 chain_read 层从 CidRegistry/VotingIdentity/AccountId/Candidate 读出)。
///
/// `family_name` 等竞选专属字段只有竞选身份存在时才是 `Some`;投票身份公民链上无姓名,为 `None`,
/// 此时保留本地正本姓名(如注册局补录)。
#[derive(Debug, Clone)]
pub(crate) struct ChainCitizen {
    pub(crate) cid_number: String,
    /// 居住省码(链上 residence),= 归属省。
    pub(crate) province_code: String,
    /// 居住市码(链上 residence),= 归属市。
    pub(crate) city_code: String,
    pub(crate) town_code: String,
    pub(crate) account_id: Option<String>,
    pub(crate) binding_revision: u64,
    pub(crate) binding_finalized_block_number: i64,
    pub(crate) binding_finalized_block_hash: String,
    /// citizen_status == Normal。
    pub(crate) status_normal: bool,
    pub(crate) passport_valid_from: Option<String>,
    pub(crate) passport_valid_until: Option<String>,
    // 竞选身份专属(投票身份为 None):
    pub(crate) family_name: Option<String>,
    pub(crate) given_name: Option<String>,
    pub(crate) citizen_sex: Option<String>,
    pub(crate) birth_date: Option<String>,
    pub(crate) birth_province_code: Option<String>,
    pub(crate) birth_city_code: Option<String>,
    pub(crate) birth_town_code: Option<String>,
}

impl ChainCitizen {
    /// 是否具投票资格:状态正常 + 护照有效期两端都在链上(窗口存在)。
    /// 具体"今日是否落在窗口内"由投票引擎按链上时间判定;此处只表达"链上身份可投"。
    fn voting_eligible(&self) -> bool {
        self.status_normal
            && self.passport_valid_from.is_some()
            && self.passport_valid_until.is_some()
    }
}

fn stable_citizen_id(cid_number: &str) -> u64 {
    cid_number
        .bytes()
        .fold(0u64, |acc, byte| {
            acc.wrapping_mul(131).wrapping_add(u64::from(byte))
        })
        .max(1)
}

/// 链上字段优先,取不到则回落本地现存值(保正本),再回落空串。
fn chain_or_local_str(chain: &Option<String>, local: Option<&str>) -> String {
    chain
        .clone()
        .or_else(|| local.map(str::to_string))
        .unwrap_or_default()
}

/// 计算公民投影要写回的最终行;返回 `None` 表示不在本作用域,跳过。
///
/// - **作用域**:公民按链上 residence 的 (省,市) 归属;不匹配本节点则跳过。
/// - **链上来源列**(投影权威):居住省/市/镇、账户、状态、护照有效期、投票资格;竞选存在时姓名/性别/
///   出生日期/出生地。
/// - **链下正本列**(绝不覆盖,从 existing 原样带回):passport_no、archive_hash、
///   onchain_* 展示、创建人;以及投票身份公民链上无姓名时的姓名/性别/出生日期(保留本地补录)。
pub(crate) fn merge_citizen_record(
    chain: &ChainCitizen,
    existing: Option<&CitizenRecord>,
    scope: &NodeScope,
    now: DateTime<Utc>,
) -> Option<CitizenRecord> {
    if chain.province_code != scope.province_code || chain.city_code != scope.city_code {
        return None;
    }

    let citizen_status = if chain.status_normal {
        CitizenStatus::Normal
    } else {
        CitizenStatus::Revoked
    };

    Some(CitizenRecord {
        id: existing
            .map(|e| e.id)
            .unwrap_or_else(|| stable_citizen_id(&chain.cid_number)),
        cid_number: chain.cid_number.clone(),
        // ── 链下正本:保留 existing,新建则空 ──
        passport_no: existing.map(|e| e.passport_no.clone()).unwrap_or_default(),
        // 姓名/性别/出生日期:竞选身份链上有 → 用链上;投票身份链上无 → 保留本地正本。
        family_name: chain_or_local_str(
            &chain.family_name,
            existing.map(|e| e.family_name.as_str()),
        ),
        given_name: chain_or_local_str(&chain.given_name, existing.map(|e| e.given_name.as_str())),
        citizen_sex: chain_or_local_str(
            &chain.citizen_sex,
            existing.map(|e| e.citizen_sex.as_str()),
        ),
        citizen_birth_date: chain_or_local_str(
            &chain.birth_date,
            existing.map(|e| e.citizen_birth_date.as_str()),
        ),
        // ── 链上来源列:投影权威 ──
        // 当前账户完全以 finalized 链快照为准；链上无有效账户时禁止回退本地旧账户。
        account_id: chain.account_id.clone(),
        binding_revision: chain.binding_revision,
        binding_finalized_block_number: Some(chain.binding_finalized_block_number),
        binding_finalized_block_hash: Some(chain.binding_finalized_block_hash.clone()),
        citizen_status,
        voting_eligible: chain.voting_eligible(),
        passport_valid_from: chain_or_local_str(
            &chain.passport_valid_from,
            existing.map(|e| e.passport_valid_from.as_str()),
        ),
        passport_valid_until: chain_or_local_str(
            &chain.passport_valid_until,
            existing.map(|e| e.passport_valid_until.as_str()),
        ),
        status_updated_at: Some(now.timestamp()),
        province_code: chain.province_code.clone(),
        city_code: chain.city_code.clone(),
        town_code: chain.town_code.clone(),
        birth_province_code: chain_or_local_str(
            &chain.birth_province_code,
            existing.map(|e| e.birth_province_code.as_str()),
        ),
        birth_city_code: chain_or_local_str(
            &chain.birth_city_code,
            existing.map(|e| e.birth_city_code.as_str()),
        ),
        birth_town_code: chain_or_local_str(
            &chain.birth_town_code,
            existing.map(|e| e.birth_town_code.as_str()),
        ),
        // ── 链下正本 / 展示:保留 existing ──
        archive_hash: existing.and_then(|e| e.archive_hash.clone()),
        onchain_tx_hash: existing.and_then(|e| e.onchain_tx_hash.clone()),
        onchain_block_number: existing.and_then(|e| e.onchain_block_number),
        onchain_at: existing.and_then(|e| e.onchain_at),
        creator_account_id: existing
            .map(|e| e.creator_account_id.clone())
            .or_else(|| chain.account_id.clone())
            .unwrap_or_default(),
        created_at: existing.map(|e| e.created_at).unwrap_or(now),
        updater_account_id: existing.and_then(|e| e.updater_account_id.clone()),
        updated_at: now,
    })
}

/// 链上机构数据(公权/私权;由 chain_read 层从 Institutions 读出)。
#[derive(Debug, Clone)]
pub(crate) struct ChainInstitution {
    pub(crate) cid_number: String,
    /// CID 省码(= 归属省)。
    pub(crate) province_code: String,
    /// CID 市码(= 归属市)。机构按 CID 市码归属,不依赖 residence。
    pub(crate) city_code: String,
    pub(crate) town_code: String,
    pub(crate) cid_full_name: String,
    pub(crate) cid_short_name: String,
    pub(crate) institution_code: String,
    pub(crate) profit: bool,
    pub(crate) is_private: bool,
    pub(crate) legal_representative: Option<LegalRepresentative>,
}

/// 计算机构投影要写回的最终行;`None` = 不在本作用域,跳过。
///
/// - **作用域**:机构按 CID 的 (省,市) 归属;不匹配本节点则跳过。
/// - **链上来源列**(投影权威):全称/简称、省/市/镇码、机构码、盈利位、法人标记、法定代表人。
/// - **链下正本列**(绝不覆盖,从 existing 保留):法定代表人证件照、机构业务分类(private_type/
///   partnership_kind/education_type,由市补录)、创建人、创建时间。
pub(crate) fn merge_institution_record(
    chain: &ChainInstitution,
    existing: Option<&Institution>,
    scope: &NodeScope,
    now: DateTime<Utc>,
) -> Option<Institution> {
    if chain.province_code != scope.province_code || chain.city_code != scope.city_code {
        return None;
    }
    Some(Institution {
        cid_number: chain.cid_number.clone(),
        cid_full_name: Some(chain.cid_full_name.clone()),
        cid_short_name: Some(chain.cid_short_name.clone()),
        category: if chain.is_private {
            InstitutionCategory::PrivateInstitution
        } else {
            InstitutionCategory::GovInstitution
        },
        p1: if chain.profit { "1" } else { "0" }.to_string(),
        // 行政区名字不入库(china.sqlite 单源,读时派生),只存代码。
        province_name: String::new(),
        city_name: String::new(),
        town_name: String::new(),
        province_code: chain.province_code.clone(),
        city_code: chain.city_code.clone(),
        town_code: chain.town_code.clone(),
        institution_code: chain.institution_code.clone(),
        // ── 链下正本 / 市补录业务分类:保留 existing ──
        education_type: existing.and_then(|e| e.education_type.clone()),
        // private_type 优先保留本地既有(市补录可能更细),纯链上投影(existing=None)按机构码
        // 确定性派生,否则私权列表 SQL 的 `private_type IS NOT NULL` 会漏掉纯投影的私权机构。
        private_type: existing.and_then(|e| e.private_type.clone()).or_else(|| {
            crate::domains::private::common::private_type_code_from_institution_code(
                &chain.institution_code,
            )
            .map(str::to_string)
        }),
        partnership_kind: existing.and_then(|e| e.partnership_kind.clone()),
        has_legal_personality: if chain.is_private {
            Some(true) // 私权法人
        } else {
            existing.and_then(|e| e.has_legal_personality)
        },
        parent_cid_number: existing.and_then(|e| e.parent_cid_number.clone()),
        // ── 链上来源:法定代表人(姓名/CID/账户)投影权威 ──
        legal_representative: chain.legal_representative.clone(),
        // ── 链下正本:证件照保留 existing ──
        legal_representative_photo_path: existing
            .and_then(|e| e.legal_representative_photo_path.clone()),
        legal_representative_photo_name: existing
            .and_then(|e| e.legal_representative_photo_name.clone()),
        legal_representative_photo_mime: existing
            .and_then(|e| e.legal_representative_photo_mime.clone()),
        legal_representative_photo_size: existing.and_then(|e| e.legal_representative_photo_size),
        creator_account_id: existing.and_then(|e| e.creator_account_id.clone()),
        created_at: existing.map(|e| e.created_at).unwrap_or(now),
    })
}

// ───────────────────────── 链读映射 + writer(接线层) ─────────────────────────

/// 从机构 CID 唯一解析 (province_code, city_code)。复用 primitives 权威单源
/// `cid_scope_codes`;人主体 CID(去地域化、R5 不载省市)传入即 fail-closed 返回 Err。
/// 本函数仅应收机构 CID(节点自身机构 / 公私权机构投影)。
fn r5_province_city(cid_number: &str) -> Result<(String, String), String> {
    let (province, city) = cid_scope_codes(cid_number.as_bytes())
        .map_err(|e| format!("cid {cid_number} scope invalid: {e}"))?;
    Ok((
        String::from_utf8_lossy(&province).into_owned(),
        String::from_utf8_lossy(&city).into_owned(),
    ))
}

/// 解析本节点作用域:`Ok(None)` = 未绑定机构;`Ok(Some((is_federal, scope)))`。
/// 城市/省注册局 scope = 其自身机构 CID 的 (省,市);联邦(Tier1)只用 is_federal 标记。
pub(crate) fn resolve_node_scope(db: &Db) -> Result<Option<(bool, NodeScope)>, String> {
    let Some(binding) = crate::auth::repo::active_node_binding(db)? else {
        return Ok(None);
    };
    let is_federal = is_tier1_registry(binding.institution_code.as_str());
    let (province_code, city_code) = r5_province_city(binding.institution_cid_number.as_str())?;
    Ok(Some((
        is_federal,
        NodeScope {
            province_code,
            city_code,
        },
    )))
}

fn chain_citizen_from_detail(d: OnChainCitizenDetail) -> ChainCitizen {
    let fmt_u32 = |v: Option<u32>| v.map(|n| n.to_string());
    let (family_name, given_name, citizen_sex, birth_date, bp, bc, bt) = match d.candidate {
        Some(c) => (
            Some(String::from_utf8_lossy(&c.family_name).into_owned()),
            Some(String::from_utf8_lossy(&c.given_name).into_owned()),
            Some(if c.citizen_sex == 0 { "MALE" } else { "FEMALE" }.to_string()),
            Some(c.birth_date.to_string()),
            Some(c.birth_province_code),
            Some(c.birth_city_code),
            Some(c.birth_town_code),
        ),
        None => (None, None, None, None, None, None, None),
    };
    ChainCitizen {
        cid_number: d.cid_number,
        province_code: d.residence_province_code,
        city_code: d.residence_city_code,
        town_code: d.residence_town_code,
        account_id: d.account_id.map(|a| format!("0x{}", hex::encode(a))),
        binding_revision: d.binding_revision,
        binding_finalized_block_number: i64::from(d.binding_finalized_block_number),
        binding_finalized_block_hash: format!("0x{}", hex::encode(d.binding_finalized_block_hash)),
        status_normal: d.status_normal,
        passport_valid_from: fmt_u32(d.passport_valid_from),
        passport_valid_until: fmt_u32(d.passport_valid_until),
        family_name,
        given_name,
        citizen_sex,
        birth_date,
        birth_province_code: bp,
        birth_city_code: bc,
        birth_town_code: bt,
    }
}

/// 按 CID 投影单个公民进本地库(读链→映射→保正本 merge→upsert)。
/// `false` = 链上无此公民或不在本作用域(跳过)。
pub(crate) async fn project_citizen_by_cid(
    db: &Db,
    cid_number: &str,
    scope: &NodeScope,
) -> Result<bool, String> {
    project_citizen_by_cid_at(db, cid_number, scope, None).await
}

/// Indexer 必须投影事件所在 finalized 块，不能在追块时偷读更晚 head。
pub(crate) async fn project_citizen_by_cid_at(
    db: &Db,
    cid_number: &str,
    scope: &NodeScope,
    block_hash: Option<[u8; 32]>,
) -> Result<bool, String> {
    let detail = match block_hash {
        Some(hash) => read_chain_citizen_detail_at(cid_number, Some(hash)).await?,
        None => read_chain_citizen_detail(cid_number).await?,
    };
    let Some(detail) = detail else {
        return Ok(false);
    };
    let chain = chain_citizen_from_detail(detail);
    let existing = db.find_citizen_by_cid(cid_number)?;
    ensure_binding_projection_monotonic(&chain, existing.as_ref())?;
    let Some(merged) = merge_citizen_record(&chain, existing.as_ref(), scope, Utc::now()) else {
        return Ok(false);
    };
    db.upsert_citizen_row(&merged)?;
    Ok(true)
}

/// 防止旧 finalized 视图覆盖较新绑定；同 revision 的账户不一致同样是链/投影冲突。
fn ensure_binding_projection_monotonic(
    chain: &ChainCitizen,
    existing: Option<&CitizenRecord>,
) -> Result<(), String> {
    let Some(existing) = existing else {
        return Ok(());
    };
    if existing.binding_revision > chain.binding_revision {
        return Err(format!(
            "CID {} binding revision regressed from {} to {}",
            chain.cid_number, existing.binding_revision, chain.binding_revision
        ));
    }
    if existing.binding_revision == chain.binding_revision
        && existing.binding_revision > 0
        && existing.account_id != chain.account_id
    {
        return Err(format!(
            "CID {} account changed without binding revision advance",
            chain.cid_number
        ));
    }
    Ok(())
}

fn chain_institution_from_lookup(
    cid_number: &str,
    oci: OnChainInstitution,
) -> Result<ChainInstitution, String> {
    let parts = parse_cid_number_parts(cid_number)
        .map_err(|e| format!("institution cid {cid_number} invalid: {e}"))?;
    let (province_code, city_code) = r5_province_city(cid_number)?;
    let is_private = is_private_legal_code(&parts.institution);
    let legal_representative = oci.legal_representative.map(|lr| LegalRepresentative {
        family_name: String::from_utf8_lossy(&lr.family_name).into_owned(),
        given_name: String::from_utf8_lossy(&lr.given_name).into_owned(),
        cid_number: String::from_utf8_lossy(&lr.cid_number).into_owned(),
        account_id: format!("0x{}", hex::encode(lr.account_id)),
    });
    Ok(ChainInstitution {
        cid_number: cid_number.to_string(),
        province_code,
        city_code,
        town_code: String::from_utf8_lossy(&oci.town_code).into_owned(),
        cid_full_name: String::from_utf8_lossy(&oci.cid_full_name).into_owned(),
        cid_short_name: String::from_utf8_lossy(&oci.cid_short_name).into_owned(),
        institution_code: parts.institution_code_text,
        profit: parts.profit,
        is_private,
        legal_representative,
    })
}

/// 按 CID 投影单个私权机构进本地库。公权机构走 gov 投影,此处只收私权。
/// `false` = 链上无此机构 / 非私权 / 不在本作用域(跳过)。
pub(crate) async fn project_private_institution_by_cid(
    db: &Db,
    cid_number: &str,
    scope: &NodeScope,
) -> Result<bool, String> {
    let Some(oci) = institution_lookup(cid_number).await? else {
        return Ok(false);
    };
    let chain = chain_institution_from_lookup(cid_number, oci)?;
    if !chain.is_private {
        return Ok(false); // 公权机构由 gov 投影负责
    }
    let existing = db
        .get_institution_with_accounts(cid_number)?
        .map(|(inst, _accounts)| inst);
    let Some(merged) = merge_institution_record(&chain, existing.as_ref(), scope, Utc::now())
    else {
        return Ok(false);
    };
    db.upsert_institution_row(&merged)?;
    Ok(true)
}

/// M3 联邦 drill-in:把目标 (省,市) 的链上全部公民 + 私权机构投影进本地库。
/// 返回 (投影公民数, 投影私权机构数)。公权机构由 gov 投影负责,不在此。
/// 每条内部按作用域 + 保正本 merge;单条失败向上抛(由调用方决定是否整体失败)。
pub(crate) async fn drill_in_project_scope(
    db: &Db,
    scope: &NodeScope,
) -> Result<(usize, usize), String> {
    // 公民:按链上 residence 枚举本市 CID。
    let mut citizen_cids: Vec<String> = Vec::new();
    crate::core::chain_runtime::for_each_chain_citizen_cid_in_scope(
        scope.province_code.as_str(),
        scope.city_code.as_str(),
        |cid| citizen_cids.push(cid),
    )
    .await?;
    let mut citizens = 0usize;
    for cid in citizen_cids {
        if project_citizen_by_cid(db, cid.as_str(), scope).await? {
            citizens += 1;
        }
    }

    // 私权机构:走私权专用投影(联邦按市查看私权机构列表时也复用它)。
    let institutions = drill_in_project_private_scope(db, scope).await?;
    Ok((citizens, institutions))
}

/// M3 私权部分:只投影目标 (省,市) 的链上私权机构。
/// 联邦(Tier1)省组管理员按市查看私权机构列表时触发它(枚举全部私权 CID,按 CID 市码粗过滤本市,
/// 逐个投影;私权数量有限,不涉及公民全表扫)。返回投影的私权机构数。
pub(crate) async fn drill_in_project_private_scope(
    db: &Db,
    scope: &NodeScope,
) -> Result<usize, String> {
    let mut inst_cids: Vec<String> = Vec::new();
    crate::core::chain_runtime::for_each_chain_private_institution_cid(|cid| inst_cids.push(cid))
        .await?;
    let mut institutions = 0usize;
    for cid in inst_cids {
        let Ok((province, city)) = r5_province_city(cid.as_str()) else {
            continue;
        };
        if province == scope.province_code
            && city == scope.city_code
            && project_private_institution_by_cid(db, cid.as_str(), scope).await?
        {
            institutions += 1;
        }
    }
    Ok(institutions)
}

/// 联邦 drill-in 投影请求:目标省市名(前端选市时提交)。
#[derive(serde::Deserialize)]
pub(crate) struct DrillInProjectQuery {
    pub(crate) province_name: String,
    pub(crate) city_name: String,
}

/// 联邦注册局省组管理员进入本省某市 → 把该市链上全部公民 + 私权机构投影进联邦本地库。
///
/// 访问控制:仅联邦(Tier1)可调;目标省必须是该省组管理员分管的省,越省 403。
pub(crate) async fn drill_in_project_city(
    axum::extract::State(state): axum::extract::State<crate::AppState>,
    headers: axum::http::HeaderMap,
    axum::extract::Query(query): axum::extract::Query<DrillInProjectQuery>,
) -> axum::response::Response {
    use axum::http::StatusCode;
    use axum::response::IntoResponse;

    let ctx = match crate::require_admin_any(&state, &headers) {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    if !is_tier1_registry(ctx.institution_code.as_str()) {
        return crate::api_error(StatusCode::FORBIDDEN, 1003, "只有联邦注册局可以按市投影");
    }
    // 省级访问控制:目标省必须是该省组管理员分管的省。
    let scope_province = ctx.scope_province_name.as_deref().unwrap_or_default();
    if scope_province.is_empty() || scope_province != query.province_name.as_str() {
        return crate::api_error(StatusCode::FORBIDDEN, 1003, "只能进入本省下辖的市");
    }
    let Some(province_code) =
        crate::cid::china::province_code_by_name(query.province_name.as_str())
    else {
        return crate::api_error(StatusCode::BAD_REQUEST, 1001, "未知省名");
    };
    let Some(city_code) = crate::cid::china::city_code_by_name(
        query.province_name.as_str(),
        query.city_name.as_str(),
    ) else {
        return crate::api_error(StatusCode::BAD_REQUEST, 1001, "未知市名");
    };
    let scope = NodeScope {
        province_code: province_code.to_string(),
        city_code: city_code.to_string(),
    };
    match drill_in_project_scope(&state.db, &scope).await {
        Ok((citizens, institutions)) => axum::Json(crate::core::response::ApiResponse {
            code: 0,
            message: "ok".to_string(),
            data: serde_json::json!({
                "citizens_projected": citizens,
                "institutions_projected": institutions,
            }),
        })
        .into_response(),
        Err(err) => {
            tracing::warn!(error = %err, "drill-in projection failed");
            crate::api_error(StatusCode::BAD_GATEWAY, 5002, "链读投影失败(链不可达)")
        }
    }
}

#[cfg(test)]
// 投影合并夹具使用固定时间与链上记录，断言式解包仅用于测试定位。
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    fn scope() -> NodeScope {
        NodeScope {
            province_code: "GZ".to_string(),
            city_code: "018".to_string(),
        }
    }

    fn chain_voting_citizen() -> ChainCitizen {
        ChainCitizen {
            cid_number: "GZ018-CTZN1-100000000-2026".to_string(),
            province_code: "GZ".to_string(),
            city_code: "018".to_string(),
            town_code: "018001".to_string(),
            account_id: Some("0x".to_string() + &"11".repeat(32)),
            binding_revision: 1,
            binding_finalized_block_number: 7,
            binding_finalized_block_hash: "0x".to_string() + &"22".repeat(32),
            status_normal: true,
            passport_valid_from: Some("20260101".to_string()),
            passport_valid_until: Some("20360101".to_string()),
            // 投票身份链上无姓名:
            family_name: None,
            given_name: None,
            citizen_sex: None,
            birth_date: None,
            birth_province_code: None,
            birth_city_code: None,
            birth_town_code: None,
        }
    }

    fn now() -> DateTime<Utc> {
        DateTime::from_timestamp(1_782_950_400, 0).expect("valid ts")
    }

    #[test]
    fn out_of_scope_city_is_skipped() {
        let mut chain = chain_voting_citizen();
        chain.city_code = "999".to_string();
        assert!(merge_citizen_record(&chain, None, &scope(), now()).is_none());
    }

    #[test]
    fn out_of_scope_province_is_skipped() {
        let mut chain = chain_voting_citizen();
        chain.province_code = "LN".to_string();
        assert!(merge_citizen_record(&chain, None, &scope(), now()).is_none());
    }

    #[test]
    fn r5_province_city_reads_institution_scope() {
        // 机构 CID:R5 = 省2 + 市3。
        assert_eq!(
            r5_province_city("GZ018-SFGYR-201206100-2026").expect("institution scope"),
            ("GZ".to_string(), "018".to_string())
        );
    }

    #[test]
    fn r5_province_city_rejects_person_cid() {
        // 人主体 CN 号去地域化、R5 不载省市 → fail-closed,不把号段误读成区划码。
        assert!(r5_province_city("CN220-CTZN2-198805200-2026").is_err());
    }

    #[test]
    fn fresh_projection_maps_chain_fields_and_blank_offchain() {
        let out =
            merge_citizen_record(&chain_voting_citizen(), None, &scope(), now()).expect("in scope");
        assert_eq!(out.city_code, "018");
        assert_eq!(out.town_code, "018001");
        assert_eq!(out.passport_valid_from, "20260101");
        assert_eq!(out.citizen_status, CitizenStatus::Normal);
        assert!(out.voting_eligible);
        // 投票身份链上无姓名 → 空;链下正本 passport_no 空。
        assert_eq!(out.family_name, "");
        assert_eq!(out.passport_no, "");
        // 创建人回落到公民自身账户(来源锚点)。
        assert_eq!(out.creator_account_id, "0x".to_string() + &"11".repeat(32));
    }

    #[test]
    fn reprojection_preserves_offchain_original_fields() {
        // 本地已有一条注册局补录过链下正本的行(护照号 + 姓名 + 档案哈希 + 创建人)。
        let existing = merge_citizen_record(&chain_voting_citizen(), None, &scope(), now())
            .map(|mut row| {
                row.passport_no = "P12345678".to_string(); // 链下正本
                row.family_name = "王".to_string(); // 注册局补录姓名(投票身份链上仍无)
                row.given_name = "五".to_string();
                row.archive_hash = Some("hash-abc".to_string());
                row.creator_account_id = "0x".to_string() + &"ee".repeat(32);
                row.created_at = DateTime::from_timestamp(1_700_000_000, 0).unwrap();
                row
            })
            .unwrap();

        // 链上后来更新了居住地/护照(仍是投票身份,链上无姓名)。
        let mut chain = chain_voting_citizen();
        chain.town_code = "018002".to_string();
        chain.passport_valid_until = Some("20400101".to_string());

        let out = merge_citizen_record(&chain, Some(&existing), &scope(), now()).expect("in scope");
        // 链上来源列被更新:
        assert_eq!(out.town_code, "018002");
        assert_eq!(out.passport_valid_until, "20400101");
        // 链下正本绝不被覆盖:
        assert_eq!(out.passport_no, "P12345678");
        assert_eq!(out.archive_hash.as_deref(), Some("hash-abc"));
        assert_eq!(out.creator_account_id, "0x".to_string() + &"ee".repeat(32));
        assert_eq!(out.created_at, existing.created_at);
        // 投票身份链上无姓名 → 保留注册局补录的姓名,不被链上空值抹掉。
        assert_eq!(out.family_name, "王");
        assert_eq!(out.given_name, "五");
    }

    #[test]
    fn candidate_chain_name_overrides_local() {
        // 已有本地行(姓名空);链上升为竞选身份带姓名 → 用链上姓名。
        let existing =
            merge_citizen_record(&chain_voting_citizen(), None, &scope(), now()).unwrap();
        let mut chain = chain_voting_citizen();
        chain.family_name = Some("李".to_string());
        chain.given_name = Some("四".to_string());
        chain.citizen_sex = Some("MALE".to_string());
        chain.birth_date = Some("20000101".to_string());

        let out = merge_citizen_record(&chain, Some(&existing), &scope(), now()).unwrap();
        assert_eq!(out.family_name, "李");
        assert_eq!(out.given_name, "四");
        assert_eq!(out.citizen_sex, "MALE");
        assert_eq!(out.citizen_birth_date, "20000101");
    }

    #[test]
    fn revoked_status_projects_and_clears_voting_eligible() {
        let mut chain = chain_voting_citizen();
        chain.status_normal = false;
        let out = merge_citizen_record(&chain, None, &scope(), now()).unwrap();
        assert_eq!(out.citizen_status, CitizenStatus::Revoked);
        assert!(!out.voting_eligible);
    }

    #[test]
    fn finalized_missing_account_never_falls_back_to_local_account() {
        let existing =
            merge_citizen_record(&chain_voting_citizen(), None, &scope(), now()).unwrap();
        let mut chain = chain_voting_citizen();
        chain.binding_revision = 2;
        chain.account_id = None;
        chain.status_normal = false;
        let out = merge_citizen_record(&chain, Some(&existing), &scope(), now()).unwrap();
        assert_eq!(out.account_id, None);
        assert_eq!(out.binding_revision, 2);
    }

    #[test]
    fn projection_rejects_revision_regression_and_same_revision_account_change() {
        let mut existing =
            merge_citizen_record(&chain_voting_citizen(), None, &scope(), now()).unwrap();
        existing.binding_revision = 3;

        let mut regressed = chain_voting_citizen();
        regressed.binding_revision = 2;
        assert!(ensure_binding_projection_monotonic(&regressed, Some(&existing)).is_err());

        let mut conflicting = chain_voting_citizen();
        conflicting.binding_revision = 3;
        conflicting.account_id = Some("0x".to_string() + &"22".repeat(32));
        assert!(ensure_binding_projection_monotonic(&conflicting, Some(&existing)).is_err());

        let mut advanced = conflicting;
        advanced.binding_revision = 4;
        assert!(ensure_binding_projection_monotonic(&advanced, Some(&existing)).is_ok());
    }

    fn chain_institution() -> ChainInstitution {
        ChainInstitution {
            cid_number: "GZ018-SFGYR-201206100-2026".to_string(),
            province_code: "GZ".to_string(),
            city_code: "018".to_string(),
            town_code: String::new(),
            cid_full_name: "公民链技术发展基金会".to_string(),
            cid_short_name: "公民链基金会".to_string(),
            institution_code: "SFGY".to_string(),
            profit: false,
            is_private: true,
            legal_representative: Some(LegalRepresentative {
                family_name: "程".to_string(),
                given_name: "伟".to_string(),
                cid_number: "CN220-CTZN2-198805200-2026".to_string(),
                account_id: "0x".to_string() + &"9c".repeat(32),
            }),
        }
    }

    #[test]
    fn institution_out_of_scope_is_skipped() {
        let mut chain = chain_institution();
        chain.city_code = "999".to_string();
        assert!(merge_institution_record(&chain, None, &scope(), now()).is_none());
    }

    #[test]
    fn institution_fresh_projection_maps_chain_fields() {
        let out = merge_institution_record(&chain_institution(), None, &scope(), now())
            .expect("in scope");
        assert_eq!(out.category, InstitutionCategory::PrivateInstitution);
        assert_eq!(out.cid_short_name.as_deref(), Some("公民链基金会"));
        assert_eq!(out.institution_code, "SFGY");
        assert_eq!(out.has_legal_personality, Some(true));
        assert_eq!(
            out.legal_representative
                .as_ref()
                .map(|l| l.given_name.as_str()),
            Some("伟")
        );
        // 链下证件照:新建为空。
        assert!(out.legal_representative_photo_path.is_none());
        // private_type 无既有行时按机构码确定性派生(SFGY → WELFARE),否则私权列表 SQL
        // 的 `private_type IS NOT NULL` 会漏掉纯链上投影的私权机构。
        assert_eq!(out.private_type.as_deref(), Some("WELFARE"));
    }

    #[test]
    fn institution_reprojection_preserves_photo_and_local_classification() {
        // 市已补录证件照 + 私权业务分类 + 创建人。
        let existing = merge_institution_record(&chain_institution(), None, &scope(), now())
            .map(|mut inst| {
                inst.legal_representative_photo_path = Some("/photos/foundation.jpg".to_string());
                inst.legal_representative_photo_size = Some(20480);
                inst.private_type = Some("FOUNDATION".to_string());
                inst.creator_account_id = Some("0x".to_string() + &"ee".repeat(32));
                inst.created_at = DateTime::from_timestamp(1_700_000_000, 0).unwrap();
                inst
            })
            .unwrap();

        // 链上后来改了简称(全称/简称是链上来源)。
        let mut chain = chain_institution();
        chain.cid_short_name = "基金会新简称".to_string();

        let out =
            merge_institution_record(&chain, Some(&existing), &scope(), now()).expect("in scope");
        // 链上来源列被更新:
        assert_eq!(out.cid_short_name.as_deref(), Some("基金会新简称"));
        // 链下正本绝不被覆盖:
        assert_eq!(
            out.legal_representative_photo_path.as_deref(),
            Some("/photos/foundation.jpg")
        );
        assert_eq!(out.legal_representative_photo_size, Some(20480));
        assert_eq!(out.private_type.as_deref(), Some("FOUNDATION"));
        assert_eq!(
            out.creator_account_id.as_deref(),
            Some(("0x".to_string() + &"ee".repeat(32)).as_str())
        );
        assert_eq!(out.created_at, existing.created_at);
    }
}
