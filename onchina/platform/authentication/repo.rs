//! 读写登录挑战、会话、机构绑定及安全动作记录。

use chrono::{DateTime, Duration, Utc};
use sha2::{Digest, Sha256};

use crate::authentication::login::{
    AdminInstitutionCandidate, AdminSession, LoginSignRequest, NodeBindingChallenge,
    NodeInstitutionBinding, QrLoginResultRecord,
};
use crate::authentication::security_model::{AdminActionChallenge, AdminSecurityGrant};
use crate::database::postgres_error_text;
use crate::Db;

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

/// 候选展示资料只从所属机构投影读取，不授予任职或业务权限。
type BindingCandidateMetadata = (
    String,
    Option<String>,
    Option<String>,
    String,
    String,
    String,
);

pub(crate) fn resolve_binding_candidate_metadata_conn(
    conn: &mut impl postgres::GenericClient,
    cid_number: &str,
) -> Result<Option<BindingCandidateMetadata>, String> {
    let row = conn
        .query_opt(
            "SELECT s.cid_number, s.full_name, s.short_name,
                    s.province_code, COALESCE(s.city_code, ''), COALESCE(s.town_code, '')
             FROM subject s
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

pub fn get_active_node_binding_conn(
    conn: &mut impl postgres::GenericClient,
) -> Result<Option<NodeInstitutionBinding>, String> {
    let row = conn
        .query_opt(
            "SELECT binding_id, candidate_id, institution_code, institution_cid,
                    bound_account, bound_at, state_code
             FROM binding
             WHERE state_code = 'ACTIVE'
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

pub fn active_node_binding(db: &Db) -> Result<Option<NodeInstitutionBinding>, String> {
    db.with_client(get_active_node_binding_conn)
}

/// 绑定确认、治理写入和实体投影共用事务锁；调用方必须持有数据库事务。
pub fn lock_node_binding_conn(conn: &mut impl postgres::GenericClient) -> Result<(), String> {
    conn.query_one(
        "SELECT pg_advisory_xact_lock(hashtextextended('onchina-node-binding', 0))",
        &[],
    )
    .map_err(|e| format!("lock node binding failed: {e}"))?;
    Ok(())
}

pub fn upsert_active_node_binding_conn(
    conn: &mut impl postgres::GenericClient,
    binding: &NodeInstitutionBinding,
) -> Result<(), String> {
    conn.execute(
        "UPDATE binding
         SET state_code = 'INACTIVE'
         WHERE state_code = 'ACTIVE'",
        &[],
    )
    .map_err(|e| {
        format!(
            "deactivate old node binding failed: {}",
            postgres_error_text(&e)
        )
    })?;
    conn.execute(
        "INSERT INTO binding (
            binding_id, candidate_id, institution_code, institution_cid,
            bound_account, bound_at, state_code
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

pub fn deactivate_active_node_binding_conn(
    conn: &mut impl postgres::GenericClient,
) -> Result<u64, String> {
    let changed = conn
        .execute(
            "UPDATE binding
             SET state_code = 'INACTIVE'
             WHERE state_code = 'ACTIVE'",
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
        .map_err(|e| format!("encode authentication record failed: {e}"))?;
    conn.execute("INSERT INTO enrollment(payload_json) VALUES($1) ON CONFLICT ((payload_json->>'bindingChallengeId')) DO UPDATE SET payload_json=EXCLUDED.payload_json", &[&payload]).map_err(|e| format!("store authentication record failed: {e}"))?;
    Ok(())
}

pub(crate) fn get_node_binding_challenge_conn(
    conn: &mut impl postgres::GenericClient,
    binding_challenge_id: &str,
) -> Result<Option<NodeBindingChallenge>, String> {
    let row = conn
        .query_opt(
            "SELECT payload_json FROM enrollment WHERE payload_json->>'bindingChallengeId' = $1",
            &[&binding_challenge_id],
        )
        .map_err(|e| format!("query NodeBindingChallenge failed: {e}"))?;
    row.map(|row| serde_json::from_value::<NodeBindingChallenge>(row.get(0)))
        .transpose()
        .map_err(|e| format!("decode NodeBindingChallenge failed: {e}"))
}

pub(crate) fn consume_node_binding_challenge_conn(
    conn: &mut impl postgres::GenericClient,
    challenge: &NodeBindingChallenge,
) -> Result<(), String> {
    let count = conn.execute("UPDATE enrollment SET payload_json=jsonb_set(payload_json,'{consumed}','true') WHERE payload_json->>'bindingChallengeId'=$1 AND (payload_json->>'consumed')::boolean=false", &[&challenge.binding_challenge_id]).map_err(|e| format!("consume binding challenge failed: {e}"))?;
    if count != 1 {
        return Err("binding challenge missing or consumed".into());
    }
    Ok(())
}

pub fn delete_admin_runtime_state_conn(
    conn: &mut impl postgres::GenericClient,
    account_id: &str,
) -> Result<(), String> {
    conn.execute(
        "DELETE FROM session WHERE payload_json->>'accountId' = $1",
        &[&account_id],
    )
    .map_err(|e| format!("delete admin sessions failed: {e}"))?;
    conn.execute(
        "DELETE FROM challenge WHERE payload_json->>'actorAccountId' = $1",
        &[&account_id],
    )
    .map_err(|e| format!("delete admin action challenges failed: {e}"))?;
    conn.execute(
        "DELETE FROM \"grant\" WHERE payload_json->>'actorAccountId' = $1",
        &[&account_id],
    )
    .map_err(|e| format!("delete admin security grants failed: {e}"))?;
    Ok(())
}

pub(crate) fn delete_all_admin_sessions_conn(
    conn: &mut impl postgres::GenericClient,
) -> Result<u64, String> {
    conn.execute("DELETE FROM session", &[])
        .map_err(|e| format!("delete all admin sessions failed: {e}"))
}

pub(crate) fn cleanup_security_state_conn(
    conn: &mut impl postgres::GenericClient,
    now: DateTime<Utc>,
) -> Result<(), String> {
    let now = now.timestamp_millis();
    conn.execute("DELETE FROM challenge WHERE (payload_json->>'consumed')::boolean OR (payload_json->>'expiresAt')::bigint < $1", &[&now]).map_err(|e| format!("cleanup action challenges failed: {e}"))?;
    conn.execute("DELETE FROM \"grant\" WHERE (payload_json->>'consumed')::boolean OR (payload_json->>'expiresAt')::bigint < $1", &[&now]).map_err(|e| format!("cleanup security grants failed: {e}"))?;
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
            "SELECT payload_json FROM challenge WHERE payload_json->>'actionId' = $1 AND payload_json->>'actorAccountId' = $2",
            &[&action_id,&actor_account_id],
        )
        .map_err(|e| format!("query AdminActionChallenge failed: {e}"))?;
    row.map(|row| serde_json::from_value::<AdminActionChallenge>(row.get(0)))
        .transpose()
        .map_err(|e| format!("decode AdminActionChallenge failed: {e}"))
}

pub(crate) fn upsert_action_challenge_conn(
    conn: &mut impl postgres::GenericClient,
    challenge: &AdminActionChallenge,
) -> Result<(), String> {
    let payload = serde_json::to_value(challenge)
        .map_err(|e| format!("encode authentication record failed: {e}"))?;
    conn.execute("INSERT INTO challenge(payload_json) VALUES($1) ON CONFLICT ((payload_json->>'actionId')) DO UPDATE SET payload_json=EXCLUDED.payload_json", &[&payload]).map_err(|e| format!("store authentication record failed: {e}"))?;
    Ok(())
}

pub(crate) fn insert_security_grant_conn(
    conn: &mut impl postgres::GenericClient,
    grant: &AdminSecurityGrant,
) -> Result<(), String> {
    let payload = serde_json::to_value(grant)
        .map_err(|e| format!("encode authentication record failed: {e}"))?;
    conn.execute("INSERT INTO \"grant\"(payload_json) VALUES($1) ON CONFLICT ((payload_json->>'grantId')) DO UPDATE SET payload_json=EXCLUDED.payload_json", &[&payload]).map_err(|e| format!("store authentication record failed: {e}"))?;
    Ok(())
}

/// 按 grant_id 取冷签授权,同时以 actor_account_id 做先验隔离——只能读自己持有的授权。
/// 非本人 grant_id 直接当作不存在(返回 None),不在 DB 层暴露他人授权。
pub(crate) fn get_security_grant_conn(
    conn: &mut impl postgres::GenericClient,
    grant_id: &str,
    actor_account_id: &str,
) -> Result<Option<AdminSecurityGrant>, String> {
    let row = conn.query_opt("SELECT payload_json FROM \"grant\" WHERE payload_json->>'grantId'=$1 AND payload_json->>'actorAccountId'=$2", &[&grant_id,&actor_account_id]).map_err(|e| format!("query security grant failed: {e}"))?;
    row.map(|row| serde_json::from_value::<AdminSecurityGrant>(row.get(0)))
        .transpose()
        .map_err(|e| format!("decode security grant failed: {e}"))
}

pub(crate) fn cleanup_login_state_conn(
    conn: &mut impl postgres::GenericClient,
    now: DateTime<Utc>,
) -> Result<(), String> {
    let stale_login = (now - Duration::minutes(10)).timestamp_millis();
    let consumed_login = now.timestamp_millis();
    conn.execute("DELETE FROM login WHERE (payload_json->>'expiresAt')::bigint < $1 OR ((payload_json->>'consumed')::boolean AND (payload_json->>'expiresAt')::bigint < $2)", &[&stale_login,&consumed_login]).map_err(|e| format!("cleanup login requests failed: {e}"))?;
    let stale_created = now - Duration::hours(1);
    conn.execute("DELETE FROM result WHERE (payload_json->>'createdAt')::timestamptz < $1 OR (payload_json->>'expiresAt')::bigint < $2", &[&stale_created,&stale_login]).map_err(|e| format!("cleanup QR results failed: {e}"))?;
    conn.execute("DELETE FROM enrollment WHERE (payload_json->>'expiresAt')::bigint < $1 OR ((payload_json->>'consumed')::boolean AND (payload_json->>'expiresAt')::bigint < $2)", &[&stale_login,&consumed_login]).map_err(|e| format!("cleanup binding challenges failed: {e}"))?;
    Ok(())
}

/// 调用方在同一绑定锁事务中完成机构匹配后保存挑战，不另开连接或提交事务。
pub(crate) fn insert_login_sign_request_conn(
    conn: &mut impl postgres::GenericClient,
    challenge: &LoginSignRequest,
) -> Result<(), String> {
    cleanup_login_state_conn(conn, Utc::now())?;
    let payload =
        serde_json::to_value(challenge).map_err(|e| format!("encode login request failed: {e}"))?;
    conn.execute("INSERT INTO login(payload_json) VALUES($1) ON CONFLICT ((payload_json->>'challengeId')) DO UPDATE SET payload_json=EXCLUDED.payload_json", &[&payload])
        .map_err(|e| format!("store login request failed: {e}"))?;
    Ok(())
}

pub(crate) fn get_login_sign_request_conn(
    conn: &mut impl postgres::GenericClient,
    challenge_id: &str,
) -> Result<Option<LoginSignRequest>, String> {
    let row = conn
        .query_opt(
            "SELECT payload_json FROM login WHERE payload_json->>'challengeId' = $1",
            &[&challenge_id],
        )
        .map_err(|e| format!("query LoginSignRequest failed: {e}"))?;
    row.map(|row| serde_json::from_value::<LoginSignRequest>(row.get(0)))
        .transpose()
        .map_err(|e| format!("decode LoginSignRequest failed: {e}"))
}

pub(crate) fn update_login_sign_request_conn(
    conn: &mut impl postgres::GenericClient,
    challenge: &LoginSignRequest,
) -> Result<(), String> {
    let payload =
        serde_json::to_value(challenge).map_err(|e| format!("encode login request failed: {e}"))?;
    let count = conn
        .execute(
            "UPDATE login SET payload_json=$2 WHERE payload_json->>'challengeId'=$1",
            &[&challenge.challenge_id, &payload],
        )
        .map_err(|e| format!("update login request failed: {e}"))?;
    if count != 1 {
        return Err("login request missing".into());
    }
    Ok(())
}

/// 会话只以原令牌SHA-256定位，JSON不存明文、空值占位或第二份哈希,
/// DB 被 dump 也拿不到可直接冒用的明文令牌(明文只在客户端 Cookie/请求头里)。
/// 令牌本身是高熵随机 UUID,无需加盐即可抵御彩虹表。
pub(crate) fn admin_session_token_hash(raw_token: &str) -> String {
    hex::encode(Sha256::digest(raw_token.as_bytes()))
}

pub(crate) fn insert_admin_session_conn(
    conn: &mut impl postgres::GenericClient,
    session: &AdminSession,
) -> Result<(), String> {
    let token_hash = admin_session_token_hash(&session.token);
    let payload =
        serde_json::to_value(session).map_err(|e| format!("encode session failed: {e}"))?;
    conn.execute("INSERT INTO session(token_hash,payload_json) VALUES($1,$2) ON CONFLICT(token_hash) DO UPDATE SET payload_json=EXCLUDED.payload_json", &[&token_hash,&payload]).map_err(|e| format!("store session failed: {e}"))?;
    Ok(())
}

pub(crate) fn delete_admin_session(db: &Db, token: &str) -> Result<(), String> {
    let token_hash = admin_session_token_hash(token.trim());
    db.with_client(move |conn| {
        conn.execute("DELETE FROM session WHERE token_hash = $1", &[&token_hash])
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
            "SELECT payload_json FROM session WHERE token_hash=$1",
            &[&token_hash],
        )
        .map_err(|e| format!("query session failed: {e}"))?;
    // 缺少完整登录快照的旧会话无效，要求重新登录；不建立旧结构兼容分支。
    let mut session = row.and_then(|row| serde_json::from_value::<AdminSession>(row.get(0)).ok());
    // 只有本次请求持有原令牌；JSON不保存其副本，后续touch仍按同一输入定位。
    if let Some(session) = session.as_mut() {
        session.token = token.to_string();
    }
    Ok(session)
}

pub(crate) fn touch_admin_session_conn(
    conn: &mut impl postgres::GenericClient,
    session: &AdminSession,
) -> Result<(), String> {
    // 活动时间不能覆盖并发完成的密钥设置阶段或登录上下文。
    let token_hash = admin_session_token_hash(&session.token);
    let active_at = session.last_active_at.timestamp_millis();
    conn.execute("UPDATE session SET payload_json=jsonb_set(payload_json,'{lastActiveAt}',to_jsonb($2::bigint)) WHERE token_hash=$1", &[&token_hash,&active_at])
        .map_err(|e| format!("touch session failed: {e}"))?;
    Ok(())
}

pub(crate) fn complete_passkey_setup_conn(
    conn: &mut impl postgres::GenericClient,
    ctx: &crate::authentication::login::AdminAuthContext,
) -> Result<(), String> {
    conn.execute("UPDATE session SET payload_json=jsonb_set(payload_json,'{setupRequired}','false'::jsonb) WHERE payload_json->>'accountId'=$1 AND payload_json->>'personCid'=$2 AND payload_json->>'bindingRevision'=$3 AND payload_json ? 'setupRequired'", &[&ctx.account_id,&ctx.person_cid,&ctx.binding_revision])
        .map_err(|e| format!("complete session setup failed: {e}"))?;
    Ok(())
}

pub(crate) fn update_session_workspace_conn(
    conn: &mut impl postgres::GenericClient,
    token: &str,
    workspace: &crate::workspace::InstitutionWorkspace,
) -> Result<(), String> {
    let token_hash = admin_session_token_hash(token);
    let workspace = serde_json::to_value(workspace).map_err(|e| e.to_string())?;
    let count = conn.execute("UPDATE session SET payload_json=jsonb_set(payload_json,'{workspace}',$2) WHERE token_hash=$1", &[&token_hash,&workspace])
        .map_err(|e| format!("complete login workspace failed: {e}"))?;
    if count != 1 {
        return Err("login session is no longer valid".into());
    }
    Ok(())
}

pub(crate) fn cleanup_admin_sessions_conn(
    conn: &mut impl postgres::GenericClient,
    now: DateTime<Utc>,
    idle_timeout_minutes: i64,
) -> Result<(), String> {
    let expires_before = now.timestamp_millis();
    conn.execute(
        "DELETE FROM session WHERE (payload_json->>'expiresAt')::bigint < $1",
        &[&expires_before],
    )
    .map_err(|e| format!("cleanup expired sessions failed: {e}"))?;
    let idle_before = (now - Duration::minutes(idle_timeout_minutes)).timestamp_millis();
    conn.execute("DELETE FROM session WHERE payload_json->>'institutionCode'='CREG' AND (payload_json->>'lastActiveAt')::bigint < $1", &[&idle_before]).map_err(|e| format!("cleanup idle sessions failed: {e}"))?;
    Ok(())
}

/// 列出某机构当前有会话的去重管理员账户(链上集合复查用)。

pub(crate) fn insert_qr_login_result_conn(
    conn: &mut impl postgres::GenericClient,
    challenge_id: &str,
    result: &QrLoginResultRecord,
) -> Result<(), String> {
    let payload =
        serde_json::to_value(result).map_err(|e| format!("encode QR result failed: {e}"))?;
    conn.execute("INSERT INTO result(challenge_id,payload_json) VALUES($1,$2) ON CONFLICT(challenge_id) DO UPDATE SET payload_json=EXCLUDED.payload_json", &[&challenge_id,&payload]).map_err(|e| format!("store QR result failed: {e}"))?;
    Ok(())
}

pub(crate) fn get_qr_login_result_conn(
    conn: &mut impl postgres::GenericClient,
    challenge_id: &str,
) -> Result<Option<QrLoginResultRecord>, String> {
    let row = conn
        .query_opt(
            "SELECT payload_json FROM result WHERE challenge_id = $1",
            &[&challenge_id],
        )
        .map_err(|e| format!("query QrLoginResultRecord failed: {e}"))?;
    row.map(|row| serde_json::from_value::<QrLoginResultRecord>(row.get(0)))
        .transpose()
        .map_err(|e| format!("decode QrLoginResultRecord failed: {e}"))
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
#[path = "../tests/authentication_repo.rs"]
mod tests;

/// 节点看板只读取机构展示资料；该视图不含人员、岗位或登录授权。
pub struct BindingInstitutionView {
    pub institution_code: String,
    pub institution_cid_number: Option<String>,
    pub cid_full_name: Option<String>,
    pub cid_short_name: Option<String>,
    pub scope_province_name: Option<String>,
    pub scope_city_name: Option<String>,
    pub scope_town_name: Option<String>,
}
pub fn candidate_for_binding_conn(
    conn: &mut impl postgres::GenericClient,
    binding: &NodeInstitutionBinding,
) -> Result<BindingInstitutionView, String> {
    let metadata = resolve_binding_candidate_metadata_conn(conn, &binding.institution_cid_number)?;
    let (province, city) =
        primitives::cid::number::cid_scope_codes(binding.institution_cid_number.as_bytes())?;
    let town = metadata.as_ref().map(|m| m.5.as_str());
    let (p, c, t) = crate::codes::china::area_display_names(
        std::str::from_utf8(&province).map_err(|_| "province invalid")?,
        Some(std::str::from_utf8(&city).map_err(|_| "city invalid")?),
        town,
    );
    Ok(BindingInstitutionView {
        institution_code: binding.institution_code.clone(),
        institution_cid_number: Some(binding.institution_cid_number.clone()),
        cid_full_name: metadata.as_ref().and_then(|m| m.1.clone()),
        cid_short_name: metadata.and_then(|m| m.2),
        scope_province_name: (!p.is_empty()).then_some(p),
        scope_city_name: (!c.is_empty()).then_some(c),
        scope_town_name: (!t.is_empty()).then_some(t),
    })
}
