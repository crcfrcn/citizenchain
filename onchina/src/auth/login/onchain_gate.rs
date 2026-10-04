//! 登录链上集合鉴权 + 会话签发(QR 登录与挑战登录共用)。
//!
//! (去中心化鉴权):
//! - 验签证明扫码者持有 `account_id` 私钥后,membership 真源切到**链上 Active 管理员集合**。
//! - 平台启动不预设机构;首次登录从 `verified_account_id` 反查候选机构,二次确认后本节点绑定唯一机构。
//! - 后台 `revoke_stale_admin_sessions_loop` 周期复查,管理员被链上移除后≤TTL 失效。

use chrono::{DateTime, Duration, Utc};
use uuid::Uuid;

use crate::auth::repo;
use crate::core::chain_runtime;
use crate::crypto::pubkey::same_account_id;
use crate::*;

use super::model::{
    AdminIdentifyOutput, AdminInstitutionCandidate, AdminSession, NodeBindingChallenge,
    NodeBindingRequiredOutput, NodeInstitutionBinding,
};

/// 链上集合鉴权失败分类(映射 HTTP 状态)。
pub(super) enum GateError {
    /// 扫码者不在本机构链上 Active 管理员集合。
    NotOnchainAdmin,
    /// 扫码者属于国家储委会/省储委会/省储行,这些机构只走节点桌面端。
    DesktopGovernanceUnsupported,
    /// 扫码者属于个人多签,不进入机构节点控制台。
    PersonalMultisigUnsupported,
    /// 链节点不可达 / 读取失败(瞬时,允许重试,绝不降级查本地表)。
    ChainUnreachable(String),
    /// 节点绑定确认请求非法。
    BindingInvalid(String),
    /// 节点绑定确认已过期。
    BindingExpired,
    /// 当前管理员不再属于待绑定机构。
    BindingMismatch,
    /// 授权作用域无法确定(业务前置条件不满足,重试无用)。
    ///
    /// 与 [`GateError::Db`] 严格分开:后者是数据库故障(瞬时,可重试),前者是数据本身
    /// 不足以派生作用域(机构 CID 非法、FRG 缺省码等),重试永远不会成功。
    AuthorizationScopeUnavailable(String),
    /// 本地状态读写失败(数据库故障)。
    Db(String),
}

/// 把 `db.with_client` 返回的字符串错误按前缀分流成 GateError。
///
/// `repo::SCOPE_ERROR_PREFIX` 标记的是「授权作用域无法确定」,不是数据库故障;
/// 混进 [`GateError::Db`] 会让前端显示「请稍后重试」,而这类错误重试永远不成功。
fn classify_state_error(message: String) -> GateError {
    match message.strip_prefix(repo::SCOPE_ERROR_PREFIX) {
        Some(reason) => GateError::AuthorizationScopeUnavailable(reason.to_string()),
        None => GateError::Db(message),
    }
}

pub(super) enum GateOutcome {
    Session {
        access_token: String,
        expire_at: DateTime<Utc>,
        // 会话输出远大于绑定提示，装箱避免每个枚举值都按最大变体占用栈空间。
        admin: Box<AdminIdentifyOutput>,
    },
    BindingRequired(NodeBindingRequiredOutput),
}

pub(super) fn gate_error_response(err: GateError) -> axum::response::Response {
    use axum::http::StatusCode;
    match err {
        GateError::NotOnchainAdmin => {
            api_error(StatusCode::FORBIDDEN, 2002, "not an on-chain admin")
        }
        GateError::DesktopGovernanceUnsupported => api_error(
            StatusCode::FORBIDDEN,
            2002,
            chain_runtime::DESKTOP_GOVERNANCE_LOGIN_UNSUPPORTED,
        ),
        GateError::PersonalMultisigUnsupported => api_error(
            StatusCode::FORBIDDEN,
            2002,
            chain_runtime::PERSONAL_MULTISIG_LOGIN_UNSUPPORTED,
        ),
        GateError::ChainUnreachable(message) => {
            tracing::warn!(error = %message, "chain unreachable during login gate");
            api_error(StatusCode::BAD_GATEWAY, 5002, "chain unreachable")
        }
        GateError::BindingInvalid(message) => {
            api_error(StatusCode::BAD_REQUEST, 1001, message.as_str())
        }
        GateError::BindingExpired => {
            api_error(StatusCode::GONE, 1007, "node binding challenge expired")
        }
        GateError::BindingMismatch => api_error(
            StatusCode::FORBIDDEN,
            2002,
            "admin no longer belongs to selected institution",
        ),
        GateError::AuthorizationScopeUnavailable(reason) => {
            // 业务前置条件不满足,不是故障:给出可操作原因,且不得暗示「重试可解」。
            tracing::warn!(reason = %reason, "login gate authorization scope unavailable");
            api_error(
                StatusCode::UNPROCESSABLE_ENTITY,
                2005,
                &format!("authorization scope unavailable: {reason}"),
            )
        }
        GateError::Db(message) => {
            // 完整原因只进服务端日志;响应只带不含敏感信息的短码,避免泄露库结构。
            tracing::error!(error = %message, "login gate state error");
            api_error(StatusCode::INTERNAL_SERVER_ERROR, 5001, "login state error")
        }
    }
}

fn candidate_from_membership_conn(
    conn: &mut postgres::Client,
    membership: &chain_runtime::ActiveAdminMembership,
) -> Result<AdminInstitutionCandidate, String> {
    let institution_code = chain_runtime::institution_code_label(&membership.institution_code);
    let mut candidate = AdminInstitutionCandidate {
        candidate_id: membership.candidate_id(),
        institution_code: institution_code.clone(),
        admin_level: chain_runtime::admin_level_label_for(&institution_code),
        institution_cid_number: Some(membership.cid_number.clone()),
        frg_province_code: membership.frg_province_code_hex(),
        cid_full_name: None,
        cid_short_name: None,
        scope_province_name: None,
        scope_city_name: None,
        scope_town_name: None,
    };

    // 权限先从链上身份键派生，展示元数据随后补齐且不得覆盖权限字段。
    repo::derive_candidate_authorization_scope_conn(conn, &mut candidate)?;
    repo::hydrate_candidate_institution_metadata_conn(conn, &mut candidate)?;
    Ok(candidate)
}

fn candidates_from_memberships_conn(
    conn: &mut postgres::Client,
    memberships: &[chain_runtime::ActiveAdminMembership],
) -> Result<Vec<AdminInstitutionCandidate>, String> {
    memberships
        .iter()
        .map(|membership| candidate_from_membership_conn(conn, membership))
        .collect()
}

fn binding_matches_candidate(
    binding: &NodeInstitutionBinding,
    candidate: &AdminInstitutionCandidate,
) -> bool {
    binding.candidate_id == candidate.candidate_id
}

async fn find_allowed_memberships_for_login(
    normalized_account_id: &str,
) -> Result<Vec<chain_runtime::ActiveAdminMembership>, GateError> {
    match chain_runtime::find_active_admin_memberships(normalized_account_id).await {
        Ok(memberships) => Ok(memberships),
        Err(err) if err == chain_runtime::DESKTOP_GOVERNANCE_LOGIN_UNSUPPORTED => {
            Err(GateError::DesktopGovernanceUnsupported)
        }
        Err(err) if err == chain_runtime::PERSONAL_MULTISIG_LOGIN_UNSUPPORTED => {
            Err(GateError::PersonalMultisigUnsupported)
        }
        Err(err) => Err(GateError::ChainUnreachable(err)),
    }
}

/// 钱包码给出目标账户后，按当前签名 `account_id` 反查链上 Active 管理员集合。
///
/// 判据 fail-closed：账户格式非法、链不可达、集合为空一律拒绝登录，不存在「读不到就
/// 放行」。二维码不重复携带管理员 CID；服务端从链上管理员名册读取 CID，并在同一个
/// finalized 区块通过 `CitizenIdentity` 正反向闭环解析当前绑定 `account_id`。因此换绑后
/// 只有新账户能登录，名册中的岗位关联旧账户不能继续签名；冻结公权管理员与私权非 LR
/// 没有 CID 时，严格按 runtime 规则使用名册 `account_id`。
///
/// 完成签名后仍会再次执行完整 gate，以最新链上状态为准签发会话。
pub(super) async fn validate_login_identity(account_id: &str) -> Result<String, GateError> {
    let normalized =
        chain_runtime::normalize_account_id(account_id).ok_or(GateError::NotOnchainAdmin)?;
    let memberships = find_allowed_memberships_for_login(normalized.as_str()).await?;
    if memberships.is_empty() {
        return Err(GateError::NotOnchainAdmin);
    }
    Ok(normalized)
}

/// 已验签账户经链上集合鉴权后，按节点绑定状态返回会话或待绑定候选。
pub(super) async fn issue_session_after_onchain_gate(
    state: &AppState,
    verified_account_id: &str,
    now: DateTime<Utc>,
) -> Result<GateOutcome, GateError> {
    let normalized = chain_runtime::normalize_account_id(verified_account_id)
        .ok_or(GateError::NotOnchainAdmin)?;
    let memberships = find_allowed_memberships_for_login(normalized.as_str()).await?;
    if memberships.is_empty() {
        return Err(GateError::NotOnchainAdmin);
    }

    let candidates = state
        .db
        .with_client(move |conn| candidates_from_memberships_conn(conn, &memberships))
        .map_err(classify_state_error)?;
    let Some(binding) = repo::active_node_binding(&state.db).map_err(GateError::Db)? else {
        let binding_challenge_id = Uuid::new_v4().to_string();
        let challenge = NodeBindingChallenge {
            binding_challenge_id: binding_challenge_id.clone(),
            account_id: normalized,
            candidates: candidates.clone(),
            expire_at: now + Duration::minutes(10),
            consumed: false,
        };
        state
            .db
            .with_client(move |conn| repo::insert_node_binding_challenge_conn(conn, &challenge))
            .map_err(GateError::Db)?;
        return Ok(GateOutcome::BindingRequired(NodeBindingRequiredOutput {
            binding_challenge_id,
            account_id: verified_account_id.to_string(),
            candidates,
        }));
    };
    let Some(candidate) = candidates
        .iter()
        .find(|candidate| binding_matches_candidate(&binding, candidate))
        .cloned()
    else {
        return Err(GateError::NotOnchainAdmin);
    };
    let (access_token, expire_at, admin) =
        issue_session_for_candidate(state, normalized.as_str(), &candidate, now).await?;
    Ok(GateOutcome::Session {
        access_token,
        expire_at,
        admin: Box::new(admin),
    })
}

pub(super) async fn confirm_node_binding_after_onchain_gate(
    state: &AppState,
    binding_challenge_id: &str,
    candidate_id: &str,
    now: DateTime<Utc>,
) -> Result<(String, DateTime<Utc>, AdminIdentifyOutput), GateError> {
    let binding_challenge_id = binding_challenge_id.trim().to_string();
    let candidate_id = candidate_id.trim().to_string();
    if binding_challenge_id.is_empty() || candidate_id.is_empty() {
        return Err(GateError::BindingInvalid(
            "binding_challenge_id and candidate_id are required".to_string(),
        ));
    }
    let challenge = state
        .db
        .with_client(move |conn| {
            repo::cleanup_login_state_conn(conn, now)?;
            repo::get_node_binding_challenge_conn(conn, binding_challenge_id.as_str())
        })
        .map_err(GateError::Db)?
        .ok_or_else(|| GateError::BindingInvalid("node binding challenge not found".to_string()))?;
    if challenge.consumed {
        return Err(GateError::BindingInvalid(
            "node binding challenge already consumed".to_string(),
        ));
    }
    if now > challenge.expire_at {
        return Err(GateError::BindingExpired);
    }
    let Some(selected) = challenge
        .candidates
        .iter()
        .find(|candidate| candidate.candidate_id == candidate_id)
        .cloned()
    else {
        return Err(GateError::BindingInvalid(
            "selected institution candidate not found".to_string(),
        ));
    };
    let normalized = chain_runtime::normalize_account_id(challenge.account_id.as_str())
        .ok_or(GateError::BindingMismatch)?;
    let memberships = find_allowed_memberships_for_login(normalized.as_str()).await?;
    let fresh_candidates = state
        .db
        .with_client(move |conn| candidates_from_memberships_conn(conn, &memberships))
        .map_err(classify_state_error)?;
    let Some(fresh_selected) = fresh_candidates
        .into_iter()
        .find(|candidate| candidate.candidate_id == selected.candidate_id)
    else {
        return Err(GateError::BindingMismatch);
    };
    let institution_cid_number =
        fresh_selected
            .institution_cid_number
            .clone()
            .ok_or_else(|| {
                GateError::BindingInvalid("selected institution CID is required".to_string())
            })?;
    let binding = NodeInstitutionBinding {
        binding_id: Uuid::new_v4().to_string(),
        candidate_id: fresh_selected.candidate_id.clone(),
        institution_code: fresh_selected.institution_code.clone(),
        institution_cid_number,
        frg_province_code: fresh_selected.frg_province_code.clone(),
        bound_account_id: normalized.clone(),
        bound_at: now,
        status: "ACTIVE".to_string(),
    };
    let challenge_for_consume = challenge.clone();
    let binding_for_db = binding.clone();
    state
        .db
        .with_client(move |conn| {
            repo::upsert_active_node_binding_conn(conn, &binding_for_db)?;
            repo::consume_node_binding_challenge_conn(conn, &challenge_for_consume)?;
            Ok(())
        })
        .map_err(GateError::Db)?;
    issue_session_for_candidate(state, normalized.as_str(), &fresh_selected, now).await
}

async fn issue_session_for_candidate(
    state: &AppState,
    verified_account_id: &str,
    candidate: &AdminInstitutionCandidate,
    now: DateTime<Utc>,
) -> Result<(String, DateTime<Utc>, AdminIdentifyOutput), GateError> {
    let institution_code = candidate.institution_code.clone();
    let candidate_id = candidate.candidate_id.clone();
    let scope_province_name = candidate.scope_province_name.clone();
    let scope_city_name = candidate.scope_city_name.clone();
    let scope_town_name = candidate.scope_town_name.clone();
    let cid_short_name = candidate.cid_short_name.clone();
    let institution_cid_number = candidate
        .institution_cid_number
        .clone()
        .ok_or_else(|| GateError::BindingInvalid("institution CID is required".to_string()))?;
    let identity = chain_runtime::identity_from_binding_parts(
        &institution_code,
        Some(&institution_cid_number),
        candidate.frg_province_code.as_deref(),
    )
    .map_err(GateError::BindingInvalid)?;
    let chain_admins = chain_runtime::fetch_active_admins_onchain(&identity)
        .await
        .map_err(GateError::ChainUnreachable)?;
    let chain_admin = chain_admins
        .as_ref()
        .and_then(|admins| {
            admins
                .iter()
                .find(|admin| same_account_id(&admin.account_id, verified_account_id))
        })
        .ok_or(GateError::NotOnchainAdmin)?;
    let chain_family_name = chain_admin.family_name.clone();
    let chain_given_name = chain_admin.given_name.clone();
    let account_id_for_db = verified_account_id.to_string();
    let mut result = state
        .db
        .with_client(move |conn| {
            // 已有本地行优先(保留既有省映射 / 既有市行 id);否则按节点身份新建元数据行。
            let existing = repo::get_admin_by_account_id_conn(conn, account_id_for_db.as_str())?;
            let admin = match existing {
                Some(mut current) => {
                    // 链上身份与本地登记冲突时,以链上机构码为准(去中心化真源)。
                    current.institution_code = institution_code.clone();
                    current.family_name = chain_family_name.clone();
                    current.given_name = chain_given_name.clone();
                    current.city_name = scope_city_name.clone().unwrap_or_default();
                    current.updated_at = Some(now);
                    current
                }
                None => AdminUser {
                    id: repo::next_admin_id_conn(conn)?,
                    account_id: account_id_for_db.clone(),
                    family_name: chain_family_name.clone(),
                    given_name: chain_given_name.clone(),
                    institution_code: institution_code.clone(),
                    built_in: false,
                    creator_account_id: account_id_for_db.clone(),
                    created_at: now,
                    updated_at: Some(now),
                    city_name: scope_city_name.clone().unwrap_or_default(),
                },
            };

            // 节点机构归属已由 active binding 承载;admins 只缓存登录管理员元数据。
            repo::upsert_admin_conn(conn, &admin)?;
            let cid_short_name = cid_short_name.or_else(|| {
                repo::resolve_home_cid_short_name_conn(
                    conn,
                    &admin.institution_code,
                    scope_province_name.as_deref(),
                    scope_city_name.as_deref(),
                )
                .ok()
                .flatten()
            });

            let access_token = Uuid::new_v4().to_string();
            let expire_at = now + Duration::hours(8);
            repo::insert_admin_session_conn(
                conn,
                &AdminSession {
                    token: access_token.clone(),
                    account_id: admin.account_id.clone(),
                    institution_code: institution_code.clone(),
                    candidate_id: candidate_id.clone(),
                    expire_at,
                    last_active_at: now,
                },
            )?;

            let admin_level = chain_runtime::admin_level_label_for(&institution_code);
            let capabilities = crate::platform::capability::capabilities_for(&institution_code);
            let workspace = crate::workspace::build_institution_workspace(
                &institution_code,
                cid_short_name.as_deref(),
                capabilities,
                Vec::new(),
            );
            Ok((
                access_token,
                expire_at,
                AdminIdentifyOutput {
                    account_id: admin.account_id,
                    institution_cid_number,
                    institution_code: institution_code.clone(),
                    admin_level,
                    capabilities,
                    workspace,
                    family_name: admin.family_name,
                    given_name: admin.given_name,
                    scope_province_name,
                    scope_city_name,
                    scope_town_name,
                    cid_short_name,
                },
            ))
        })
        .map_err(GateError::Db)?;

    let workspace_modules =
        crate::domains::membership::workspace_modules_for(&result.2.institution_cid_number).await;
    result.2.workspace.workspace_modules = workspace_modules;

    Ok(result)
}

/// 后台周期复查:把已不在本机构链上 Active 集合的管理员的会话清退。
///
/// 管理员"失效即时生效"靠此扫描(默认 45s,`ONCHINA_ADMIN_ONCHAIN_REVOKE_SECONDS` 可调)。
/// 链不可达时跳过本轮(绝不因瞬时断链批量清退);账户不存在(None)亦跳过(链未就绪保守处理)。
pub(crate) async fn revoke_stale_admin_sessions_loop(db: Db) {
    let interval_secs = std::env::var("ONCHINA_ADMIN_ONCHAIN_REVOKE_SECONDS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(45);
    let mut ticker = tokio::time::interval(std::time::Duration::from_secs(interval_secs));
    loop {
        ticker.tick().await;
        if let Err(err) = revoke_stale_admin_sessions_once(&db).await {
            tracing::warn!(error = %err, "on-chain admin session revocation sweep failed");
        }
    }
}

async fn revoke_stale_admin_sessions_once(db: &Db) -> Result<(), String> {
    let Some(binding) = repo::active_node_binding(db)? else {
        return Ok(());
    };
    let identity = chain_runtime::identity_from_binding_parts(
        &binding.institution_code,
        Some(binding.institution_cid_number.as_str()),
        binding.frg_province_code.as_deref(),
    )?;
    let Some(onchain_admins) = chain_runtime::fetch_active_admins_onchain(&identity).await? else {
        // 账户暂不存在(链未就绪/未配),不冒然清退本地会话。
        return Ok(());
    };
    let institution_code = binding.institution_code.clone();
    db.with_client(move |conn| {
        let account_ids = repo::list_session_admin_account_ids_conn(conn, &institution_code)?;
        for account_id in account_ids {
            let still_admin = onchain_admins
                .iter()
                .any(|admin| same_account_id(&admin.account_id, account_id.as_str()));
            if !still_admin {
                let removed =
                    repo::delete_admin_sessions_for_account_id_conn(conn, account_id.as_str())?;
                if removed > 0 {
                    tracing::info!(
                        account_id = %account_id,
                        sessions = removed,
                        "revoked sessions for admin no longer on-chain"
                    );
                }
            }
        }
        Ok(())
    })
}
