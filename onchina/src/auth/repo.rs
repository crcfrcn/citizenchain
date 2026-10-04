//! 管理员结构化表读写。
//!
//! 管理员、登录签名请求、会话和安全动作状态全部以数据库表为唯一持久化真源。

use chrono::{DateTime, Duration, Utc};
use postgres::Client;
use sha2::{Digest, Sha256};

use crate::auth::login::{
    AdminInstitutionCandidate, AdminSession, LoginSignRequest, NodeBindingChallenge,
    NodeInstitutionBinding, QrLoginResultRecord,
};
use crate::auth::model::AdminUser;
use crate::auth::security_model::{AdminActionChallenge, AdminSecurityGrant};
use crate::core::db::postgres_error_text;
use crate::Db;
use primitives::cid::number::cid_scope_codes;

fn admin_from_row(row: &postgres::Row) -> Result<AdminUser, String> {
    let id: i64 = row.get(0);
    Ok(AdminUser {
        id: u64::try_from(id).unwrap_or(0),
        account_id: row.get(1),
        family_name: row.get(2),
        given_name: row.get(3),
        institution_code: row.get(4),
        built_in: row.get(5),
        creator_account_id: row.get(6),
        created_at: row.get(7),
        updated_at: row.get(8),
        city_name: row.get(9),
    })
}

fn binding_from_row(row: &postgres::Row) -> Result<NodeInstitutionBinding, String> {
    Ok(NodeInstitutionBinding {
        binding_id: row.get(0),
        candidate_id: row.get(1),
        institution_code: row.get(2),
        institution_cid_number: row.get(3),
        frg_province_code: row.get(4),
        bound_account_id: row.get(5),
        bound_at: row.get(6),
        status: row.get(7),
    })
}

// Tier1 创世注册局管理员「全走链读」：管理员账户 ID 来自 FRG AdminAccounts，
// 省维度只来自 InstitutionRoleAssignments；本地不得建立权限缓存。

pub(crate) fn get_admin_by_id_and_registry_org_conn(
    conn: &mut Client,
    id: u64,
    institution_code: &str,
) -> Result<Option<AdminUser>, String> {
    let id = id as i64;
    let row = conn
        .query_opt(
            "SELECT admin_id, account_id, family_name, given_name, institution_code, built_in, creator_account_id, created_at, updated_at, city_name
             FROM admins
             WHERE admin_id = $1 AND institution_code = $2",
            &[&id, &institution_code],
        )
        .map_err(|e| format!("query admin by id and institution_code failed: {e}"))?;
    row.as_ref().map(admin_from_row).transpose()
}

/// Tier2 下级注册局(CREG)管理员列表。
///
/// 每节点单省部署,本地 `admins` 缓存即本省数据,列表仅按机构码 + 可选市名过滤。
pub(crate) fn list_city_registry_admins_by_scope_conn(
    conn: &mut Client,
    city_name: Option<&str>,
    limit: usize,
    offset: usize,
) -> Result<(usize, Vec<AdminUser>), String> {
    let limit = i64::try_from(limit).unwrap_or(500);
    let offset = i64::try_from(offset).unwrap_or(0);
    let code = crate::core::chain_runtime::TIER2_REGISTRY_CODE;
    let (count_row, rows) = if let Some(city_name) = city_name {
        let count_row = conn
            .query_one(
                "SELECT COUNT(*) FROM admins
                 WHERE institution_code = $1 AND city_name = $2",
                &[&code, &city_name],
            )
            .map_err(|e| format!("count city registry admins by city failed: {e}"))?;
        let rows = conn
            .query(
                "SELECT admin_id, account_id, family_name, given_name, institution_code, built_in, creator_account_id, created_at, updated_at, city_name
                 FROM admins
                 WHERE institution_code = $1 AND city_name = $2
                 ORDER BY admin_id DESC
                 LIMIT $3 OFFSET $4",
                &[&code, &city_name, &limit, &offset],
            )
            .map_err(|e| format!("query city registry admins by city failed: {e}"))?;
        (count_row, rows)
    } else {
        let count_row = conn
            .query_one(
                "SELECT COUNT(*) FROM admins WHERE institution_code = $1",
                &[&code],
            )
            .map_err(|e| format!("count city registry admins failed: {e}"))?;
        let rows = conn
            .query(
                "SELECT admin_id, account_id, family_name, given_name, institution_code, built_in, creator_account_id, created_at, updated_at, city_name
                 FROM admins
                 WHERE institution_code = $1
                 ORDER BY admin_id DESC
                 LIMIT $2 OFFSET $3",
                &[&code, &limit, &offset],
            )
            .map_err(|e| format!("query city registry admins failed: {e}"))?;
        (count_row, rows)
    };
    let total: i64 = count_row.get(0);
    Ok((
        usize::try_from(total).unwrap_or(0),
        rows.iter()
            .map(admin_from_row)
            .collect::<Result<Vec<_>, _>>()?,
    ))
}

pub(crate) fn count_city_registry_admins_by_city_conn(
    conn: &mut Client,
    city_name: &str,
) -> Result<usize, String> {
    let code = crate::core::chain_runtime::TIER2_REGISTRY_CODE;
    let row = conn
        .query_one(
            "SELECT COUNT(*) FROM admins WHERE institution_code = $1 AND city_name = $2",
            &[&code, &city_name],
        )
        .map_err(|e| format!("count city registry admins by city failed: {e}"))?;
    let count: i64 = row.get(0);
    Ok(usize::try_from(count).unwrap_or(0))
}

pub(crate) fn get_admin_by_account_id(
    db: &Db,
    account_id: &str,
) -> Result<Option<AdminUser>, String> {
    let account_id = account_id.trim().to_string();
    db.with_client(move |conn| get_admin_by_account_id_conn(conn, account_id.as_str()))
}

pub(crate) fn get_admin_by_account_id_conn(
    conn: &mut Client,
    account_id: &str,
) -> Result<Option<AdminUser>, String> {
    let row = conn
        .query_opt(
            "SELECT admin_id, account_id, family_name, given_name, institution_code, built_in, creator_account_id, created_at, updated_at, city_name
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
/// FRG 省作用域只认绑定中的链上 `InstitutionRoleAssignments` 省岗位码；
/// 其它机构按绑定 CID 的机构行政区投影解析。绑定缺失或机构不一致一律失败关闭。
pub(crate) fn derive_admin_scope_conn(
    conn: &mut Client,
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
    authorization_scope_from_identity_conn(
        conn,
        binding.institution_code.as_str(),
        binding.institution_cid_number.as_str(),
        binding.frg_province_code.as_deref(),
    )
}

/// 解析当前管理员所属机构的 cid_short_name 单一字段。
/// 联邦注册局管理员 → institution_code='FRG' 的全局唯一机构(总统府联邦注册局,简称=联邦注册局);
/// 市注册局管理员   → institution_code='CREG' AND province_name AND city_name 的本市机构(如 合肥市注册局)。
/// 无对应行返回 None(前端按空处理,绝不另造名字)。
pub(crate) fn resolve_home_cid_short_name_conn(
    conn: &mut Client,
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
    conn: &mut Client,
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
    conn: &mut Client,
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
/// `auth::login::onchain_gate`,与 `qr_login` 的 `http:<kind>:` 前缀约定同源。
pub(crate) const SCOPE_ERROR_PREFIX: &str = "scope:";

/// 从授权真源派生候选的行政作用域。
///
/// FRG 的机构 CID 行政区只是登记地址，绝不能覆盖省管理员岗位；其省作用域仅由
/// `frg_province_code`（链上 InstitutionRoleAssignments）决定。其它机构才读取 CID 投影位置。
fn authorization_scope_from_identity_conn(
    conn: &mut Client,
    institution_code: &str,
    institution_cid_number: &str,
    frg_province_code: Option<&str>,
) -> Result<AdminScope, String> {
    let identity = crate::core::chain_runtime::identity_from_binding_parts(
        institution_code,
        Some(institution_cid_number),
        frg_province_code,
    )?;
    let institution_scope = if crate::core::chain_runtime::is_tier1_registry(institution_code) {
        None
    } else {
        // 作用域优先取本地投影,投影缺失时从机构 CID 兜底派生。
        //
        // 投影只是缓存,机构 CID 才是不可漂移的单源(`china-code-immutable` / ADR-021),
        // 因此缺投影绝不能阻断登录——私权机构本地无投影(`genesis_projection` 的私权回填
        // 尚未实现),旧实现在这里直接 Err 导致所有私权机构管理员登不进控制台。
        //
        // 保留投影优先而非无条件走 CID:`cid_scope_codes` 只解出 R5 = 省2 + 市3,
        // **不含镇码**。镇级机构(`T***`,14 种 `AdminLevel::Town`)的镇码只存在于投影里,
        // 无条件改走 CID 会让镇级管理员 `scope_town_name` 恒为 None,
        // 进而在 `scope::rules` 的 TOWN 分支 fail-closed 成空可见范围。
        let (province_code, city_code, town_code) =
            match resolve_binding_candidate_metadata_conn(conn, institution_cid_number)? {
                Some((_, _, _, province_code, city_code, town_code)) => {
                    (province_code, city_code, town_code)
                }
                None => {
                    let (province, city) = cid_scope_codes(institution_cid_number.as_bytes())
                        .map_err(|error| {
                            format!("{SCOPE_ERROR_PREFIX}institution cid scope invalid: {error}")
                        })?;
                    (
                        String::from_utf8_lossy(&province).into_owned(),
                        String::from_utf8_lossy(&city).into_owned(),
                        String::new(),
                    )
                }
            };
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
        identity.frg_province_code,
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
    conn: &mut Client,
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
    conn: &mut Client,
    binding: &NodeInstitutionBinding,
) -> Result<AdminInstitutionCandidate, String> {
    let mut candidate = AdminInstitutionCandidate {
        candidate_id: binding.candidate_id.clone(),
        institution_code: binding.institution_code.clone(),
        admin_level: crate::core::chain_runtime::admin_level_label_for(&binding.institution_code),
        institution_cid_number: Some(binding.institution_cid_number.clone()),
        frg_province_code: binding.frg_province_code.clone(),
        cid_full_name: None,
        cid_short_name: None,
        scope_province_name: None,
        scope_city_name: None,
        scope_town_name: None,
    };
    derive_candidate_authorization_scope_conn(conn, &mut candidate)?;
    hydrate_candidate_institution_metadata_conn(conn, &mut candidate)?;
    Ok(candidate)
}

pub(crate) fn get_active_node_binding_conn(
    conn: &mut Client,
) -> Result<Option<NodeInstitutionBinding>, String> {
    let row = conn
        .query_opt(
            "SELECT binding_id, candidate_id, institution_code, institution_cid_number,
                    frg_province_code, bound_account_id, bound_at, status
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

pub(crate) fn upsert_active_node_binding_conn(
    conn: &mut Client,
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
            frg_province_code, bound_account_id, bound_at, status
         )
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8)",
        &[
            &binding.binding_id,
            &binding.candidate_id,
            &binding.institution_code,
            &binding.institution_cid_number,
            &binding.frg_province_code,
            &binding.bound_account_id,
            &binding.bound_at,
            &binding.status,
        ],
    )
    .map_err(|e| format!("insert node binding failed: {}", postgres_error_text(&e)))?;
    Ok(())
}

pub(crate) fn deactivate_active_node_binding_conn(conn: &mut Client) -> Result<u64, String> {
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
    conn: &mut Client,
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
    conn: &mut Client,
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
    conn: &mut Client,
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

pub(crate) fn next_admin_id_conn(conn: &mut Client) -> Result<u64, String> {
    let row = conn
        .query_one("SELECT COALESCE(MAX(admin_id), 0) + 1 FROM admins", &[])
        .map_err(|e| format!("allocate admin id failed: {e}"))?;
    let id: i64 = row.get(0);
    Ok(u64::try_from(id).unwrap_or(1))
}

/// 写入 / 更新本地管理员缓存行(`account_id` 为冲突键,幂等)。
///
/// 管理员成员资格与节点机构归属以链上 active 集合 + 本节点 active binding 为准。
/// 本函数只维护 `admins` 登录元数据缓存本身。
pub(crate) fn upsert_admin_conn(conn: &mut Client, admin: &AdminUser) -> Result<(), String> {
    conn.execute(
        "INSERT INTO admins(admin_id, account_id, family_name, given_name, institution_code, built_in, creator_account_id, created_at, updated_at, city_name)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
         ON CONFLICT (account_id) DO UPDATE SET
            family_name = EXCLUDED.family_name,
            given_name = EXCLUDED.given_name,
            institution_code = EXCLUDED.institution_code,
            built_in = EXCLUDED.built_in,
            creator_account_id = EXCLUDED.creator_account_id,
            updated_at = EXCLUDED.updated_at,
            city_name = EXCLUDED.city_name",
        &[
            &(admin.id as i64),
            &admin.account_id,
            &admin.family_name,
            &admin.given_name,
            &admin.institution_code,
            &admin.built_in,
            &admin.creator_account_id,
            &admin.created_at,
            &admin.updated_at,
            &admin.city_name,
        ],
    )
    .map_err(|e| format!("upsert admin failed: {e}"))?;
    Ok(())
}

pub(crate) fn delete_admin_runtime_state_conn(
    conn: &mut Client,
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

pub(crate) fn delete_all_admin_sessions_conn(conn: &mut Client) -> Result<u64, String> {
    conn.execute("DELETE FROM admin_sessions", &[])
        .map_err(|e| format!("delete all admin sessions failed: {e}"))
}

pub(crate) fn cleanup_security_state_conn(
    conn: &mut Client,
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
    conn: &mut Client,
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
    conn: &mut Client,
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
    conn: &mut Client,
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
    conn: &mut Client,
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
    conn: &mut Client,
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
    conn: &mut Client,
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
    conn: &mut Client,
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
    conn: &mut Client,
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
    conn: &mut Client,
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
    conn: &mut Client,
    session: &AdminSession,
) -> Result<(), String> {
    insert_admin_session_conn(conn, session)
}

pub(crate) fn cleanup_admin_sessions_conn(
    conn: &mut Client,
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
    conn: &mut Client,
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
    conn: &mut Client,
    account_id: &str,
) -> Result<u64, String> {
    conn.execute(
        "DELETE FROM admin_sessions WHERE account_id = $1",
        &[&account_id],
    )
    .map_err(|e| format!("delete admin sessions for account failed: {e}"))
}

pub(crate) fn insert_qr_login_result_conn(
    conn: &mut Client,
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
    conn: &mut Client,
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
    use super::{authorization_scope_from_sources, cid_scope_codes, SCOPE_ERROR_PREFIX};

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
