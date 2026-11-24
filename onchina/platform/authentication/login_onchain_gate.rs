//! 验证人员当前绑定和真实有效任职，再选择本次机构岗位并签发会话。
use super::model::{
    AdminIdentifyOutput, AdminInstitutionCandidate, AdminSession, NodeBindingChallenge,
    NodeBindingRequiredOutput, NodeInstitutionBinding,
};
use crate::{authentication::repo, chain_runtime, *};
use chrono::{DateTime, Duration, Utc};
use uuid::Uuid;
pub(super) enum GateError {
    NoAssignment,
    ChainUnreachable(String),
    BindingInvalid(String),
    BindingExpired,
    BindingMismatch,
    AuthorizationScopeUnavailable(String),
    Db(String),
}
fn classify_state_error(message: String) -> GateError {
    match message.strip_prefix(repo::SCOPE_ERROR_PREFIX) {
        Some(s) => GateError::AuthorizationScopeUnavailable(s.into()),
        None => GateError::Db(message),
    }
}
pub(super) enum GateOutcome {
    Session {
        access_token: String,
        expires_at: DateTime<Utc>,
        admin: Box<AdminIdentifyOutput>,
    },
    BindingRequired(NodeBindingRequiredOutput),
}
pub(super) fn gate_error_response(error: GateError) -> axum::response::Response {
    use axum::http::StatusCode;
    match error {
        GateError::NoAssignment | GateError::BindingMismatch => {
            api_error(StatusCode::FORBIDDEN, 2002, "没有当前有效的机构岗位任职")
        }
        GateError::ChainUnreachable(e) => {
            tracing::warn!(error=%e,"read current person assignments failed");
            api_error(StatusCode::BAD_GATEWAY, 5002, "chain unreachable")
        }
        GateError::BindingInvalid(e) => api_error(StatusCode::BAD_REQUEST, 1001, &e),
        GateError::BindingExpired => {
            api_error(StatusCode::GONE, 1007, "role selection challenge expired")
        }
        GateError::AuthorizationScopeUnavailable(e) => {
            api_error(StatusCode::UNPROCESSABLE_ENTITY, 2005, &e)
        }
        GateError::Db(e) => {
            tracing::error!(error=%e,"login state failed");
            api_error(StatusCode::INTERNAL_SERVER_ERROR, 5001, "login state error")
        }
    }
}
pub(super) fn candidate_from_membership_conn(
    conn: &mut postgres::Client,
    m: &chain_runtime::ActiveMembership,
) -> Result<AdminInstitutionCandidate, String> {
    let code = chain_runtime::institution_code_label(&m.institution_code);
    let mut candidate = AdminInstitutionCandidate {
        candidate_id: m.candidate_id(),
        person_cid: m.person_cid.clone(),
        role_code: m.role_code.clone(),
        role_name: m.role_name.clone(),
        institution_code: code.clone(),
        admin_level: chain_runtime::admin_level_label_for(&code),
        institution_cid_number: Some(m.cid_number.clone()),
        frg_province_code: m.frg_province_code_hex(),
        cid_full_name: None,
        cid_short_name: None,
        scope_province_name: None,
        scope_city_name: None,
        scope_town_name: None,
    };
    let (p, c, t) = chain_runtime::role_scope(
        &m.institution_code,
        &m.cid_number,
        &m.role_code,
        &m.town_code,
    )?;
    candidate.scope_province_name = p;
    candidate.scope_city_name = c;
    candidate.scope_town_name = t;
    // 本地资料仅补机构名称，不能覆盖链上任职、人员或授权辖区。
    repo::hydrate_candidate_institution_metadata_conn(conn, &mut candidate)?;
    Ok(candidate)
}
pub(super) async fn validate_login_identity(
    account: &str,
) -> Result<(String, Vec<chain_runtime::ActiveMembership>), GateError> {
    let normalized = chain_runtime::normalize_account_id(account).ok_or(GateError::NoAssignment)?;
    let memberships = chain_runtime::find_active_memberships(&normalized)
        .await
        .map_err(GateError::ChainUnreachable)?;
    if memberships.is_empty() {
        return Err(GateError::NoAssignment);
    }
    Ok((normalized, memberships))
}
pub(super) async fn issue_session_after_onchain_gate(
    state: &AppState,
    account: &str,
    now: DateTime<Utc>,
) -> Result<GateOutcome, GateError> {
    let (account, memberships) = validate_login_identity(account).await?;
    if memberships.len() == 1
        && repo::active_node_binding(&state.db)
            .map_err(GateError::Db)?
            .is_some()
    {
        let candidate = state
            .db
            .with_client(|c| candidate_from_membership_conn(c, &memberships[0]))
            .map_err(classify_state_error)?;
        let (access_token, expires_at, admin) =
            issue_session_for_candidate(state, &account, &candidate, now).await?;
        return Ok(GateOutcome::Session {
            access_token,
            expires_at,
            admin: Box::new(admin),
        });
    }
    let candidates = state
        .db
        .with_client(|conn| {
            memberships
                .iter()
                .map(|m| candidate_from_membership_conn(conn, m))
                .collect::<Result<Vec<_>, _>>()
        })
        .map_err(classify_state_error)?;
    let id = Uuid::new_v4().to_string();
    let challenge = NodeBindingChallenge {
        binding_challenge_id: id.clone(),
        account_id: account.clone(),
        candidates: candidates.clone(),
        expires_at: now + Duration::minutes(10),
        consumed: false,
    };
    state
        .db
        .with_client(|conn| repo::insert_node_binding_challenge_conn(conn, &challenge))
        .map_err(GateError::Db)?;
    Ok(GateOutcome::BindingRequired(NodeBindingRequiredOutput {
        binding_challenge_id: id,
        account_id: account,
        candidates,
    }))
}
pub(super) async fn confirm_node_binding_after_onchain_gate(
    state: &AppState,
    id: &str,
    selected: &str,
    now: DateTime<Utc>,
) -> Result<(String, DateTime<Utc>, AdminIdentifyOutput), GateError> {
    let challenge = state
        .db
        .with_client(|c| repo::get_node_binding_challenge_conn(c, id))
        .map_err(GateError::Db)?
        .ok_or_else(|| GateError::BindingInvalid("role selection challenge missing".into()))?;
    if challenge.consumed {
        return Err(GateError::BindingInvalid("role selection consumed".into()));
    }
    if now > challenge.expires_at {
        return Err(GateError::BindingExpired);
    }
    if !challenge
        .candidates
        .iter()
        .any(|c| c.candidate_id == selected)
    {
        return Err(GateError::BindingMismatch);
    }
    let (_, memberships) = validate_login_identity(&challenge.account_id).await?;
    let membership = memberships
        .iter()
        .find(|m| m.candidate_id() == selected)
        .ok_or(GateError::BindingMismatch)?;
    let candidate = state
        .db
        .with_client(|c| candidate_from_membership_conn(c, membership))
        .map_err(classify_state_error)?;
    state
        .db
        .with_client(|conn| {
            let mut tx = conn.transaction().map_err(|e| e.to_string())?;
            repo::lock_node_binding_conn(&mut tx)?;
            let fresh = repo::get_node_binding_challenge_conn(&mut tx, id)?
                .ok_or("role selection missing")?;
            if fresh.consumed || Utc::now() > fresh.expires_at {
                return Err("role selection expired or consumed".into());
            }
            // 只有首次初始化创建节点归属；选择其它任职不会更换现有节点绑定。
            if repo::get_active_node_binding_conn(&mut tx)?.is_none() {
                repo::upsert_active_node_binding_conn(
                    &mut tx,
                    &NodeInstitutionBinding {
                        binding_id: Uuid::new_v4().to_string(),
                        candidate_id: candidate.candidate_id.clone(),
                        institution_code: candidate.institution_code.clone(),
                        institution_cid_number: candidate
                            .institution_cid_number
                            .clone()
                            .ok_or("institution missing")?,
                        bound_account_id: challenge.account_id.clone(),
                        bound_at: now,
                        status: "ACTIVE".into(),
                    },
                )?;
            }
            repo::consume_node_binding_challenge_conn(&mut tx, &challenge)?;
            tx.commit().map_err(|e| e.to_string())
        })
        .map_err(GateError::Db)?;
    issue_session_for_candidate(state, &challenge.account_id, &candidate, now).await
}
async fn issue_session_for_candidate(
    state: &AppState,
    account: &str,
    candidate: &AdminInstitutionCandidate,
    now: DateTime<Utc>,
) -> Result<(String, DateTime<Utc>, AdminIdentifyOutput), GateError> {
    let cid = candidate
        .institution_cid_number
        .as_deref()
        .ok_or(GateError::BindingMismatch)?;
    let identity =
        chain_runtime::identity_from_binding_parts(&candidate.institution_code, Some(cid))
            .map_err(GateError::BindingInvalid)?;
    let view = chain_runtime::FinalizedChainView::connect()
        .await
        .map_err(GateError::ChainUnreachable)?;
    let rows = crate::institutions::fetch_active_assignments_at(&identity, &view, Some(account))
        .await
        .map_err(GateError::ChainUnreachable)?;
    let assignment = rows
        .iter()
        .find(|a| a.role_code == candidate.role_code && a.person_cid == candidate.person_cid)
        .ok_or(GateError::NoAssignment)?;
    let token = Uuid::new_v4().to_string();
    let expires = now + Duration::hours(8);
    let caps =
        crate::modules::capability::capabilities_for(&state.modules, &candidate.institution_code);
    let workspace = crate::workspace::build_institution_workspace(
        &state.modules,
        &candidate.institution_code,
        candidate.cid_short_name.as_deref(),
        caps,
        Vec::new(),
    );
    let mut identify = AdminIdentifyOutput {
        binding_revision: assignment.binding_revision.clone(),
        person_cid: assignment.person_cid.clone(),
        role_code: assignment.role_code.clone(),
        account_id: account.into(),
        institution_cid_number: cid.into(),
        institution_code: candidate.institution_code.clone(),
        admin_level: candidate.admin_level.clone(),
        family_name: assignment.family_name.clone(),
        given_name: assignment.given_name.clone(),
        scope_province_name: candidate.scope_province_name.clone(),
        scope_city_name: candidate.scope_city_name.clone(),
        scope_town_name: candidate.scope_town_name.clone(),
        cid_short_name: candidate.cid_short_name.clone(),
        capabilities: caps,
        workspace,
        setup_required: false,
    };
    state
        .db
        .with_client(|conn| {
            let mut tx = conn.transaction().map_err(|e| e.to_string())?;
            repo::lock_node_binding_conn(&mut tx)?;
            if repo::get_active_node_binding_conn(&mut tx)?.is_none() {
                return Err("node binding missing".into());
            }
            identify.setup_required = !crate::authentication::passkey::has_credential(
                &mut tx,
                &AdminSession::new(
                    token.clone(),
                    candidate.candidate_id.clone(),
                    now,
                    identify.clone(),
                )
                .context(),
            )?;
            repo::insert_admin_session_conn(
                &mut tx,
                &AdminSession::new(
                    token.clone(),
                    candidate.candidate_id.clone(),
                    now,
                    identify.clone(),
                ),
            )?;
            tx.commit().map_err(|e| e.to_string())
        })
        .map_err(GateError::Db)?;
    identify.workspace.workspace_modules = state
        .modules
        .workspace_modules(cid, &crate::modules::session_headers(&token))
        .await;
    state
        .db
        .with_client(|conn| repo::update_session_workspace_conn(conn, &token, &identify.workspace))
        .map_err(GateError::Db)?;
    Ok((token, expires, identify))
}
pub(crate) async fn revoke_stale_admin_sessions_loop(db: Db) {
    let mut ticker = tokio::time::interval(std::time::Duration::from_secs(45));
    loop {
        ticker.tick().await;
        if let Err(e) = revoke_stale_admin_sessions_once(&db).await {
            tracing::warn!(error=%e,"recheck institution role sessions failed");
        }
    }
}
pub(crate) async fn revoke_stale_admin_sessions_once(db: &Db) -> Result<(), String> {
    let sessions = db.with_client(|conn| {
        conn.query("SELECT payload_json FROM session", &[])
            .map_err(|e| e.to_string())
    })?;
    for row in sessions {
        let value: serde_json::Value = row.get(0);
        let session: AdminSession = match serde_json::from_value(value.clone()) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let memberships = chain_runtime::find_active_memberships(&session.account_id).await?;
        let current = memberships.iter().any(|m| {
            m.person_cid == session.person_cid
                && m.cid_number == session.institution_cid_number
                && m.role_code == session.role_code
        });
        let identity = chain_runtime::FinalizedChainView::connect().await?;
        let revision = crate::chain_identity::read_citizen_identity_at(
            &session.person_cid,
            Some(identity.block_hash.0),
        )
        .await?
        .binding_revision;
        if !current
            || revision.map(|r| r.to_string()).as_deref() != Some(session.binding_revision.as_str())
        {
            db.with_client(|conn| {
                conn.execute("DELETE FROM session WHERE payload_json=$1", &[&value])
                    .map_err(|e| e.to_string())
            })?;
        }
    }
    Ok(())
}
