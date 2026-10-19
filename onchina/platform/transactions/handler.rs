//! 平台统一链交易提交与恢复；业务投影仍调用原所属业务实现。
use axum::{extract::State, http::{HeaderMap, StatusCode}, response::IntoResponse, Json};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use crate::authentication::login::AdminAuthContext;
use crate::core::chain_submit;
use crate::core::chain_submit::parse_hex32;
use crate::crypto::pubkey::same_account_id;
use super::session::{ChainSignSession, SessionLease};
use crate::*;

#[derive(Deserialize, Serialize)]
pub(crate) struct ChainSubmitInput {
    pub(crate) request_id: String,
    /// 冷钱包扫码回签(前端已从响应 QR 解析);后端按会话签名字节重新验签。
    pub(crate) account_id: String,
    pub(crate) signature: String,
}

#[derive(Serialize)]
pub(crate) struct ChainSubmitOutput {
    pub(crate) purpose: String,
    pub(crate) cid_number: String,
    pub(crate) tx_hash: String,
    pub(crate) block_number: Option<u64>,
    pub(crate) citizen: Option<serde_json::Value>,
}

pub(crate) fn delete_session_best_effort(state: &AppState, request_id: &str, reason: &str) {
    if let Err(err) = state.db.delete_chain_sign_session(request_id) {
        tracing::error!(
            error = %err,
            request_id = %request_id,
            reason = %reason,
            "delete chain sign session failed"
        );
    }
}

fn stored_submission_attempt(
    session: &ChainSignSession,
) -> Option<chain_submit::SubmissionAttempt> {
    let recovery_cursor_block_number = session
        .context
        .get("recovery_cursor_block_number")
        .and_then(|value| value.as_u64());
    let recovery_cursor_block_hash = session
        .context
        .get("recovery_cursor_block_hash")
        .and_then(|value| value.as_str())
        .and_then(parse_hex32);
    if recovery_cursor_block_number.is_some() != recovery_cursor_block_hash.is_some() {
        return None;
    }
    let tx_hash = session.context.get("submitted_tx_hash")?.as_str()?;
    parse_hex32(tx_hash)?;
    Some(chain_submit::SubmissionAttempt {
        tx_hash: tx_hash.to_string(),
        from_block_number: session
            .context
            .get("submitted_from_block_number")?
            .as_u64()?,
        from_block_hash: session
            .context
            .get("submitted_from_block_hash")?
            .as_str()
            .and_then(parse_hex32)?,
        recovery_cursor_block_number,
        recovery_cursor_block_hash,
    })
}

fn stored_finalized_submit(
    session: &ChainSignSession,
) -> Option<chain_submit::FinalizedChainSubmit> {
    let tx_hash = session.context.get("finalized_tx_hash")?.as_str()?;
    parse_hex32(tx_hash)?;
    Some(chain_submit::FinalizedChainSubmit {
        tx_hash: tx_hash.to_string(),
        block_number: session.context.get("finalized_block_number")?.as_u64()?,
        block_hash: session
            .context
            .get("finalized_block_hash")?
            .as_str()
            .and_then(parse_hex32)?,
    })
}

#[derive(Debug, PartialEq, Eq)]
enum ChainResumePhase {
    /// 尚未尝试网络提交，可以执行完整的提交前授权复查和 dry-run。
    Submit,
    /// 已持久化提交哈希与 finalized 锚点，必须先恢复结果，禁止直接盲目重提。
    Recover(chain_submit::SubmissionAttempt),
    /// 链上结果已确认并持久化，本次重试只能核对 finalized 状态并补本地投影。
    Project(chain_submit::FinalizedChainSubmit),
}

fn chain_resume_phase(session: &ChainSignSession) -> Result<ChainResumePhase, String> {
    let has_finalized_marker = [
        "finalized_tx_hash",
        "finalized_block_number",
        "finalized_block_hash",
    ]
    .iter()
    .any(|key| session.context.get(*key).is_some());
    if has_finalized_marker {
        return stored_finalized_submit(session)
            .map(ChainResumePhase::Project)
            .ok_or_else(|| {
                "finalized chain submit context is incomplete or malformed".to_string()
            });
    }

    let has_submission_marker = [
        "submitted_tx_hash",
        "submitted_from_block_number",
        "submitted_from_block_hash",
        "recovery_cursor_block_number",
        "recovery_cursor_block_hash",
    ]
    .iter()
    .any(|key| session.context.get(*key).is_some());
    if has_submission_marker {
        return stored_submission_attempt(session)
            .map(ChainResumePhase::Recover)
            .ok_or_else(|| "submission recovery context is incomplete or malformed".to_string());
    }

    Ok(ChainResumePhase::Submit)
}

fn persist_submission_attempt(
    session: &ChainSignSession,
    lease: &SessionLease,
    attempt: &chain_submit::SubmissionAttempt,
) -> Result<(), String> {
    let mut context = session.context.clone();
    let map = context
        .as_object_mut()
        .ok_or_else(|| "chain sign session context must be object".to_string())?;
    map.insert(
        "submitted_tx_hash".to_string(),
        serde_json::Value::String(attempt.tx_hash.clone()),
    );
    map.insert(
        "submitted_from_block_number".to_string(),
        serde_json::Value::from(attempt.from_block_number),
    );
    map.insert(
        "submitted_from_block_hash".to_string(),
        serde_json::Value::String(format!("0x{}", hex::encode(attempt.from_block_hash))),
    );
    // 每次真正重提都会建立新的 finalized 锚点；旧扫描游标必须清除。
    map.remove("recovery_cursor_block_number");
    map.remove("recovery_cursor_block_hash");
    lease.update_context(&context)
}

fn persist_recovery_cursor(
    session: &ChainSignSession,
    lease: &SessionLease,
    cursor_block_number: u64,
    cursor_block_hash: [u8; 32],
) -> Result<(), String> {
    let mut context = session.context.clone();
    let map = context
        .as_object_mut()
        .ok_or_else(|| "chain sign session context must be object".to_string())?;
    map.insert(
        "recovery_cursor_block_number".to_string(),
        serde_json::Value::from(cursor_block_number),
    );
    map.insert(
        "recovery_cursor_block_hash".to_string(),
        serde_json::Value::String(format!("0x{}", hex::encode(cursor_block_hash))),
    );
    lease.update_context(&context)
}

fn clear_recovery_cursor(session: &ChainSignSession, lease: &SessionLease) -> Result<(), String> {
    let mut context = session.context.clone();
    let map = context
        .as_object_mut()
        .ok_or_else(|| "chain sign session context must be object".to_string())?;
    map.remove("recovery_cursor_block_number");
    map.remove("recovery_cursor_block_hash");
    lease.update_context(&context)
}

fn persist_finalized_submit(
    session: &ChainSignSession,
    lease: &SessionLease,
    finalized: &chain_submit::FinalizedChainSubmit,
) -> Result<(), String> {
    let mut context = session.context.clone();
    let map = context
        .as_object_mut()
        .ok_or_else(|| "chain sign session context must be object".to_string())?;
    map.insert(
        "finalized_tx_hash".to_string(),
        serde_json::Value::String(finalized.tx_hash.clone()),
    );
    map.insert(
        "finalized_block_number".to_string(),
        serde_json::Value::from(finalized.block_number),
    );
    map.insert(
        "finalized_block_hash".to_string(),
        serde_json::Value::String(format!("0x{}", hex::encode(finalized.block_hash))),
    );
    map.remove("recovery_cursor_block_number");
    map.remove("recovery_cursor_block_hash");
    lease.update_context(&context)
}

async fn submit_with_persisted_attempt(
    session: &ChainSignSession,
    signature: &str,
    lease: &SessionLease,
) -> Result<chain_submit::FinalizedChainSubmit, chain_submit::ChainSubmitError> {
    chain_submit::assemble_and_submit(
        &session.call_data,
        session.account_id.as_str(),
        signature,
        session.nonce,
        session.signing_hash.as_str(),
        |attempt| persist_submission_attempt(session, lease, attempt),
    )
    .await
}

/// 首次提交和安全重提共用同一套应用层授权复查；finalized 恢复与投影补写不复查，
/// 否则管理员在链交易成功后被撤权会导致本地投影永久无法完成。
async fn recheck_chain_submit_authorization(state: &AppState, ctx: &AdminAuthContext, session: &ChainSignSession) -> Result<(), axum::response::Response> {
    if let Some(owner) = state.modules.owner_of_purpose(&session.purpose) {
        if state.modules.local_owns(&owner) {
            return state.modules.local.as_ref().ok_or_else(|| api_error(StatusCode::SERVICE_UNAVAILABLE, 1503, "module unavailable"))?.recheck_submit(state, ctx, session).await;
        }
        return Err(api_error(StatusCode::SERVICE_UNAVAILABLE, 1503, "module unavailable"));
    }
    if builtin_purposes().contains(&session.purpose.as_str()) && state.modules.local.is_none() { return Ok(()); }
    Err(api_error(StatusCode::SERVICE_UNAVAILABLE, 1503, "module unavailable"))
}

/// 统一链交易 submit:验签者一致 → 组装/dry-run/提交 → finalized 成功 → 按 purpose 落正式投影。
pub(crate) async fn submit_chain_sign(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<ChainSubmitInput>,
) -> impl IntoResponse {
    let ctx = match require_admin_any(&state, &headers) {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    // 链上写(PasskeyColdSign 档)由两原语搭成:passkey 在冷签会话创建那一步消费
    // (由 `PasskeyProof` 类型强制),钱包冷签在本步消费。故本步不再要求第二次
    // passkey——那会变成"一个操作两次 passkey",与三档契约的"一次"相违。
    // 本步的会话归属校验(session.account_id == ctx.account_id)+ 签名者一致校验
    // + 链上授权复核共同保证提交方就是发起方。
    let session = match state.db.find_chain_sign_session(input.request_id.as_str()) {
        Ok(Some(v)) => v,
        Ok(None) => return api_error(StatusCode::NOT_FOUND, 1004, "冷签会话不存在"),
        Err(err) => {
            tracing::error!(error = %err, "query chain sign session failed");
            return api_error(StatusCode::INTERNAL_SERVER_ERROR, 1004, "冷签会话查询失败");
        }
    };
    if let Some(owner) = state.modules.owner_of_purpose(&session.purpose) {
        if !state.modules.local_owns(&owner) {
            return crate::modules::forward_json(&state, &owner, "/api/admin/chain/submit", &headers, &input).await;
        }
    } else if !builtin_purposes().contains(&session.purpose.as_str()) || state.modules.local.is_some() {
        return api_error(StatusCode::SERVICE_UNAVAILABLE, 1503, "module unavailable");
    }
    if session.consumed_at.is_some() {
        delete_session_best_effort(&state, &session.request_id, "consumed residue");
        return api_error(StatusCode::CONFLICT, 1005, "冷签会话已被消费");
    }
    let lease = match SessionLease::acquire(&state.db, &input.request_id) {
        Ok(Some(lease)) => lease,
        Ok(None) => return api_error(StatusCode::CONFLICT, 1005, "冷签会话不存在、已完成或正在处理中"),
        Err(_) => return api_error(StatusCode::INTERNAL_SERVER_ERROR, 1004, "冷签会话查询失败"),
    };
    let session = match state.db.find_chain_sign_session(&input.request_id) {
        Ok(Some(session)) => session,
        Ok(None) => return api_error(StatusCode::NOT_FOUND, 1004, "冷签会话不存在"),
        Err(_) => return api_error(StatusCode::INTERNAL_SERVER_ERROR, 1004, "冷签会话查询失败"),
    };
    if session.consumed_at.is_some() {
        delete_session_best_effort(&state, session.request_id.as_str(), "consumed residue");
        return api_error(StatusCode::CONFLICT, 1005, "冷签会话已被消费");
    }
    let resume_phase = match chain_resume_phase(&session) {
        Ok(value) => value,
        Err(err) => {
            tracing::error!(error = %err, "chain resume context invalid");
            return api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                1004,
                "冷签会话的链恢复数据损坏，已保留会话并禁止重提",
            );
        }
    };
    let has_chain_attempt = !matches!(resume_phase, ChainResumePhase::Submit);
    if session.expires_at < Utc::now() && !has_chain_attempt {
        delete_session_best_effort(&state, session.request_id.as_str(), "expired");
        return api_error(StatusCode::GONE, 1005, "冷签会话已过期,请重新发起");
    }
    if !same_account_id(session.account_id.as_str(), ctx.account_id.as_str()) {
        if !has_chain_attempt {
            delete_session_best_effort(&state, session.request_id.as_str(), "actor mismatch");
        }
        return api_error(StatusCode::FORBIDDEN, 1003, "只有发起管理员可以提交本会话");
    }
    if !same_account_id(input.account_id.as_str(), session.account_id.as_str()) {
        if !has_chain_attempt {
            delete_session_best_effort(&state, session.request_id.as_str(), "signer mismatch");
        }
        return api_error(StatusCode::FORBIDDEN, 1003, "签名钱包与会话管理员不一致");
    }
    let origin_cid = session.context.get("origin_institution_cid_number").and_then(|value| value.as_str());
    if origin_cid != Some(ctx.institution_cid_number.as_str()) {
        return api_error(StatusCode::FORBIDDEN, 1003, "冷签会话不属于当前机构");
    }
    if !has_chain_attempt {
        if let Err(resp) = recheck_chain_submit_authorization(&state, &ctx, &session).await {
            delete_session_best_effort(
                &state,
                session.request_id.as_str(),
                "chain submit authorization recheck failed",
            );
            return resp;
        }
    }

    if let Some(module) = &state.modules.local {
        if let Err(err) = module.validate_submit_context(&session) {
            tracing::error!(error = %err, "finalized binding context invalid");
            if !has_chain_attempt { delete_session_best_effort(&state, &session.request_id, "finalized binding context invalid"); }
            return api_error(StatusCode::INTERNAL_SERVER_ERROR, 1004, "CID 绑定会话数据损坏");
        }
    }
    let had_stored_finalized = matches!(resume_phase, ChainResumePhase::Project(_));
    let submit_result = match resume_phase {
        ChainResumePhase::Project(finalized) => match chain_submit::verify_finalized_submit(&finalized).await {
            Ok(()) => Ok(finalized),
            Err(error) => return api_error(StatusCode::BAD_GATEWAY, 2004, &error),
        },
        ChainResumePhase::Recover(attempt) => {
            match chain_submit::find_finalized_success(
                attempt.tx_hash.as_str(),
                attempt.from_block_number,
                attempt.from_block_hash,
                attempt.recovery_cursor_block_number,
                attempt.recovery_cursor_block_hash,
            )
            .await
            {
                Ok(chain_submit::FinalizedRecoveryScan::Found(finalized)) => Ok(finalized),
                Ok(chain_submit::FinalizedRecoveryScan::Continue {
                    cursor_block_number,
                    cursor_block_hash,
                }) => {
                    if let Err(err) = persist_recovery_cursor(
                        &session,
                        &lease,
                        cursor_block_number,
                        cursor_block_hash,
                    ) {
                        tracing::error!(error = %err, "persist finalized recovery cursor failed");
                        return api_error(
                            StatusCode::INTERNAL_SERVER_ERROR,
                            2004,
                            "finalized 恢复游标保存失败；会话已保留，请重试",
                        );
                    }
                    let detail = format!(
                        "交易 {} 的 finalized 历史仍在分批核对；本批游标已保存到区块 {}，请用同一请求继续",
                        attempt.tx_hash, cursor_block_number
                    );
                    return api_error(StatusCode::BAD_GATEWAY, 2004, detail.as_str());
                }
                Ok(chain_submit::FinalizedRecoveryScan::Exhausted) => {
                    // 已完整扫描到原始 finalized 锚点。先清除旧游标，让后续任何失败重试
                    // 都会从新的 finalized head 再扫，避免漏掉扫描期间刚进块的原交易。
                    if let Err(err) = clear_recovery_cursor(&session, &lease) {
                        tracing::error!(error = %err, "clear finalized recovery cursor failed");
                        return api_error(
                            StatusCode::INTERNAL_SERVER_ERROR,
                            2004,
                            "finalized 恢复游标重置失败；会话已保留并禁止重提",
                        );
                    }

                    // system_accountNextIndex 等于原 nonce，才表明查询节点尚未消费该 nonce，
                    // 且未把同 nonce 计入本地交易池；此时重发完全相同的 extrinsic（同一
                    // tx hash）不会创建第二个业务动作。nonce 不等时只恢复，禁止盲重放。
                    let nonce_is_available = match chain_submit::fetch_nonce(
                        session.account_id.as_str(),
                    )
                    .await
                    {
                        Ok(nonce) => nonce == session.nonce,
                        Err(err) => {
                            tracing::error!(error = %err, "read nonce for safe resubmit failed");
                            false
                        }
                    };
                    let target_is_pending = match &state.modules.local {
                        Some(module) => module.safe_to_resubmit(&session).await,
                        None => true,
                    };
                    if nonce_is_available && target_is_pending {
                        // 首次提交和重提都复用相同授权复查；恢复/投影本身不依赖当前岗位。
                        if let Err(resp) =
                            recheck_chain_submit_authorization(&state, &ctx, &session).await
                        {
                            return resp;
                        }
                        submit_with_persisted_attempt(&session, input.signature.as_str(), &lease)
                            .await
                    } else {
                        let detail = format!(
                            "交易 {} 已完整扫描到提交锚点但当前不满足安全重提条件；已重置恢复游标并保留会话，稍后重试将从新的 finalized head 核对",
                            attempt.tx_hash
                        );
                        return api_error(StatusCode::BAD_GATEWAY, 2004, detail.as_str());
                    }
                }
                Err(err) => {
                    tracing::error!(
                        error = %err,
                        tx_hash = %attempt.tx_hash,
                        "recover finalized chain submit failed"
                    );
                    let detail = format!(
                        "交易 {} 的 finalized 结果暂无法恢复；会话与待落库数据已保留: {err}",
                        attempt.tx_hash
                    );
                    return api_error(StatusCode::BAD_GATEWAY, 2004, detail.as_str());
                }
            }
        }
        ChainResumePhase::Submit => {
            submit_with_persisted_attempt(&session, input.signature.as_str(), &lease).await
        }
    };
    let finalized_submit = match submit_result {
        Ok(finalized) => finalized,
        Err(err) => {
            tracing::error!(error = %err, "chain submit failed");
            if let Some(tx_hash) = err.submitted_tx_hash() {
                crate::core::runtime_ops::append_audit_log(
                    &state,
                    "CHAIN_SUBMIT_FINALIZED_UNCERTAIN",
                    &ctx.account_id,
                    session
                        .context
                        .get("cid_number")
                        .and_then(|value| value.as_str())
                        .map(str::to_string),
                    serde_json::json!({
                        "request_id": session.request_id.clone(),
                        "purpose": session.purpose.clone(),
                        "tx_hash": tx_hash,
                        "error": err.to_string(),
                    }),
                );
                let detail = format!(
                    "交易提交结果未知({tx_hash})；会话与待落库数据已保留，请稍后用同一请求重试"
                );
                return api_error(StatusCode::BAD_GATEWAY, 2004, detail.as_str());
            }
            if has_chain_attempt {
                let detail = format!(
                    "恢复后的安全重提在网络提交前被拒绝；原提交锚点与会话仍保留，将继续按 finalized 结果恢复: {err}"
                );
                return api_error(StatusCode::UNPROCESSABLE_ENTITY, 2004, detail.as_str());
            }
            delete_session_best_effort(
                &state,
                session.request_id.as_str(),
                "chain submit rejected before submission",
            );
            let detail = format!("链交易提交失败: {err}");
            return api_error(StatusCode::UNPROCESSABLE_ENTITY, 2004, detail.as_str());
        }
    };
    finish_finalized_session(&state, &headers, &session, &finalized_submit, had_stored_finalized, &lease).await
}

/// 只消费已证实的链上结果；不读取当前成员权限、不重新签名或提交网络交易。
async fn finish_finalized_session(
    state: &AppState, headers: &HeaderMap, session: &ChainSignSession,
    finalized_submit: &chain_submit::FinalizedChainSubmit, had_stored_finalized: bool, lease: &SessionLease,
) -> axum::response::Response {
    let tx_hash = finalized_submit.tx_hash.clone();
    let finalized_cid_snapshot = match &state.modules.local {
        Some(module) => match module.verify_finalized_binding(session, finalized_submit).await { Ok(value) => value, Err(response) => return response },
        None => None,
    };
    if !had_stored_finalized {
        if let Err(err) = persist_finalized_submit(session, lease, finalized_submit) {
            tracing::error!(
                error = %err,
                tx_hash = %tx_hash,
                "persist finalized chain submit state failed"
            );
            let detail = format!(
                "交易 {tx_hash} 已 finalized 成功，但本地两阶段状态保存失败；提交锚点与待落库数据仍保留，禁止重提"
            );
            return api_error(StatusCode::INTERNAL_SERVER_ERROR, 2004, detail.as_str());
        }
    }
    let block_number = Some(finalized_submit.block_number);

    let cid_number = session
        .context
        .get("cid_number")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();

    // 业务模块回写与共有会话完成器保持同一事务；恢复不要求原管理员仍在职。
    let result = match &state.modules.local {
        Some(module) => module.commit_projection(&state.db, session, lease, finalized_submit, finalized_cid_snapshot.as_ref()).await,
        None => commit_projection(&state.db, session, lease, &tx_hash, |_| Ok(None)).map_err(|error| {
            tracing::error!(error = %error, "finalized business projection retained for recovery");
            api_error(StatusCode::INTERNAL_SERVER_ERROR, 1004, "链上交易已完成，本地投影尚未完成，会话已保留")
        }),
    };
    let citizen_output = match result { Ok(output) => output, Err(response) => return response };
    // 已完成事务后才清理短期会话；清理失败也不会再次执行投影。
    delete_session_best_effort(state, &session.request_id, "completed");
    if let Some(citizen) = &citizen_output {
        crate::core::runtime_ops::append_audit_log(state, "CITIZEN_CREATE", &session.account_id, Some(cid_number.clone()), serde_json::json!({
            "cid_number":cid_number,"passport_no":citizen["passport_no"],"family_name":citizen["family_name"],"given_name":citizen["given_name"],
            "province_code":citizen["province_code"],"city_code":citizen["city_code"],"town_code":citizen["town_code"],
            "birth_province_code":citizen["birth_province_code"],"birth_city_code":citizen["birth_city_code"],"birth_town_code":citizen["birth_town_code"],
            "voting_eligible":citizen["voting_eligible"],"onchain_tx_hash":tx_hash,
            "request_id":request_id_from_headers(headers),"actor_ip":actor_ip_from_headers(headers),
        }));
    }

    crate::core::runtime_ops::append_audit_log(
        &state,
        "CHAIN_SIGN_SUBMIT",
        &session.account_id,
        Some(cid_number.clone()),
        serde_json::json!({
            "purpose": session.purpose,
            "cid_number": cid_number,
            "tx_hash": tx_hash,
            "block_number": block_number,
            "request_id": session.request_id,
            "actor_ip": actor_ip_from_headers(&headers),
        }),
    );

    Json(ApiResponse {
        code: 0,
        message: "ok".to_string(),
        data: ChainSubmitOutput {
            purpose: session.purpose.clone(),
            cid_number,
            tx_hash,
            block_number,
            citizen: citizen_output,
        },
    })
    .into_response()
}


pub(crate) fn commit_projection(
    db: &Db, session: &ChainSignSession, lease: &SessionLease, tx_hash: &str,
    apply: impl FnOnce(&mut postgres::Transaction<'_>) -> Result<Option<serde_json::Value>, String> + Send,
) -> Result<Option<serde_json::Value>, String> {
    db.with_client(|conn| {
        let mut tx = conn.transaction().map_err(|e| format!("begin chain completion failed: {e}"))?;
        crate::authentication::repo::lock_node_binding_conn(&mut tx)?;
        lease.verify_conn(&mut tx)?;
        let binding = crate::authentication::repo::get_active_node_binding_conn(&mut tx)?.ok_or("chain completion requires original institution binding")?;
        if !origin_matches_binding(session, &binding) { return Err("chain session original institution does not match current binding".into()); }
        let output = apply(&mut tx)?;
        let updated = tx.execute("UPDATE chain_sign_sessions SET consumed_at=now() WHERE request_id=$1 AND consumed_at IS NULL AND context->>'finalized_tx_hash'=$2", &[&session.request_id,&tx_hash]).map_err(|e| e.to_string())?;
        if updated != 1 { return Err("chain completion did not consume exactly one session".into()); }
        tx.commit().map_err(|e| format!("commit chain completion failed: {e}"))?;
        Ok(output)
    })
}

pub(crate) fn builtin_purposes() -> Vec<&'static str> {
    vec![crate::institutions::admins::PURPOSE_INSTITUTION_GOVERNANCE,
        crate::institutions::accounts::handler::PURPOSE_INSTITUTION_ADD_ACCOUNT,
        crate::institutions::accounts::handler::PURPOSE_INSTITUTION_CLOSE_ACCOUNT]
}


fn origin_matches_binding(session: &ChainSignSession, binding: &crate::authentication::login::NodeInstitutionBinding) -> bool {
    session.context.get("origin_binding_id").and_then(|value| value.as_str()).is_some_and(|id| !id.is_empty())
        && session.context.get("origin_institution_cid_number").and_then(|value| value.as_str()) == Some(binding.institution_cid_number.as_str())
        && session.context.get("origin_institution_code").and_then(|value| value.as_str()) == Some(binding.institution_code.as_str())
}

/// 后台只恢复已留有提交/最终确认锚点的会话；不需要原管理员仍在职，不会代替用户签名或重提。
pub(crate) async fn recover_chain_sessions_loop(state: AppState) {
    loop {
        if let Err(error) = recover_chain_sessions_once(&state).await { tracing::warn!(error = %error, "chain session recovery paused"); }
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
    }
}

async fn recover_chain_sessions_once(state: &AppState) -> Result<(), String> {
    let purposes = if state.modules.local.is_some() { state.modules.local_purposes() } else { builtin_purposes().into_iter().map(str::to_string).collect() };
    let ids = state.db.with_client(|conn| conn.query("SELECT request_id FROM chain_sign_sessions WHERE purpose=ANY($1) AND consumed_at IS NULL AND (context ? 'submitted_tx_hash' OR context ? 'finalized_tx_hash') ORDER BY recovery_checked_at NULLS FIRST,created_at LIMIT 32", &[&purposes])
        .map(|rows| rows.iter().map(|row| row.get::<_, String>(0)).collect::<Vec<_>>()).map_err(|e| e.to_string()))?;
    for id in ids {
        if let Err(error) = recover_one_chain_session(state, &id).await { tracing::warn!(request_id = %id, error = %error, "chain session recovery retained for retry"); }
    }
    Ok(())
}

async fn recover_one_chain_session(state: &AppState, id: &str) -> Result<(), String> {
        let Some(lease) = SessionLease::acquire(&state.db, id)? else { return Ok(()); };
        let Some(session) = state.db.find_chain_sign_session(id)? else { return Ok(()); };
        let Some(binding) = crate::authentication::repo::active_node_binding(&state.db)? else { return Ok(()); };
        if !origin_matches_binding(&session, &binding) { return Ok(()); }
        let finalized = match chain_resume_phase(&session)? {
            ChainResumePhase::Submit => return Ok(()),
            ChainResumePhase::Project(finalized) => { chain_submit::verify_finalized_submit(&finalized).await?; finalized }
            ChainResumePhase::Recover(attempt) => match chain_submit::find_finalized_success(&attempt.tx_hash, attempt.from_block_number, attempt.from_block_hash, attempt.recovery_cursor_block_number, attempt.recovery_cursor_block_hash).await? {
                chain_submit::FinalizedRecoveryScan::Found(finalized) => { persist_finalized_submit(&session, &lease, &finalized)?; finalized }
                chain_submit::FinalizedRecoveryScan::Continue { cursor_block_number, cursor_block_hash } => { persist_recovery_cursor(&session, &lease, cursor_block_number, cursor_block_hash)?; return Ok(()); }
                chain_submit::FinalizedRecoveryScan::Exhausted => { clear_recovery_cursor(&session, &lease)?; return Ok(()); }
            }
        };
        let _ = finish_finalized_session(state, &HeaderMap::new(), &session, &finalized, true, &lease).await;
    Ok(())
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
#[path = "handler_tests.rs"]
mod tests;
