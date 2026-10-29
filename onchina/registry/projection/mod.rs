//! 链→注册局本地库 通用投影逻辑(M1 indexer 增量 / M3 联邦 drill-in 共用)。
//!
//! 行为契约由本模块测试和数据库 schema 共同固定。
//! 本模块拥有公民投影纯逻辑、事务写入与增量 worker；公共机构投影调用平台公开接口。
//! 因此保正本(链下正本不被覆盖)、作用域过滤、插入 vs 更新全部可离线单测,无需活链。
//!
//! 关键手法:merge 出的最终行**已把链下正本从 existing 原样带回**,故写库时即使走现有
//! `upsert_citizen_row`(ON CONFLICT DO UPDATE 全列=EXCLUDED)也不会丢正本——EXCLUDED 已是
//! 保留后的正确值。链读与写库在下方 writer 层,merge 纯逻辑离线单测。

use crate::institutions::catalog::projection::{
    apply_institution_projection_conn, chain_institution_from_lookup, chain_or_local_str,
    merge_institution_record, r5_province_city, resolve_node_scope, ChainInstitution, NodeScope,
};
use chrono::{DateTime, Utc};

use crate::cid::InstitutionCategory;
use crate::core::chain_runtime::{
    institution_lookup, institution_lookup_at, is_tier1_registry, read_chain_citizen_detail,
    read_chain_citizen_detail_at, OnChainCitizenDetail,
};
use crate::core::db::Db;
use crate::institutions::subjects::model::LegalRepresentative;
use crate::institutions::subjects::Institution;
use crate::registry::citizens::model::{CitizenRecord, CitizenStatus};

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
    pub(crate) binding_block_number: i64,
    pub(crate) binding_block_hash: String,
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

/// 新公民仅接收本范围；已有公民迁出时仍更新链上地域，使旧范围查询立即排除。
///
/// - **作用域**:新行按当前 residence 归属；已有行迁出仍保留正本并刷新地域，不复制或删除附件。
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
    if existing.is_none()
        && (chain.province_code != scope.province_code || chain.city_code != scope.city_code)
    {
        return None;
    }

    let citizen_status = if chain.status_normal {
        CitizenStatus::Normal
    } else {
        CitizenStatus::Revoked
    };

    Some(CitizenRecord {
        id: existing.map(|e| e.id).unwrap_or(0),
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
        binding_block_number: Some(chain.binding_block_number),
        binding_block_hash: Some(chain.binding_block_hash.clone()),
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

// ───────────────────────── 链读映射 + writer(接线层) ─────────────────────────

/// 链日期只接受真实日历日；None 仍表示该身份尚无该日期。
pub(crate) fn chain_date(value: Option<u32>) -> Result<Option<String>, String> {
    value
        .map(|value| {
            chrono::NaiveDate::from_ymd_opt((value / 10_000) as i32, value / 100 % 100, value % 100)
                .map(|date| date.format("%Y-%m-%d").to_string())
                .ok_or_else(|| "invalid chain calendar date".to_string())
        })
        .transpose()
}

fn chain_citizen_from_detail(d: OnChainCitizenDetail) -> Result<ChainCitizen, String> {
    let (family_name, given_name, citizen_sex, birth_date, bp, bc, bt) = match d.candidate {
        Some(c) => (
            Some(String::from_utf8_lossy(&c.family_name).into_owned()),
            Some(String::from_utf8_lossy(&c.given_name).into_owned()),
            Some(if c.citizen_sex == 0 { "MALE" } else { "FEMALE" }.to_string()),
            chain_date(Some(c.birth_date))?,
            Some(c.birth_province_code),
            Some(c.birth_city_code),
            Some(c.birth_town_code),
        ),
        None => (None, None, None, None, None, None, None),
    };
    Ok(ChainCitizen {
        cid_number: d.cid_number,
        province_code: d.residence_province_code,
        city_code: d.residence_city_code,
        town_code: d.residence_town_code,
        account_id: d.account_id.map(|a| format!("0x{}", hex::encode(a))),
        binding_revision: d.binding_revision,
        binding_block_number: i64::from(d.binding_block_number),
        binding_block_hash: format!("0x{}", hex::encode(d.binding_block_hash)),
        status_normal: d.status_normal,
        passport_valid_from: chain_date(d.passport_valid_from)?,
        passport_valid_until: chain_date(d.passport_valid_until)?,
        family_name,
        given_name,
        citizen_sex,
        birth_date,
        birth_province_code: bp,
        birth_city_code: bc,
        birth_town_code: bt,
    })
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
    let chain = chain_citizen_from_detail(detail)?;
    db.with_client(move |conn| {
        let mut tx = conn
            .transaction()
            .map_err(|e| format!("begin citizen projection failed: {e}"))?;
        crate::authentication::repo::lock_node_binding_conn(&mut tx)?;
        let changed = apply_citizen_projection_conn(&mut tx, &chain, scope)?;
        tx.commit()
            .map_err(|e| format!("commit citizen projection failed: {e}"))?;
        Ok(changed)
    })
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
    db.with_client(move |conn| {
        let mut tx = conn
            .transaction()
            .map_err(|e| format!("begin institution projection failed: {e}"))?;
        crate::authentication::repo::lock_node_binding_conn(&mut tx)?;
        let changed = apply_institution_projection_conn(&mut tx, &chain, scope)?;
        tx.commit()
            .map_err(|e| format!("commit institution projection failed: {e}"))?;
        Ok(changed)
    })
}

/// 网络读取与数据库写入分离；所有实体都读取同一个 finalized 区块。
pub(crate) struct ProjectionBatch {
    citizens: Vec<ChainCitizen>,
    institutions: Vec<ChainInstitution>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProjectionBinding {
    pub(crate) binding_id: String,
    pub(crate) institution_cid_number: String,
    pub(crate) scope: NodeScope,
}

impl ProjectionBinding {
    pub(crate) fn key(&self, genesis_hash: &str) -> String {
        format!(
            "registry-entities:{genesis_hash}:{}:{}:{}",
            self.institution_cid_number, self.scope.province_code, self.scope.city_code
        )
    }

    fn matches(&self, binding: &crate::authentication::login::NodeInstitutionBinding) -> bool {
        binding.binding_id == self.binding_id
            && binding.institution_cid_number == self.institution_cid_number
            && crate::core::chain_runtime::is_subordinate_registry(&binding.institution_code)
    }
}

/// 只有下级注册局使用常驻实体投影；联邦按市查看仍走既有 drill-in。
pub(crate) fn projection_binding(db: &Db) -> Result<Option<ProjectionBinding>, String> {
    let Some(binding) = crate::authentication::repo::active_node_binding(db)? else {
        return Ok(None);
    };
    if !crate::core::chain_runtime::is_subordinate_registry(&binding.institution_code) {
        return Ok(None);
    }
    let (province_code, city_code) = r5_province_city(&binding.institution_cid_number)?;
    Ok(Some(ProjectionBinding {
        binding_id: binding.binding_id,
        institution_cid_number: binding.institution_cid_number,
        scope: NodeScope {
            province_code,
            city_code,
        },
    }))
}

pub(crate) fn projection_cursor(
    db: &Db,
    key: &str,
    genesis: &str,
) -> Result<Option<(i64, String)>, String> {
    db.with_client(|conn| read_projection_cursor_conn(conn, key, genesis))
}

fn read_projection_cursor_conn(
    conn: &mut impl postgres::GenericClient,
    key: &str,
    genesis: &str,
) -> Result<Option<(i64, String)>, String> {
    let row = conn.query_opt("SELECT block_number,block_hash FROM projection WHERE projection_key=$1 AND genesis_hash=$2 AND state_code='OK'", &[&key, &genesis])
        .map_err(|e| format!("read registry projection cursor failed: {e}"))?;
    row.map(|row| {
        let number = row
            .get::<_, Option<i64>>(0)
            .filter(|number| *number >= 0)
            .ok_or_else(|| "invalid entity projection cursor number".to_string())?;
        let hash: String = row.get(1);
        if crate::core::chain_submit::parse_hex32(&hash).is_none() {
            return Err("invalid entity projection cursor hash".to_string());
        }
        Ok((number, hash))
    })
    .transpose()
}

pub(crate) async fn prepare_projection_batch(
    citizen_cids: &[String],
    institution_cids: &[String],
    block_hash: [u8; 32],
) -> Result<ProjectionBatch, String> {
    let mut batch = ProjectionBatch {
        citizens: Vec::new(),
        institutions: Vec::new(),
    };
    for cid in citizen_cids {
        if let Some(detail) = read_chain_citizen_detail_at(cid, Some(block_hash)).await? {
            batch.citizens.push(chain_citizen_from_detail(detail)?);
        }
    }
    for cid in institution_cids {
        if let Some(detail) = institution_lookup_at(cid, Some(block_hash)).await? {
            let institution = chain_institution_from_lookup(cid, detail)?;
            if institution.is_private {
                batch.institutions.push(institution);
            }
        }
    }
    Ok(batch)
}

pub(crate) async fn prepare_projection_snapshot(
    db: &Db,
    scope: &NodeScope,
    block_hash: [u8; 32],
) -> Result<ProjectionBatch, String> {
    let mut citizens = Vec::new();
    crate::core::chain_runtime::for_each_chain_citizen_cid_in_scope_at(
        &scope.province_code,
        &scope.city_code,
        Some(block_hash),
        |cid| citizens.push(cid),
    )
    .await?;
    let local = db.with_client(|conn| {
        conn.query(
            "SELECT cid_number FROM citizen WHERE province_code=$1 AND city_code=$2",
            &[&scope.province_code, &scope.city_code],
        )
        .map(|rows| {
            rows.iter()
                .map(|row| row.get::<_, String>(0))
                .collect::<Vec<_>>()
        })
        .map_err(|e| e.to_string())
    })?;
    citizens.extend(local);
    citizens.sort();
    citizens.dedup();
    let mut all_institutions = Vec::new();
    crate::core::chain_runtime::for_each_chain_private_institution_cid_at(
        Some(block_hash),
        |cid| all_institutions.push(cid),
    )
    .await?;
    let mut institutions = Vec::new();
    for cid in all_institutions {
        let (province, city) = r5_province_city(&cid)?;
        if province == scope.province_code && city == scope.city_code {
            institutions.push(cid);
        }
    }
    prepare_projection_batch(&citizens, &institutions, block_hash).await
}

fn apply_citizen_projection_conn(
    conn: &mut impl postgres::GenericClient,
    chain: &ChainCitizen,
    scope: &NodeScope,
) -> Result<bool, String> {
    Db::lock_citizen_conn(conn, &chain.cid_number)?;
    let existing = Db::find_citizen_by_cid_conn(conn, &chain.cid_number)?;
    // 用户刚完成的较新链交易已经落库时，历史追块不能把它退回旧块。
    if existing
        .as_ref()
        .and_then(|row| row.binding_block_number)
        .is_some_and(|number| number > chain.binding_block_number)
    {
        return Ok(false);
    }
    ensure_binding_projection_monotonic(chain, existing.as_ref())?;
    let Some(row) = merge_citizen_record(chain, existing.as_ref(), scope, Utc::now()) else {
        return Ok(false);
    };
    Db::upsert_target_citizen_rows(conn, &row)?;
    Ok(true)
}

/// 数据与进度原子提交。旧绑定、重复批次或任一实体失败均不能推进游标。
pub(crate) fn commit_projection_batch(
    db: &Db,
    binding: &ProjectionBinding,
    genesis: &str,
    previous: Option<i64>,
    block_number: i64,
    block_hash: &str,
    batch: &ProjectionBatch,
) -> Result<bool, String> {
    let key = binding.key(genesis);
    db.with_client(|conn| {
        let mut tx = conn.transaction().map_err(|e| format!("begin entity projection failed: {e}"))?;
        crate::authentication::repo::lock_node_binding_conn(&mut tx)?;
        let active = crate::authentication::repo::get_active_node_binding_conn(&mut tx)?;
        if !active.as_ref().is_some_and(|active| binding.matches(active)) { return Ok(false); }
        let current = read_projection_cursor_conn(&mut tx, &key, genesis)?.map(|(number, _)| number);
        if current != previous || !valid_projection_advance(previous, block_number) { return Err("entity projection cursor changed or noncontiguous".to_string()); }
        for citizen in &batch.citizens { apply_citizen_projection_conn(&mut tx, citizen, &binding.scope)?; }
        for institution in &batch.institutions { apply_institution_projection_conn(&mut tx, institution, &binding.scope)?; }
        tx.execute("INSERT INTO projection(projection_key,genesis_hash,block_hash,block_number,item_count,account_count,state_code,synced_at) VALUES($1,$2,$3,$4,$5,0,'OK',now()) ON CONFLICT(projection_key) DO UPDATE SET genesis_hash=EXCLUDED.genesis_hash,block_hash=EXCLUDED.block_hash,block_number=EXCLUDED.block_number,item_count=EXCLUDED.item_count,state_code='OK',synced_at=now()",
            &[&key, &genesis, &block_hash, &block_number, &i64::try_from(batch.citizens.len()+batch.institutions.len()).map_err(|_| "entity count exceeds i64")?])
            .map_err(|e| format!("save entity projection cursor failed: {e}"))?;
        tx.commit().map_err(|e| format!("commit entity projection failed: {e}"))?;
        Ok(true)
    })
}

fn valid_projection_advance(previous: Option<i64>, next: i64) -> bool {
    next >= 0 && previous.map_or(true, |previous| previous.checked_add(1) == Some(next))
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
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
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
                "citizensProjected": citizens,
                "institutionsProjected": institutions,
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
            cid_number: format!(
                "CN000-CTZN{}-100000000-2026",
                primitives::cid::number::checksum_char_m1("CN000CTZN1000000002026", true)
            ),
            province_code: "GZ".to_string(),
            city_code: "018".to_string(),
            town_code: "018001".to_string(),
            account_id: Some("0x".to_string() + &"11".repeat(32)),
            binding_revision: 1,
            binding_block_number: 7,
            binding_block_hash: "0x".to_string() + &"22".repeat(32),
            status_normal: true,
            passport_valid_from: Some("2026-01-01".to_string()),
            passport_valid_until: Some("2036-01-01".to_string()),
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
    fn existing_citizen_move_out_refreshes_scope_and_preserves_originals() {
        let chain = chain_voting_citizen();
        let mut existing = merge_citizen_record(&chain, None, &scope(), now()).unwrap();
        existing.id = 17;
        existing.family_name = "保留".into();
        existing.archive_hash = Some("original".into());
        existing.passport_no = "original-passport".into();
        let mut moved = chain.clone();
        moved.province_code = "GD".into();
        moved.city_code = "001".into();
        moved.binding_block_number += 1;
        assert!(merge_citizen_record(&moved, None, &scope(), now()).is_none());
        let refreshed = merge_citizen_record(&moved, Some(&existing), &scope(), now()).unwrap();
        assert_eq!(refreshed.province_code, "GD");
        assert_eq!(refreshed.city_code, "001");
        assert_eq!(refreshed.id, 17);
        assert_eq!(refreshed.family_name, "保留");
        assert_eq!(refreshed.archive_hash, existing.archive_hash);
        assert_eq!(refreshed.passport_no, existing.passport_no);
        assert!(merge_citizen_record(
            &moved,
            None,
            &NodeScope {
                province_code: "GD".into(),
                city_code: "001".into()
            },
            now()
        )
        .is_some());
    }

    #[test]
    #[ignore = "由控制台交付本机 onchina_test 数据库后执行隔离 PostgreSQL 集成回归"]
    fn entity_failure_rolls_back_rows_and_cursor_and_old_binding_cannot_write() {
        use crate::authentication::{login::NodeInstitutionBinding, repo};
        let sandbox = crate::core::db::IsolatedTestDb::new();
        let institution = format!(
            "GZ018-CREG{}-000000001-2026",
            primitives::cid::number::checksum_char_m1("GZ018CREG0000000012026", false)
        );
        let binding = NodeInstitutionBinding {
            binding_id: "binding-a".into(),
            candidate_id: "candidate-a".into(),
            institution_code: "CREG".into(),
            institution_cid_number: institution,
            bound_account_id: format!("0x{}", "11".repeat(32)),
            bound_at: now(),
            status: "ACTIVE".into(),
        };
        sandbox
            .db
            .with_client(|conn| repo::upsert_active_node_binding_conn(conn, &binding))
            .unwrap();
        let binding = projection_binding(&sandbox.db).unwrap().unwrap();
        let hash = format!("0x{}", "22".repeat(32));
        let chain = chain_voting_citizen();
        let batch = ProjectionBatch {
            citizens: vec![chain.clone()],
            institutions: vec![],
        };
        assert!(
            commit_projection_batch(&sandbox.db, &binding, "chain-a", None, 7, &hash, &batch)
                .unwrap()
        );
        let original = sandbox
            .db
            .find_citizen_by_cid(&chain.cid_number)
            .unwrap()
            .unwrap();
        assert!(original.id > 0 && original.id <= i64::MAX as u64);
        sandbox.db.with_client(|conn| { conn.execute("UPDATE citizen SET family_name='保留',archive_hash='local-original' WHERE cid_number=$1", &[&chain.cid_number]).unwrap(); Ok(()) }).unwrap();
        let mut first = chain.clone();
        first.cid_number = format!(
            "CN000-CTZN{}-100000001-2026",
            primitives::cid::number::checksum_char_m1("CN000CTZN1000000012026", true)
        );
        first.account_id = Some(format!("0x{}", "33".repeat(32)));
        first.binding_block_number = 8;
        let mut bad = chain.clone();
        bad.cid_number = format!(
            "CN000-CTZN{}-100000002-2026",
            primitives::cid::number::checksum_char_m1("CN000CTZN1000000022026", true)
        );
        bad.account_id = Some(format!("0x{}", "44".repeat(32)));
        bad.binding_block_number = 8;
        bad.binding_revision = u64::MAX;
        let failed = ProjectionBatch {
            citizens: vec![first.clone(), bad],
            institutions: vec![],
        };
        assert!(commit_projection_batch(
            &sandbox.db,
            &binding,
            "chain-a",
            Some(7),
            8,
            &hash,
            &failed
        )
        .is_err());
        assert!(sandbox
            .db
            .find_citizen_by_cid(&first.cid_number)
            .unwrap()
            .is_none());
        assert_eq!(
            projection_cursor(&sandbox.db, &binding.key("chain-a"), "chain-a")
                .unwrap()
                .unwrap()
                .0,
            7
        );
        assert!(commit_projection_batch(
            &sandbox.db,
            &binding,
            "chain-a",
            Some(7),
            8,
            &hash,
            &batch
        )
        .unwrap());
        let reread = sandbox
            .db
            .find_citizen_by_cid(&chain.cid_number)
            .unwrap()
            .unwrap();
        assert_eq!(reread.id, original.id);
        assert_eq!(reread.family_name, "保留");
        assert_eq!(reread.archive_hash.as_deref(), Some("local-original"));
        assert!(commit_projection_batch(
            &sandbox.db,
            &binding,
            "chain-a",
            Some(7),
            8,
            &hash,
            &batch
        )
        .is_err());
        sandbox
            .db
            .with_client(|conn| repo::deactivate_active_node_binding_conn(conn).map(|_| ()))
            .unwrap();
        assert!(!commit_projection_batch(
            &sandbox.db,
            &binding,
            "chain-a",
            Some(8),
            9,
            &hash,
            &batch
        )
        .unwrap());
        assert_eq!(
            projection_cursor(&sandbox.db, &binding.key("chain-a"), "chain-a")
                .unwrap()
                .unwrap()
                .0,
            8
        );
    }

    #[test]
    fn projection_cursor_requires_contiguous_success_and_is_scoped() {
        assert!(valid_projection_advance(None, 100));
        assert!(valid_projection_advance(Some(100), 101));
        for next in [-1, 99, 100, 102] {
            assert!(!valid_projection_advance(Some(100), next));
        }
        assert!(!valid_projection_advance(Some(i64::MAX), i64::MAX));
        let binding = ProjectionBinding {
            binding_id: "binding-a".into(),
            institution_cid_number: format!(
                "GZ018-CREG{}-000000001-2026",
                primitives::cid::number::checksum_char_m1("GZ018CREG0000000012026", false)
            ),
            scope: scope(),
        };
        assert_ne!(binding.key("chain-a"), binding.key("chain-b"));
        let other = ProjectionBinding {
            institution_cid_number: "another".into(),
            ..binding.clone()
        };
        assert_ne!(binding.key("chain-a"), other.key("chain-a"));
    }

    #[test]
    fn chain_dates_validate_calendar_and_keep_missing_values() {
        assert_eq!(chain_date(None).unwrap(), None);
        assert_eq!(
            chain_date(Some(20240229)).unwrap().as_deref(),
            Some("2024-02-29")
        );
        for value in [0, 20230229, 20261301, 20260100, 20260431] {
            assert!(chain_date(Some(value)).is_err());
        }
        let row = merge_citizen_record(&chain_voting_citizen(), None, &scope(), now()).unwrap();
        assert_eq!(
            row.computed_identity_status_on_date(
                chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap()
            ),
            CitizenStatus::Normal
        );
        assert_eq!(
            row.computed_identity_status_on_date(
                chrono::NaiveDate::from_ymd_opt(2036, 1, 2).unwrap()
            ),
            CitizenStatus::Revoked
        );
        assert_eq!(row.id, 0); // 新记录的真实 ID 只由数据库正式写入分配。
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
        assert_eq!(out.passport_valid_from, "2026-01-01");
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
        chain.passport_valid_until = Some("2040-01-01".to_string());

        let out = merge_citizen_record(&chain, Some(&existing), &scope(), now()).expect("in scope");
        // 链上来源列被更新:
        assert_eq!(out.town_code, "018002");
        assert_eq!(out.passport_valid_until, "2040-01-01");
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
        chain.birth_date = Some("2000-01-01".to_string());

        let out = merge_citizen_record(&chain, Some(&existing), &scope(), now()).unwrap();
        assert_eq!(out.family_name, "李");
        assert_eq!(out.given_name, "四");
        assert_eq!(out.citizen_sex, "MALE");
        // 链上日期已映射为标准日历格式，投影保持该格式。
        assert_eq!(out.citizen_birth_date, "2000-01-01");
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
        let dto = serde_json::to_value(&out).unwrap();
        assert_eq!(dto["status"], "ACTIVE");
        assert_eq!(dto["chainStatus"], "ACTIVE_ON_CHAIN");
        assert_eq!(dto["subjectProperty"], "S");
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
        assert!(out.photo_path.is_none());
        // private_type 无既有行时按机构码确定性派生(SFGY → WELFARE),否则私权列表 SQL
        // 的 `private_type IS NOT NULL` 会漏掉纯链上投影的私权机构。
        assert_eq!(out.private_type.as_deref(), Some("WELFARE"));
    }

    #[test]
    fn institution_reprojection_preserves_photo_and_local_classification() {
        // 市已补录证件照 + 私权业务分类 + 创建人。
        let existing = merge_institution_record(&chain_institution(), None, &scope(), now())
            .map(|mut inst| {
                inst.photo_path = Some("/photos/foundation.jpg".to_string());
                inst.photo_size = Some(20480);
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
        assert_eq!(out.photo_path.as_deref(), Some("/photos/foundation.jpg"));
        assert_eq!(out.photo_size, Some(20480));
        assert_eq!(out.private_type.as_deref(), Some("FOUNDATION"));
        assert_eq!(
            out.creator_account_id.as_deref(),
            Some(("0x".to_string() + &"ee".repeat(32)).as_str())
        );
        assert_eq!(out.created_at, existing.created_at);
    }
}

/// 注册局既有创世档案投影，保留原有调用时机与幂等行为。
#[path = "genesis.rs"]
pub(crate) mod genesis;

pub(crate) async fn entity_projection_worker(db_pool: Db) {
    loop {
        if let Err(error) = project_entities_once(&db_pool).await {
            tracing::warn!(error = %error, "registry entity projection paused; cursor retained for retry");
        }
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
    }
}
async fn project_entities_once(db_pool: &Db) -> Result<(), String> {
    use crate::registry::projection;
    let Some(binding) = projection::projection_binding(db_pool)? else {
        return Ok(());
    };
    let ws_url = crate::core::chain_url::chain_ws_url()?;
    let rpc_client = crate::core::chain_url::rpc_client(&ws_url).await?;
    let rpc =
        subxt::backend::legacy::LegacyRpcMethods::<subxt::PolkadotConfig>::new(rpc_client.clone());
    let client = subxt::OnlineClient::<subxt::PolkadotConfig>::from_rpc_client(rpc_client)
        .await
        .map_err(|e| format!("connect entity projection failed: {e}"))?;
    let genesis = format!("{:#x}", client.genesis_hash());
    let head_hash = rpc
        .chain_get_finalized_head()
        .await
        .map_err(|e| format!("read entity finalized head failed: {e}"))?;
    let head = rpc
        .chain_get_header(Some(head_hash))
        .await
        .map_err(|e| format!("read entity finalized header failed: {e}"))?
        .ok_or("entity finalized header missing")?;
    let latest = i64::from(head.number);
    let mut cursor = match projection::projection_cursor(db_pool, &binding.key(&genesis), &genesis)?
    {
        Some(cursor) => cursor,
        None => {
            // 初始化/补漏只读固定 finalized 快照，不重写交易流水，也不清除链下正本。
            let batch =
                projection::prepare_projection_snapshot(db_pool, &binding.scope, head_hash.0)
                    .await?;
            projection::commit_projection_batch(
                db_pool,
                &binding,
                &genesis,
                None,
                latest,
                &format!("{head_hash:#x}"),
                &batch,
            )?;
            return Ok(());
        }
    };
    if cursor.0 > latest {
        return Err("entity projection cursor is ahead of finalized chain".into());
    }
    // 每轮最多 64 块，绑定改变时尽快交回下一轮；每块都在事务内重核绑定。
    let end = latest.min(cursor.0.saturating_add(64));
    while cursor.0 < end {
        let number = cursor.0 + 1;
        let hash = rpc
            .chain_get_block_hash(Some(
                (u32::try_from(number).map_err(|_| "entity block exceeds u32")?).into(),
            ))
            .await
            .map_err(|e| format!("read entity block hash failed: {e}"))?
            .ok_or("entity block hash missing")?;
        let block = client
            .blocks()
            .at(hash)
            .await
            .map_err(|e| format!("read entity block failed: {e}"))?;
        if format!("{:#x}", block.header().parent_hash) != cursor.1 {
            return Err("entity projection parent hash mismatch".into());
        }
        let events = block
            .events()
            .await
            .map_err(|e| format!("read entity events failed: {e}"))?;
        let (citizens, institutions) =
            crate::indexer::event_parser::collect_entity_projection_cids(&events)?;
        let batch = projection::prepare_projection_batch(&citizens, &institutions, hash.0).await?;
        let hash_text = format!("{hash:#x}");
        if !projection::commit_projection_batch(
            db_pool,
            &binding,
            &genesis,
            Some(cursor.0),
            number,
            &hash_text,
            &batch,
        )? {
            return Ok(());
        }
        cursor = (number, hash_text);
    }
    Ok(())
}
