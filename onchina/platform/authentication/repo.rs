//! 管理员结构化表读写。
//!
//! 管理员、登录签名请求、会话和安全动作状态全部以数据库表为唯一持久化真源。

use chrono::{DateTime, Duration, Utc};
use sha2::{Digest, Sha256};

use crate::authentication::login::{
    AdminInstitutionCandidate, AdminSession, LoginSignRequest, NodeBindingChallenge,
    NodeInstitutionBinding, QrLoginResultRecord,
};
use crate::authentication::model::AdminUser;
use crate::authentication::security_model::{AdminActionChallenge, AdminSecurityGrant};
use crate::core::db::postgres_error_text;
use crate::Db;
use primitives::cid::number::cid_scope_codes;

pub(crate) fn admin_from_row(row: &postgres::Row) -> Result<AdminUser, String> {
    let id: i64 = row.get(0);
    Ok(AdminUser {
        id: u64::try_from(id).ok().filter(|id| *id > 0).ok_or("invalid stored admin id")?,
        account_id: row.get(1),
        family_name: row.get(2),
        given_name: row.get(3),
        institution_code: row.get(4),
        built_in: row.get(5),
        creator_account_id: row.get(6),
        created_at: row.get(7),
        updated_at: row.get(8),
        city_name: row.get(9),
        institution_cid_number: row.get(10),
    })
}

fn binding_from_row(row: &postgres::Row) -> Result<NodeInstitutionBinding, String> {
    Ok(NodeInstitutionBinding {
        binding_id: row.get(0),
        candidate_id: row.get(1),
        institution_code: row.get(2),
        institution_cid_number: row.get(3),
        bound_account_id: row.get(4),
        bound_at: row.get(5),
        status: row.get(6),
    })
}

// Tier1 创世注册局管理员「全走链读」：管理员账户 ID 来自 FRG AdminAccounts，
// 省维度只来自 InstitutionRoleAssignments；本地不得建立权限缓存。







pub(crate) fn get_admin_by_account_id(
    db: &Db,
    account_id: &str,
) -> Result<Option<AdminUser>, String> {
    let account_id = account_id.trim().to_string();
    db.with_client(move |conn| get_admin_by_account_id_conn(conn, account_id.as_str()))
}

pub(crate) fn get_admin_by_account_id_conn(
    conn: &mut impl postgres::GenericClient,
    account_id: &str,
) -> Result<Option<AdminUser>, String> {
    let row = conn
        .query_opt(
            "SELECT admin_id, account_id, family_name, given_name, institution_code, built_in, creator_account_id, created_at, updated_at, city_name, institution_cid_number
             FROM admins
             WHERE account_id = $1",
            &[&account_id],
        )
        .map_err(|e| format!("query admin by account failed: {e}"))?;
    row.as_ref().map(admin_from_row).transpose()
}

/// 管理员授权行政作用域固定为省、市、镇三级可选名称。
type AdminScope = (Option<String>, Option<String>, Option<String>);
/// 节点绑定候选元数据固定为 CID、全称、简称和省市镇代码。
type BindingCandidateMetadata = (
    String,
    Option<String>,
    Option<String>,
    String,
    String,
    String,
);

/// 派生管理员的省/市/镇作用域。登录签发与会话重建共用此唯一入口：
/// FRG 在连接池外读取当前账户的链上任职；本函数只解析其它机构的行政区元数据。
/// 其它机构按绑定 CID 的机构行政区投影解析。绑定缺失或机构不一致一律失败关闭。
pub(crate) fn derive_admin_scope_conn(
    conn: &mut impl postgres::GenericClient,
    account_id: &str,
    institution_code: &str,
) -> Result<AdminScope, String> {
    let Some(admin) = get_admin_by_account_id_conn(conn, account_id)? else {
        return Err("admin not found while deriving authorization scope".to_string());
    };
    if admin.institution_code != institution_code {
        return Err("admin institution does not match requested authorization scope".to_string());
    }
    let Some(binding) = get_active_node_binding_conn(conn)? else {
        return Err("active node binding is required for authorization scope".to_string());
    };
    if binding.institution_code != institution_code {
        return Err("active node binding institution mismatch".to_string());
    }
    if crate::core::chain_runtime::is_tier1_registry(institution_code) { return Ok((None, None, None)); }
    authorization_scope_from_identity_conn(
        conn,
        binding.institution_code.as_str(),
        binding.institution_cid_number.as_str(),
        None,
    )
}

/// 解析当前管理员所属机构的 cid_short_name 单一字段。
/// 联邦注册局管理员 → institution_code='FRG' 的全局唯一机构(总统府联邦注册局,简称=联邦注册局);
/// 市注册局管理员   → institution_code='CREG' AND province_name AND city_name 的本市机构(如 合肥市注册局)。
/// 无对应行返回 None(前端按空处理,绝不另造名字)。
pub(crate) fn resolve_home_cid_short_name_conn(
    conn: &mut impl postgres::GenericClient,
    institution_code: &str,
    scope_province_name: Option<&str>,
    scope_city_name: Option<&str>,
) -> Result<Option<String>, String> {
    let row = if crate::core::chain_runtime::is_tier1_registry(institution_code) {
        // Tier1 创世注册局全局唯一,直接按机构码取其机构简称。
        conn.query_opt(
            "SELECT cid_short_name FROM subjects \
             WHERE institution_code = $1 AND status = 'ACTIVE' LIMIT 1",
            &[&institution_code],
        )
        .map_err(|e| format!("query federal registry short name failed: {e}"))?
    } else if crate::core::chain_runtime::admin_level_label_for(institution_code).as_deref()
        == Some("NATIONAL")
    {
        // NJD 等全国级机构没有省市作用域,按机构码直接解析本机构简称。
        conn.query_opt(
            "SELECT cid_short_name FROM subjects \
             WHERE institution_code = $1 AND status = 'ACTIVE' LIMIT 1",
            &[&institution_code],
        )
        .map_err(|e| format!("query national institution short name failed: {e}"))?
    } else {
        // 市级机构按本机构码 + 省 + 市定位机构简称。
        // subjects 已不存行政区名字,按 china.sqlite 把省/市名字派生成 code 再过滤(单源)。
        let (Some(province), Some(city)) = (scope_province_name, scope_city_name) else {
            return Ok(None);
        };
        let (Some(province_code), Some(city_code)) = (
            crate::cid::china::province_code_by_name(province),
            crate::cid::china::city_code_by_name(province, city),
        ) else {
            return Ok(None);
        };
        conn.query_opt(
            "SELECT cid_short_name FROM subjects \
             WHERE institution_code = $1 AND status = 'ACTIVE' \
               AND province_code = $2 AND city_code = $3 LIMIT 1",
            &[&institution_code, &province_code, &city_code],
        )
        .map_err(|e| format!("query institution short name failed: {e}"))?
    };
    Ok(row.and_then(|r| r.get::<usize, Option<String>>(0)))
}

pub(crate) fn resolve_home_cid_short_name(
    db: &Db,
    institution_code: &str,
    scope_province_name: Option<&str>,
    scope_city_name: Option<&str>,
) -> Result<Option<String>, String> {
    let institution_code = institution_code.to_string();
    let province = scope_province_name.map(str::to_string);
    let city = scope_city_name.map(str::to_string);
    db.with_client(move |conn| {
        resolve_home_cid_short_name_conn(
            conn,
            institution_code.as_str(),
            province.as_deref(),
            city.as_deref(),
        )
    })
}

pub(crate) fn resolve_binding_candidate_metadata_conn(
    conn: &mut impl postgres::GenericClient,
    cid_number: &str,
) -> Result<Option<BindingCandidateMetadata>, String> {
    let row = conn
        .query_opt(
            "SELECT s.cid_number, s.cid_full_name, s.cid_short_name,
                    s.province_code, COALESCE(s.city_code, ''), COALESCE(s.town_code, '')
             FROM subjects s
             WHERE s.cid_number = $1
             ORDER BY s.updated_at DESC
             LIMIT 1",
            &[&cid_number],
        )
        .map_err(|e| {
            format!(
                "query binding candidate metadata failed: {}",
                postgres_error_text(&e)
            )
        })?;
    Ok(row.map(|r| (r.get(0), r.get(1), r.get(2), r.get(3), r.get(4), r.get(5))))
}

/// 只补齐候选机构的展示元数据。该函数禁止写入任何授权作用域。
pub(crate) fn hydrate_candidate_institution_metadata_conn(
    conn: &mut impl postgres::GenericClient,
    candidate: &mut AdminInstitutionCandidate,
) -> Result<bool, String> {
    let Some(cid_number) = candidate
        .institution_cid_number
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_string)
    else {
        return Err("binding candidate institution_cid_number is required".to_string());
    };
    let Some((cid_number, cid_full_name, cid_short_name, _, _, _)) =
        resolve_binding_candidate_metadata_conn(conn, cid_number.as_str())?
    else {
        return Ok(false);
    };
    let changed = candidate.institution_cid_number.as_deref() != Some(cid_number.as_str())
        || candidate.cid_full_name != cid_full_name
        || candidate.cid_short_name != cid_short_name;
    candidate.institution_cid_number = Some(cid_number);
    candidate.cid_full_name = cid_full_name;
    candidate.cid_short_name = cid_short_name;
    Ok(changed)
}

/// 授权作用域无法确定时的错误前缀。
///
/// 用于把「业务前置条件不满足」与「数据库故障」分开:前者重试永远不会成功,必须给出
/// 可操作原因,不能报成「保存失败,请稍后重试」。识别方在
/// `authentication::login::onchain_gate`,与 `qr_login` 的 `http:<kind>:` 前缀约定同源。
pub(crate) const SCOPE_ERROR_PREFIX: &str = "scope:";

/// 从授权真源派生候选的行政作用域。
///
/// FRG 的机构 CID 行政区只是登记地址，绝不能覆盖省管理员岗位；其省作用域仅由
/// `frg_province_code`（链上 InstitutionRoleAssignments）决定。其它机构才读取 CID 投影位置。
fn authorization_scope_from_identity_conn(
    conn: &mut impl postgres::GenericClient,
    institution_code: &str,
    institution_cid_number: &str,
    frg_province_code: Option<&str>,
) -> Result<AdminScope, String> {
    crate::core::chain_runtime::identity_from_binding_parts(institution_code, Some(institution_cid_number))?;
    let institution_scope = if crate::core::chain_runtime::is_tier1_registry(institution_code) {
        None
    } else {
        // 作用域优先取本地投影,投影缺失时从机构 CID 兜底派生。
        //
        // 机构 CID 是省/市的唯一来源；投影缺失时仍可派生这两级，随后独立补齐本机构资料。
        // 镇码仅存在于机构资料，不得从 CID 市段另编或猜测。
        //
        // 保留投影优先而非无条件走 CID:`cid_scope_codes` 只解出 R5 = 省2 + 市3,
        // **不含镇码**。镇级机构(`T***`,14 种 `AdminLevel::Town`)的镇码只存在于投影里,
        // 无条件改走 CID 会让镇级管理员 `scope_town_name` 恒为 None,
        // 进而在 `authorization::rules` 的 TOWN 分支 fail-closed 成空可见范围。
        let (province, city) = cid_scope_codes(institution_cid_number.as_bytes()).map_err(|error| format!("{SCOPE_ERROR_PREFIX}institution cid scope invalid: {error}"))?;
        let province_code = String::from_utf8(province.to_vec()).map_err(|_| "invalid institution province code")?;
        let city_code = String::from_utf8(city.to_vec()).map_err(|_| "invalid institution city number")?;
        let town_code = resolve_binding_candidate_metadata_conn(conn, institution_cid_number)?.map(|(_,_,_,_,_,town)| town).unwrap_or_default();
        let (province_name, city_name, town_name) = crate::cid::china::area_display_names(
            province_code.as_str(),
            Some(city_code.as_str()),
            Some(town_code.as_str()),
        );
        Some((
            (!province_name.is_empty()).then_some(province_name),
            (!city_name.is_empty()).then_some(city_name),
            (!town_name.is_empty()).then_some(town_name),
        ))
    };
    authorization_scope_from_sources(
        institution_code,
        frg_province_code.map(crate::core::chain_runtime::parse_hex_2).transpose()?,
        institution_scope,
    )
}

fn authorization_scope_from_sources(
    institution_code: &str,
    frg_province_code: Option<[u8; 2]>,
    institution_scope: Option<AdminScope>,
) -> Result<AdminScope, String> {
    if crate::core::chain_runtime::is_tier1_registry(institution_code) {
        let province_code = frg_province_code.ok_or_else(|| {
            format!("{SCOPE_ERROR_PREFIX}FRG authorization requires frg_province_code")
        })?;
        let province_name = crate::core::chain_runtime::chain_province_name_by_code(province_code)
            .ok_or_else(|| {
                format!("{SCOPE_ERROR_PREFIX}FRG authorization province code is unknown")
            })?;
        return Ok((Some(province_name), None, None));
    }
    institution_scope
        .ok_or_else(|| format!("{SCOPE_ERROR_PREFIX}binding institution scope is required"))
}

pub(crate) fn derive_candidate_authorization_scope_conn(
    conn: &mut impl postgres::GenericClient,
    candidate: &mut AdminInstitutionCandidate,
) -> Result<(), String> {
    let cid_number = candidate
        .institution_cid_number
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "binding candidate institution_cid_number is required".to_string())?;
    let (province_name, city_name, town_name) = authorization_scope_from_identity_conn(
        conn,
        candidate.institution_code.as_str(),
        cid_number,
        candidate.frg_province_code.as_deref(),
    )?;
    candidate.scope_province_name = province_name;
    candidate.scope_city_name = city_name;
    candidate.scope_town_name = town_name;
    Ok(())
}

/// 把原始节点绑定转换为临时展示候选；派生结果只存在内存，不回写绑定表。
pub(crate) fn candidate_for_binding_conn(
    conn: &mut impl postgres::GenericClient,
    binding: &NodeInstitutionBinding,
) -> Result<AdminInstitutionCandidate, String> {
    let mut candidate = AdminInstitutionCandidate {
        candidate_id: binding.candidate_id.clone(),
        institution_code: binding.institution_code.clone(),
        admin_level: crate::core::chain_runtime::admin_level_label_for(&binding.institution_code),
        institution_cid_number: Some(binding.institution_cid_number.clone()),
        frg_province_code: None,
        cid_full_name: None,
        cid_short_name: None,
        scope_province_name: None,
        scope_city_name: None,
        scope_town_name: None,
    };
    if !crate::core::chain_runtime::is_tier1_registry(&candidate.institution_code) { derive_candidate_authorization_scope_conn(conn, &mut candidate)?; }
    hydrate_candidate_institution_metadata_conn(conn, &mut candidate)?;
    Ok(candidate)
}

pub(crate) fn get_active_node_binding_conn(
    conn: &mut impl postgres::GenericClient,
) -> Result<Option<NodeInstitutionBinding>, String> {
    let row = conn
        .query_opt(
            "SELECT binding_id, candidate_id, institution_code, institution_cid_number,
                    bound_account_id, bound_at, status
             FROM node_institution_bindings
             WHERE status = 'ACTIVE'
             ORDER BY bound_at DESC
             LIMIT 1",
            &[],
        )
        .map_err(|e| {
            format!(
                "query active node binding failed: {}",
                postgres_error_text(&e)
            )
        })?;
    row.as_ref().map(binding_from_row).transpose()
}

pub(crate) fn active_node_binding(db: &Db) -> Result<Option<NodeInstitutionBinding>, String> {
    db.with_client(get_active_node_binding_conn)
}

/// 绑定确认、治理写入和实体投影共用事务锁；调用方必须持有数据库事务。
pub(crate) fn lock_node_binding_conn(conn: &mut impl postgres::GenericClient) -> Result<(), String> {
    conn.query_one("SELECT pg_advisory_xact_lock(hashtextextended('onchina-node-binding', 0))", &[])
        .map_err(|e| format!("lock node binding failed: {e}"))?;
    Ok(())
}

pub(crate) fn upsert_active_node_binding_conn(
    conn: &mut impl postgres::GenericClient,
    binding: &NodeInstitutionBinding,
) -> Result<(), String> {
    conn.execute(
        "UPDATE node_institution_bindings
         SET status = 'INACTIVE'
         WHERE status = 'ACTIVE'",
        &[],
    )
    .map_err(|e| {
        format!(
            "deactivate old node binding failed: {}",
            postgres_error_text(&e)
        )
    })?;
    conn.execute(
        "INSERT INTO node_institution_bindings (
            binding_id, candidate_id, institution_code, institution_cid_number,
            bound_account_id, bound_at, status
         )
         VALUES ($1,$2,$3,$4,$5,$6,$7)",
        &[
            &binding.binding_id,
            &binding.candidate_id,
            &binding.institution_code,
            &binding.institution_cid_number,
            &binding.bound_account_id,
            &binding.bound_at,
            &binding.status,
        ],
    )
    .map_err(|e| format!("insert node binding failed: {}", postgres_error_text(&e)))?;
    Ok(())
}

pub(crate) fn deactivate_active_node_binding_conn(conn: &mut impl postgres::GenericClient) -> Result<u64, String> {
    let changed = conn
        .execute(
            "UPDATE node_institution_bindings
             SET status = 'INACTIVE'
             WHERE status = 'ACTIVE'",
            &[],
        )
        .map_err(|e| {
            format!(
                "deactivate active node binding failed: {}",
                postgres_error_text(&e)
            )
        })?;
    Ok(changed)
}

pub(crate) fn insert_node_binding_challenge_conn(
    conn: &mut impl postgres::GenericClient,
    challenge: &NodeBindingChallenge,
) -> Result<(), String> {
    let payload = serde_json::to_value(challenge)
        .map_err(|e| format!("encode node binding challenge failed: {e}"))?;
    conn.execute(
        "INSERT INTO node_binding_challenges(
            binding_challenge_id, account_id, expires_at, consumed, payload
         )
         VALUES ($1,$2,$3,$4,$5)
         ON CONFLICT (binding_challenge_id) DO UPDATE SET
            account_id = EXCLUDED.account_id,
            expires_at = EXCLUDED.expires_at,
            consumed = EXCLUDED.consumed,
            payload = EXCLUDED.payload",
        &[
            &challenge.binding_challenge_id,
            &challenge.account_id,
            &challenge.expire_at,
            &challenge.consumed,
            &payload,
        ],
    )
    .map_err(|e| {
        format!(
            "insert node binding challenge failed: {}",
            postgres_error_text(&e)
        )
    })?;
    Ok(())
}

pub(crate) fn get_node_binding_challenge_conn(
    conn: &mut impl postgres::GenericClient,
    binding_challenge_id: &str,
) -> Result<Option<NodeBindingChallenge>, String> {
    let row = conn
        .query_opt(
            "SELECT payload FROM node_binding_challenges WHERE binding_challenge_id = $1",
            &[&binding_challenge_id],
        )
        .map_err(|e| {
            format!(
                "query node binding challenge failed: {}",
                postgres_error_text(&e)
            )
        })?;
    row.map(|r| serde_json::from_value::<NodeBindingChallenge>(r.get(0)))
        .transpose()
        .map_err(|e| format!("decode node binding challenge failed: {e}"))
}

pub(crate) fn consume_node_binding_challenge_conn(
    conn: &mut impl postgres::GenericClient,
    challenge: &NodeBindingChallenge,
) -> Result<(), String> {
    let mut consumed = challenge.clone();
    consumed.consumed = true;
    let payload = serde_json::to_value(&consumed)
        .map_err(|e| format!("encode consumed node binding challenge failed: {e}"))?;
    conn.execute(
        "UPDATE node_binding_challenges
         SET consumed = true, payload = $2
         WHERE binding_challenge_id = $1",
        &[&challenge.binding_challenge_id, &payload],
    )
    .map_err(|e| {
        format!(
            "consume node binding challenge failed: {}",
            postgres_error_text(&e)
        )
    })?;
    Ok(())
}

pub(crate) fn next_admin_id_conn(conn: &mut impl postgres::GenericClient) -> Result<u64, String> {
    // 首次接续既有 ID，此后由单行原子递增；并发登录/名册登记不能读到同一个 MAX+1。
    if conn.query_opt("SELECT next_seq FROM sequence_counters WHERE seq_key='admin_id'", &[]).map_err(|e| e.to_string())?.is_none() {
        conn.execute("INSERT INTO sequence_counters(seq_key,next_seq) SELECT 'admin_id', COALESCE(MAX(admin_id),0) FROM admins ON CONFLICT (seq_key) DO NOTHING", &[])
            .map_err(|e| format!("initialize admin id counter failed: {e}"))?;
    }
    let row = conn.query_opt("UPDATE sequence_counters SET next_seq=next_seq+1 WHERE seq_key='admin_id' AND next_seq>=0 AND next_seq<9223372036854775807 RETURNING next_seq", &[])
        .map_err(|e| format!("allocate admin id failed: {e}"))?
        .ok_or("admin id capacity exhausted or counter invalid")?;
    let id: i64 = row.get(0);
    u64::try_from(id).map_err(|_| "invalid allocated admin id".to_string())
}

/// 写入 / 更新本地管理员缓存行(`account_id` 为冲突键,幂等)。
///
/// 管理员成员资格与节点机构归属以链上 active 集合 + 本节点 active binding 为准。
/// 本函数只维护 `admins` 登录元数据缓存本身。
pub(crate) fn upsert_admin_conn(conn: &mut impl postgres::GenericClient, admin: &AdminUser) -> Result<(), String> {
    conn.execute(
        "INSERT INTO admins(admin_id, account_id, family_name, given_name, institution_code, built_in, creator_account_id, created_at, updated_at, city_name, institution_cid_number)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
         ON CONFLICT (account_id) DO UPDATE SET
            family_name = EXCLUDED.family_name,
            given_name = EXCLUDED.given_name,
            institution_code = EXCLUDED.institution_code,
            built_in = EXCLUDED.built_in,
            creator_account_id = EXCLUDED.creator_account_id,
            updated_at = EXCLUDED.updated_at,
            city_name = EXCLUDED.city_name,
            institution_cid_number = EXCLUDED.institution_cid_number",
        &[
            &i64::try_from(admin.id).map_err(|_| "admin id exceeds database capacity")?,
            &admin.account_id,
            &admin.family_name,
            &admin.given_name,
            &admin.institution_code,
            &admin.built_in,
            &admin.creator_account_id,
            &admin.created_at,
            &admin.updated_at,
            &admin.city_name,
            &admin.institution_cid_number,
        ],
    )
    .map_err(|e| format!("upsert admin failed: {e}"))?;
    Ok(())
}

pub(crate) fn delete_admin_runtime_state_conn(
    conn: &mut impl postgres::GenericClient,
    account_id: &str,
) -> Result<(), String> {
    conn.execute(
        "DELETE FROM admin_sessions WHERE account_id = $1",
        &[&account_id],
    )
    .map_err(|e| format!("delete admin sessions failed: {e}"))?;
    conn.execute(
        "DELETE FROM admin_action_challenges WHERE actor_account_id = $1",
        &[&account_id],
    )
    .map_err(|e| format!("delete admin action challenges failed: {e}"))?;
    conn.execute(
        "DELETE FROM admin_security_grants WHERE actor_account_id = $1",
        &[&account_id],
    )
    .map_err(|e| format!("delete admin security grants failed: {e}"))?;
    Ok(())
}

pub(crate) fn delete_all_admin_sessions_conn(conn: &mut impl postgres::GenericClient) -> Result<u64, String> {
    conn.execute("DELETE FROM admin_sessions", &[])
        .map_err(|e| format!("delete all admin sessions failed: {e}"))
}

pub(crate) fn cleanup_security_state_conn(
    conn: &mut impl postgres::GenericClient,
    now: DateTime<Utc>,
) -> Result<(), String> {
    conn.execute(
        "DELETE FROM admin_action_challenges
         WHERE consumed = true OR expires_at < $1",
        &[&now],
    )
    .map_err(|e| format!("cleanup admin action challenges failed: {e}"))?;
    conn.execute(
        "DELETE FROM admin_security_grants
         WHERE consumed = true OR expires_at < $1",
        &[&now],
    )
    .map_err(|e| format!("cleanup admin security grants failed: {e}"))?;
    Ok(())
}

pub(crate) fn insert_action_challenge(
    db: &Db,
    challenge: &AdminActionChallenge,
) -> Result<(), String> {
    let challenge = challenge.clone();
    db.with_client(move |conn| {
        cleanup_security_state_conn(conn, Utc::now())?;
        upsert_action_challenge_conn(conn, &challenge)
    })
}

/// 按 action_id 取挑战,同时以 actor_account_id 做先验隔离——只能读自己发起的挑战。
/// 非本人 action_id 直接当作不存在(返回 None),不在 DB 层暴露他人挑战。
pub(crate) fn get_action_challenge_conn(
    conn: &mut impl postgres::GenericClient,
    action_id: &str,
    actor_account_id: &str,
) -> Result<Option<AdminActionChallenge>, String> {
    let row = conn
        .query_opt(
            "SELECT payload FROM admin_action_challenges
             WHERE action_id = $1 AND actor_account_id = $2",
            &[&action_id, &actor_account_id],
        )
        .map_err(|e| format!("query action challenge failed: {e}"))?;
    row.map(|r| serde_json::from_value::<AdminActionChallenge>(r.get(0)))
        .transpose()
        .map_err(|e| format!("decode action challenge failed: {e}"))
}

pub(crate) fn upsert_action_challenge_conn(
    conn: &mut impl postgres::GenericClient,
    challenge: &AdminActionChallenge,
) -> Result<(), String> {
    let payload = serde_json::to_value(challenge)
        .map_err(|e| format!("encode action challenge failed: {e}"))?;
    conn.execute(
        "INSERT INTO admin_action_challenges(action_id, actor_account_id, action_type, expires_at, consumed, payload)
         VALUES ($1, $2, $3, $4, $5, $6)
         ON CONFLICT (action_id) DO UPDATE SET
            actor_account_id = EXCLUDED.actor_account_id,
            action_type = EXCLUDED.action_type,
            expires_at = EXCLUDED.expires_at,
            consumed = EXCLUDED.consumed,
            payload = EXCLUDED.payload",
        &[
            &challenge.action_id,
            &challenge.actor_account_id,
            &challenge.action_type,
            &challenge.expires_at,
            &challenge.consumed,
            &payload,
        ],
    )
    .map_err(|e| format!("upsert action challenge failed: {e}"))?;
    Ok(())
}

pub(crate) fn insert_security_grant_conn(
    conn: &mut impl postgres::GenericClient,
    grant: &AdminSecurityGrant,
) -> Result<(), String> {
    let payload =
        serde_json::to_value(grant).map_err(|e| format!("encode security grant failed: {e}"))?;
    conn.execute(
        "INSERT INTO admin_security_grants(grant_id, actor_account_id, action_type, expires_at, consumed, payload)
         VALUES ($1, $2, $3, $4, $5, $6)
         ON CONFLICT (grant_id) DO UPDATE SET
            actor_account_id = EXCLUDED.actor_account_id,
            action_type = EXCLUDED.action_type,
            expires_at = EXCLUDED.expires_at,
            consumed = EXCLUDED.consumed,
            payload = EXCLUDED.payload",
        &[
            &grant.grant_id,
            &grant.actor_account_id,
            &grant.action_type,
            &grant.expires_at,
            &grant.consumed,
            &payload,
        ],
    )
    .map_err(|e| format!("insert security grant failed: {e}"))?;
    Ok(())
}

/// 按 grant_id 取冷签授权,同时以 actor_account_id 做先验隔离——只能读自己持有的授权。
/// 非本人 grant_id 直接当作不存在(返回 None),不在 DB 层暴露他人授权。
pub(crate) fn get_security_grant_conn(
    conn: &mut impl postgres::GenericClient,
    grant_id: &str,
    actor_account_id: &str,
) -> Result<Option<AdminSecurityGrant>, String> {
    let row = conn
        .query_opt(
            "SELECT payload FROM admin_security_grants
             WHERE grant_id = $1 AND actor_account_id = $2",
            &[&grant_id, &actor_account_id],
        )
        .map_err(|e| format!("query security grant failed: {e}"))?;
    row.map(|r| serde_json::from_value::<AdminSecurityGrant>(r.get(0)))
        .transpose()
        .map_err(|e| format!("decode security grant failed: {e}"))
}

pub(crate) fn cleanup_login_state_conn(
    conn: &mut impl postgres::GenericClient,
    now: DateTime<Utc>,
) -> Result<(), String> {
    let stale_login_before = now - Duration::minutes(10);
    let consumed_login_before = now;
    conn.execute(
        "DELETE FROM admin_login_sign_requests
         WHERE expires_at < $1
            OR (consumed = true AND expires_at < $2)",
        &[&stale_login_before, &consumed_login_before],
    )
    .map_err(|e| {
        format!(
            "cleanup login sign requests failed: {}",
            postgres_error_text(&e)
        )
    })?;
    let stale_qr_created_before = now - Duration::hours(1);
    let stale_qr_expired_before = now - Duration::minutes(10);
    conn.execute(
        "DELETE FROM admin_qr_login_results
         WHERE created_at < $1
            OR expires_at < $2",
        &[&stale_qr_created_before, &stale_qr_expired_before],
    )
    .map_err(|e| {
        format!(
            "cleanup qr login results failed: {}",
            postgres_error_text(&e)
        )
    })?;
    conn.execute(
        "DELETE FROM node_binding_challenges
         WHERE expires_at < $1
            OR (consumed = true AND expires_at < $2)",
        &[&stale_login_before, &consumed_login_before],
    )
    .map_err(|e| {
        format!(
            "cleanup node binding challenges failed: {}",
            postgres_error_text(&e)
        )
    })?;
    Ok(())
}

pub(crate) fn insert_login_sign_request(
    db: &Db,
    challenge: &LoginSignRequest,
) -> Result<(), String> {
    let challenge = challenge.clone();
    db.with_client(move |conn| {
        cleanup_login_state_conn(conn, Utc::now())?;
        let payload = serde_json::to_value(&challenge)
            .map_err(|e| format!("encode login sign request failed: {e}"))?;
        conn.execute(
            "INSERT INTO admin_login_sign_requests(challenge_id, session_id, account_id, expires_at, consumed, payload)
             VALUES ($1, $2, $3, $4, $5, $6)
             ON CONFLICT (challenge_id) DO UPDATE SET
                session_id = EXCLUDED.session_id,
                account_id = EXCLUDED.account_id,
                expires_at = EXCLUDED.expires_at,
                consumed = EXCLUDED.consumed,
                payload = EXCLUDED.payload",
            &[
                &challenge.challenge_id,
                &challenge.session_id,
                &challenge.account_id,
                &challenge.expire_at,
                &challenge.consumed,
                &payload,
            ],
        )
        .map_err(|e| format!("insert login sign request failed: {}", postgres_error_text(&e)))?;
        Ok(())
    })
}

pub(crate) fn get_login_sign_request_conn(
    conn: &mut impl postgres::GenericClient,
    challenge_id: &str,
) -> Result<Option<LoginSignRequest>, String> {
    let row = conn
        .query_opt(
            "SELECT payload FROM admin_login_sign_requests WHERE challenge_id = $1",
            &[&challenge_id],
        )
        .map_err(|e| {
            format!(
                "query login sign request failed: {}",
                postgres_error_text(&e)
            )
        })?;
    row.map(|r| serde_json::from_value::<LoginSignRequest>(r.get(0)))
        .transpose()
        .map_err(|e| format!("decode login sign request failed: {e}"))
}

pub(crate) fn update_login_sign_request_conn(
    conn: &mut impl postgres::GenericClient,
    challenge: &LoginSignRequest,
) -> Result<(), String> {
    let payload = serde_json::to_value(challenge)
        .map_err(|e| format!("encode login sign request failed: {e}"))?;
    conn.execute(
        "UPDATE admin_login_sign_requests
         SET account_id = $2, expires_at = $3, consumed = $4, payload = $5
         WHERE challenge_id = $1",
        &[
            &challenge.challenge_id,
            &challenge.account_id,
            &challenge.expire_at,
            &challenge.consumed,
            &payload,
        ],
    )
    .map_err(|e| {
        format!(
            "update login sign request failed: {}",
            postgres_error_text(&e)
        )
    })?;
    Ok(())
}

/// 会话令牌落库前统一取 SHA-256 十六进制:token 列与 payload 都只存哈希,
/// DB 被 dump 也拿不到可直接冒用的明文令牌(明文只在客户端 Cookie/请求头里)。
/// 令牌本身是高熵随机 UUID,无需加盐即可抵御彩虹表。
pub(crate) fn admin_session_token_hash(raw_token: &str) -> String {
    hex::encode(Sha256::digest(raw_token.as_bytes()))
}

pub(crate) fn insert_admin_session_conn(
    conn: &mut impl postgres::GenericClient,
    session: &AdminSession,
) -> Result<(), String> {
    // 入参 session.token 是明文令牌;列存哈希,payload 里的 token 清空(不落明文)。
    let token_hash = admin_session_token_hash(&session.token);
    let mut stored = session.clone();
    stored.token.clear();
    let payload =
        serde_json::to_value(&stored).map_err(|e| format!("encode admin session failed: {e}"))?;
    conn.execute(
        "INSERT INTO admin_sessions(token, account_id, institution_code, candidate_id, expires_at, last_active_at, payload)
         VALUES ($1, $2, $3, $4, $5, $6, $7)
         ON CONFLICT (token) DO UPDATE SET
            account_id = EXCLUDED.account_id,
            institution_code = EXCLUDED.institution_code,
            candidate_id = EXCLUDED.candidate_id,
            expires_at = EXCLUDED.expires_at,
            last_active_at = EXCLUDED.last_active_at,
            payload = EXCLUDED.payload",
        &[
            &token_hash,
            &session.account_id,
            &session.institution_code,
            &session.candidate_id,
            &session.expire_at,
            &session.last_active_at,
            &payload,
        ],
    )
    .map_err(|e| format!("insert admin session failed: {e}"))?;
    Ok(())
}

pub(crate) fn delete_admin_session(db: &Db, token: &str) -> Result<(), String> {
    let token_hash = admin_session_token_hash(token.trim());
    db.with_client(move |conn| {
        conn.execute(
            "DELETE FROM admin_sessions WHERE token = $1",
            &[&token_hash],
        )
        .map_err(|e| format!("delete admin session failed: {e}"))?;
        Ok(())
    })
}

pub(crate) fn get_admin_session_conn(
    conn: &mut impl postgres::GenericClient,
    token: &str,
) -> Result<Option<AdminSession>, String> {
    let token_hash = admin_session_token_hash(token);
    let row = conn
        .query_opt(
            "SELECT payload FROM admin_sessions WHERE token = $1",
            &[&token_hash],
        )
        .map_err(|e| format!("query admin session failed: {e}"))?;
    row.map(|r| serde_json::from_value::<AdminSession>(r.get(0)))
        .transpose()
        .map_err(|e| format!("decode admin session failed: {e}"))
        // payload 里 token 已清空;回填调用方持有的明文令牌,后续 touch(再 insert)
        // 才能按同一明文重新取哈希,不会因空串算错 PK。
        .map(|opt| {
            opt.map(|mut session| {
                session.token = token.to_string();
                session
            })
        })
}

pub(crate) fn touch_admin_session_conn(
    conn: &mut impl postgres::GenericClient,
    session: &AdminSession,
) -> Result<(), String> {
    insert_admin_session_conn(conn, session)
}

pub(crate) fn cleanup_admin_sessions_conn(
    conn: &mut impl postgres::GenericClient,
    now: DateTime<Utc>,
    city_idle_timeout_minutes: i64,
) -> Result<(), String> {
    conn.execute("DELETE FROM admin_sessions WHERE expires_at < $1", &[&now])
        .map_err(|e| format!("cleanup expired admin sessions failed: {e}"))?;
    let idle_cutoff = now - Duration::minutes(city_idle_timeout_minutes);
    conn.execute(
        "DELETE FROM admin_sessions
         WHERE institution_code = 'CREG' AND last_active_at < $1",
        &[&idle_cutoff],
    )
    .map_err(|e| format!("cleanup idle city admin sessions failed: {e}"))?;
    Ok(())
}

/// 列出某机构当前有会话的去重管理员账户(链上集合复查用)。
pub(crate) fn list_session_admin_account_ids_conn(
    conn: &mut impl postgres::GenericClient,
    institution_code: &str,
) -> Result<Vec<String>, String> {
    let rows = conn
        .query(
            "SELECT DISTINCT account_id FROM admin_sessions WHERE institution_code = $1",
            &[&institution_code],
        )
        .map_err(|e| format!("list session admin accounts failed: {e}"))?;
    Ok(rows.iter().map(|r| r.get::<_, String>(0)).collect())
}

/// 清退某管理员账户的全部会话,返回删除行数(链上移除后即时失效用)。
pub(crate) fn delete_admin_sessions_for_account_id_conn(
    conn: &mut impl postgres::GenericClient,
    account_id: &str,
) -> Result<u64, String> {
    conn.execute(
        "DELETE FROM admin_sessions WHERE account_id = $1",
        &[&account_id],
    )
    .map_err(|e| format!("delete admin sessions for account failed: {e}"))
}

pub(crate) fn insert_qr_login_result_conn(
    conn: &mut impl postgres::GenericClient,
    challenge_id: &str,
    result: &QrLoginResultRecord,
) -> Result<(), String> {
    let payload =
        serde_json::to_value(result).map_err(|e| format!("encode qr login result failed: {e}"))?;
    conn.execute(
        "INSERT INTO admin_qr_login_results(challenge_id, session_id, access_token, expires_at, payload, created_at)
         VALUES ($1, $2, $3, $4, $5, $6)
         ON CONFLICT (challenge_id) DO UPDATE SET
            session_id = EXCLUDED.session_id,
            access_token = EXCLUDED.access_token,
            expires_at = EXCLUDED.expires_at,
            payload = EXCLUDED.payload,
            created_at = EXCLUDED.created_at",
        &[
            &challenge_id,
            &result.session_id,
            &result.access_token,
            &result.expire_at,
            &payload,
            &result.created_at,
        ],
    )
    .map_err(|e| format!("insert qr login result failed: {e}"))?;
    Ok(())
}

pub(crate) fn get_qr_login_result_conn(
    conn: &mut impl postgres::GenericClient,
    challenge_id: &str,
) -> Result<Option<QrLoginResultRecord>, String> {
    let row = conn
        .query_opt(
            "SELECT payload FROM admin_qr_login_results WHERE challenge_id = $1",
            &[&challenge_id],
        )
        .map_err(|e| format!("query qr login result failed: {e}"))?;
    row.map(|r| serde_json::from_value::<QrLoginResultRecord>(r.get(0)))
        .transpose()
        .map_err(|e| format!("decode qr login result failed: {e}"))
}

#[cfg(test)]
// 仓储契约测试使用断言式解包定位不满足的前置条件，不影响生产错误处理。
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "由控制台交付本机 onchina_test 数据库后执行隔离 PostgreSQL 集成回归"]
    fn concurrent_admin_id_allocation_is_unique_and_overflow_is_explicit() {
        let sandbox = crate::core::db::IsolatedTestDb::new();
        let handles = (0..8).map(|_| { let db=sandbox.db.clone(); std::thread::spawn(move || (0..16).map(|_| db.with_client(next_admin_id_conn).unwrap()).collect::<Vec<_>>()) }).collect::<Vec<_>>();
        let ids = handles.into_iter().flat_map(|handle| handle.join().unwrap()).collect::<std::collections::BTreeSet<_>>();
        assert_eq!(ids.len(),128); assert_eq!(ids.first(),Some(&1)); assert_eq!(ids.last(),Some(&128));
        sandbox.db.with_client(|conn| { conn.execute("UPDATE sequence_counters SET next_seq=9223372036854775807 WHERE seq_key='admin_id'",&[]).unwrap(); assert!(next_admin_id_conn(conn).is_err()); Ok(()) }).unwrap();
    }

    #[test]
    fn frg_role_province_cannot_be_overwritten_by_institution_location() {
        let institution_location =
            Some((Some("中枢省".to_string()), Some("锦程市".to_string()), None));
        let scope = authorization_scope_from_sources("FRG", Some(*b"GZ"), institution_location)
            .expect("FRG role province must resolve");

        assert_eq!(scope, (Some("贵州省".to_string()), None, None));
    }

    #[test]
    fn frg_authorization_fails_without_role_province() {
        let result = authorization_scope_from_sources("FRG", None, None);
        assert_eq!(
            result.unwrap_err(),
            format!("{SCOPE_ERROR_PREFIX}FRG authorization requires frg_province_code")
        );
    }

    /// 作用域类错误必须带 `scope:` 前缀,否则会被当成数据库故障、报成
    /// 「请稍后重试」——而这类错误重试永远不会成功。
    #[test]
    fn scope_errors_are_prefixed_for_classification() {
        for result in [
            authorization_scope_from_sources("FRG", None, None),
            authorization_scope_from_sources("FRG", Some(*b"??"), None),
            authorization_scope_from_sources("SFGY", None, None),
        ] {
            let message = result.expect_err("这些前置条件都不满足,必须失败");
            assert!(
                message.starts_with(SCOPE_ERROR_PREFIX),
                "作用域错误缺少 scope: 前缀,会被误判成数据库故障: {message}"
            );
        }
    }

    /// 非 Tier1 机构给出作用域后即通过,不再要求本地投影命中。
    #[test]
    fn non_tier1_accepts_scope_derived_without_projection() {
        let derived_from_cid = Some((Some("贵州省".to_string()), Some("绥阳市".to_string()), None));
        let scope = authorization_scope_from_sources("SFGY", None, derived_from_cid)
            .expect("私权机构作用域可由机构 CID 派生");
        assert_eq!(
            scope,
            (Some("贵州省".to_string()), Some("绥阳市".to_string()), None)
        );
    }

    /// 机构 CID 的 R5 = 省2 + 市3,私权机构也能解出——这是缺投影时的兜底真源。
    #[test]
    fn private_institution_cid_yields_province_and_city() {
        let (province, city) =
            cid_scope_codes(b"GZ018-SFGYR-201206100-2026").expect("私权机构 CID 必须能解出省市码");
        assert_eq!(&province, b"GZ");
        assert_eq!(&city, b"018");
    }

    /// 自然人 CID 去地域化,传进来必须 fail-closed,不得把号段误读成区划码。
    #[test]
    fn person_cid_scope_is_rejected() {
        assert!(cid_scope_codes(b"CN001-CTZN-000000001-2026").is_err());
    }
}
