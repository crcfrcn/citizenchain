//! 链→注册局本地库 通用投影逻辑(M1 indexer 增量 / M3 联邦 drill-in 共用)。
//!
//! 行为契约由本模块测试和数据库 schema 共同固定。
//! 本模块拥有公民投影纯逻辑、事务写入与增量 worker；公共机构投影调用平台公开接口。
//! 因此保正本(链下正本不被覆盖)、作用域过滤、插入 vs 更新全部可离线单测,无需活链。
//!
//! 关键手法:merge 出的最终行**已把链下正本从 existing 原样带回**,故写库时即使走现有
//! `upsert_citizen_row`(ON CONFLICT DO UPDATE 全列=EXCLUDED)也不会丢正本——EXCLUDED 已是
//! 保留后的正确值。链读与写库在下方 writer 层,merge 纯逻辑离线单测。

use crate::citizens::admin_entry::CitizenWrites;
use crate::citizens::chain_identity::IdentityOperations;
use chrono::{DateTime, Utc};
use onchina::institutions::catalog::projection::{
    apply_institution_projection_conn, chain_institution_from_lookup, chain_or_local_str,
    r5_province_city, ChainInstitution, NodeScope,
};

use crate::citizens::model::{CitizenRecord, CitizenStatus};
use onchina::chain_runtime::{
    institution_lookup, institution_lookup_at, is_tier1_registry, read_chain_citizen_detail,
    read_chain_citizen_detail_at, OnChainCitizenDetail,
};
#[cfg(test)]
use onchina::codes::InstitutionCategory;
use onchina::database::Db;
#[cfg(test)]
use onchina::institutions::subjects::model::LegalRepresentative;

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
        onchina::authentication::repo::lock_node_binding_conn(&mut tx)?;
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
        onchina::authentication::repo::lock_node_binding_conn(&mut tx)?;
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

    fn matches(&self, binding: &onchina::authentication::login::NodeInstitutionBinding) -> bool {
        binding.binding_id == self.binding_id
            && binding.institution_cid_number == self.institution_cid_number
            && onchina::chain_runtime::is_subordinate_registry(&binding.institution_code)
    }
}

/// 只有下级注册局使用常驻实体投影；联邦按市查看仍走既有 drill-in。
pub(crate) fn projection_binding(db: &Db) -> Result<Option<ProjectionBinding>, String> {
    let Some(binding) = onchina::authentication::repo::active_node_binding(db)? else {
        return Ok(None);
    };
    if !onchina::chain_runtime::is_subordinate_registry(&binding.institution_code) {
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
        if onchina::chain_submit::parse_hex32(&hash).is_none() {
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
    onchina::chain_runtime::for_each_chain_citizen_cid_in_scope_at(
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
    onchina::chain_runtime::for_each_chain_private_institution_cid_at(Some(block_hash), |cid| {
        all_institutions.push(cid)
    })
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
        onchina::authentication::repo::lock_node_binding_conn(&mut tx)?;
        let active = onchina::authentication::repo::get_active_node_binding_conn(&mut tx)?;
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
    onchina::chain_runtime::for_each_chain_citizen_cid_in_scope(
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
    onchina::chain_runtime::for_each_chain_private_institution_cid(|cid| inst_cids.push(cid))
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
    axum::extract::State(state): axum::extract::State<onchina::AppState>,
    headers: axum::http::HeaderMap,
    axum::extract::Query(query): axum::extract::Query<DrillInProjectQuery>,
) -> axum::response::Response {
    use axum::http::StatusCode;
    use axum::response::IntoResponse;

    let ctx = match onchina::require_admin_any(&state, &headers) {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    if !is_tier1_registry(ctx.institution_code.as_str()) {
        return onchina::api_error(StatusCode::FORBIDDEN, 1003, "只有联邦注册局可以按市投影");
    }
    // 省级访问控制:目标省必须是该省组管理员分管的省。
    let scope_province = ctx.scope_province_name.as_deref().unwrap_or_default();
    if scope_province.is_empty() || scope_province != query.province_name.as_str() {
        return onchina::api_error(StatusCode::FORBIDDEN, 1003, "只能进入本省下辖的市");
    }
    let Some(province_code) =
        onchina::codes::china::province_code_by_name(query.province_name.as_str())
    else {
        return onchina::api_error(StatusCode::BAD_REQUEST, 1001, "未知省名");
    };
    let Some(city_code) = onchina::codes::china::city_code_by_name(
        query.province_name.as_str(),
        query.city_name.as_str(),
    ) else {
        return onchina::api_error(StatusCode::BAD_REQUEST, 1001, "未知市名");
    };
    let scope = NodeScope {
        province_code: province_code.to_string(),
        city_code: city_code.to_string(),
    };
    match drill_in_project_scope(&state.db, &scope).await {
        Ok((citizens, institutions)) => axum::Json(onchina::response::ApiResponse {
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
            onchina::api_error(StatusCode::BAD_GATEWAY, 5002, "链读投影失败(链不可达)")
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
#[path = "../tests/projection.rs"]
mod tests;

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
    use crate::projection;
    let Some(binding) = projection::projection_binding(db_pool)? else {
        return Ok(());
    };
    let ws_url = onchina::chain_url::chain_ws_url()?;
    let rpc_client = onchina::chain_url::rpc_client(&ws_url).await?;
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
            onchina::indexer::event_parser::collect_entity_projection_cids(&events)?;
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

#[cfg(test)]
use onchina::institutions::catalog::projection::merge_institution_record;
