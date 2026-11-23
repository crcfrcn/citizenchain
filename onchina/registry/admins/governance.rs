//! 注册局直接登记机构管理员；对齐现有链调用字段，保持原签名与授权规则。
use crate::*;
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use onchina::authentication::repo as auth_repo;
use onchina::authorization::rules::ensure_registry_admin;
use onchina::institutions::{
    build_chain_sign_output, fetch_registration_roles_at, fetch_registration_snapshot,
    parse_registration_action, select_registration_role, InstitutionAdminInput,
    InstitutionAssignmentChangeInput, InstitutionChainSignRequest, InstitutionRoleMutationInput,
};
use onchina::institutions::{encode_register_institution_admins, RegisterInstitutionAdminsArgs};
use serde::Deserialize;
pub(crate) const PURPOSE_INSTITUTION_REGISTER_ADMINS: &str = "INSTITUTION_REGISTER_ADMINS";

/// 从实际call参数形成会话上下文；与统一submit的cid_number消费契约一致。
fn registration_context(
    args: &RegisterInstitutionAdminsArgs,
    block: [u8; 32],
    number: u32,
) -> Result<serde_json::Value, &'static str> {
    let cid = core::str::from_utf8(&args.cid_number).map_err(|_| "target CID UTF-8")?;
    let actor = core::str::from_utf8(&args.actor_cid_number).map_err(|_| "actor CID UTF-8")?;
    let role = core::str::from_utf8(&args.actor_role_code).map_err(|_| "actor role UTF-8")?;
    Ok(
        serde_json::json!({ "cid_number": cid, "actorCidNumber": actor, "actorRoleCode": role,
        "expectedStateHash": format!("0x{}",hex::encode(args.expected_state_hash)), "initialThreshold": args.initial_threshold,
        "finalizedBlockHash": format!("0x{}",hex::encode(block)), "finalizedBlockNumber": number, "op":"register_admins" }),
    )
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PrepareRegisterInstitutionAdminsInput {
    pub(crate) cid_number: String,
    pub(crate) admins: Option<Vec<InstitutionAdminInput>>,
    pub(crate) wallet_changes: Option<onchina::institutions::RegistryWalletOperationsInput>,
    #[serde(default)]
    pub(crate) actor_role_code: Option<String>,
    pub(crate) expected_state_hash: String,
    #[serde(default)]
    pub(crate) initial_threshold: Option<u32>,
    #[serde(default)]
    pub(crate) role_mutations: Vec<InstitutionRoleMutationInput>,
    #[serde(default)]
    pub(crate) assignment_changes: Vec<InstitutionAssignmentChangeInput>,
}

pub(crate) async fn prepare_register_institution_admins(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<PrepareRegisterInstitutionAdminsInput>,
) -> impl IntoResponse {
    let ctx = match require_admin_any(&state, &headers) {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    // 链上写(PasskeyColdSign 档):会话 + passkey + 钱包冷签,三者缺一不可。
    // 凭证向下传给冷签会话创建入口,由类型保证这一步不会被跳过。
    let passkey = match onchina::authentication::passkey::require_passkey_assertion(
        &state,
        &headers,
        ctx.account_id.as_str(),
    ) {
        Ok(proof) => proof,
        Err(resp) => return resp,
    };
    if let Err(resp) = ensure_registry_admin(&ctx) {
        return resp;
    }
    let binding = match auth_repo::active_node_binding(&state.db) {
        Ok(Some(binding)) => binding,
        Ok(None) => return api_error(StatusCode::FORBIDDEN, 2002, "not an on-chain admin"),
        Err(err) => {
            tracing::error!(error = %err, "query node binding failed");
            return api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                5001,
                "node binding query failed",
            );
        }
    };
    if binding.institution_code != ctx.institution_code
        || binding.institution_cid_number != ctx.institution_cid_number
    {
        return api_error(StatusCode::FORBIDDEN, 1003, "permission denied");
    }
    let cid_number = input.cid_number.trim();
    // expectedStateHash来自页面已审阅的快照，不能在prepare中用新状态静默替换旧名册。
    let expected_state_hash = match input
        .expected_state_hash
        .strip_prefix("0x")
        .filter(|s| {
            s.len() == 64
                && s.bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        })
        .and_then(|s| hex::decode(s).ok())
        .and_then(|v| <[u8; 32]>::try_from(v).ok())
    {
        Some(hash) => hash,
        None => {
            return api_error(
                StatusCode::BAD_REQUEST,
                1001,
                "登记状态指纹无效，请刷新页面",
            )
        }
    };
    let snapshot = match fetch_registration_snapshot(cid_number).await {
        Ok(snapshot) => snapshot,
        Err(error) => {
            tracing::error!(%error, "read registration snapshot failed");
            return api_error(StatusCode::BAD_GATEWAY, 5002, "登记链状态读取失败，请重试");
        }
    };
    if snapshot.state_hash() != expected_state_hash {
        return api_error(
            StatusCode::CONFLICT,
            1005,
            "机构状态已变化，请刷新并重新审阅签名",
        );
    }
    let actor = match onchina::chain_runtime::identity_from_binding_parts(
        &binding.institution_code,
        Some(&binding.institution_cid_number),
    ) {
        Ok(actor) => actor,
        Err(error) => return api_error(StatusCode::FORBIDDEN, 1003, &error),
    };
    let roles =
        match fetch_registration_roles_at(&actor, &ctx.account_id, cid_number, snapshot.block_hash)
            .await
        {
            Ok(roles) => roles,
            Err(error) => {
                tracing::error!(%error, "read registration authority failed");
                return api_error(StatusCode::BAD_GATEWAY, 5002, "办理岗位读取失败，请重试");
            }
        };
    let actor_role_code = match select_registration_role(&roles, input.actor_role_code.as_deref()) {
        Ok(role) => role,
        Err(error) => return api_error(StatusCode::FORBIDDEN, 1003, error),
    };
    let code = snapshot.institution.institution_code;
    let action = if let Some(ops) = &input.wallet_changes {
        if input.initial_threshold.is_some()
            || !input.role_mutations.is_empty()
            || !input.assignment_changes.is_empty()
        {
            return api_error(
                StatusCode::BAD_REQUEST,
                1001,
                "钱包维护不能夹带岗位、任职或阈值",
            );
        }
        match onchina::institutions::parse_registry_wallet_action(
            &state,
            ops,
            &snapshot,
            &actor_role_code,
        ) {
            Ok(v) => v,
            Err(e) => return e,
        }
    } else {
        if input.admins.is_some() {
            return api_error(StatusCode::BAD_REQUEST, 1001, "岗位配置不接收管理员名单");
        }
        match parse_registration_action(
            &input.role_mutations,
            &input.assignment_changes,
            &snapshot,
            input.initial_threshold,
        ) {
            Ok(v) => v,
            Err(e) => return e,
        }
    };
    if let entitybase::InstitutionGovernanceAction::RegisterAdminWalletChanges {
        added,
        removed,
        replaced,
    } = &action
    {
        let mut expected = snapshot.admins.clone().unwrap_or_default();
        expected.retain(|a| !removed.iter().any(|(w, _)| w == &a.account_id));
        for row in &mut expected {
            if let Some(change) = replaced.iter().find(|c| c.admin_account == row.account_id) {
                row.account_id = change.new_wallet;
            }
        }
        expected.extend(added.clone());
        let Some(reviewed) = input.admins.as_ref() else {
            return api_error(StatusCode::BAD_REQUEST, 1001, "钱包维护缺少审阅结果");
        };
        if expected.len() != reviewed.len()
            || expected.iter().zip(reviewed).any(|(a, i)| {
                onchina::authentication::login::parse_account_id_bytes(&i.account_id)
                    != Some(a.account_id)
                    || i.cid_number
                        .as_ref()
                        .is_some_and(|v| v.as_bytes() != a.cid_number.as_slice())
                    || i.family_name
                        .as_ref()
                        .is_some_and(|v| v.as_bytes() != a.family_name.as_slice())
                    || i.given_name
                        .as_ref()
                        .is_some_and(|v| v.as_bytes() != a.given_name.as_slice())
            })
        {
            return api_error(
                StatusCode::BAD_REQUEST,
                1001,
                "显示名册与明确的钱包操作不一致",
            );
        }
    }
    // 复用统一call 9；签名绑定完整action、原始状态指纹和显式初始阈值。
    let args = RegisterInstitutionAdminsArgs {
        is_private: snapshot.module_tag() == b"pri-mgmt",
        cid_number: cid_number.as_bytes().to_vec(),
        action,
        institution_code: code,
        actor_cid_number: binding.institution_cid_number.as_bytes().to_vec(),
        actor_role_code: actor_role_code.as_bytes().to_vec(),
        expected_state_hash,
        initial_threshold: input.initial_threshold,
    };
    let context = match registration_context(&args, snapshot.block_hash, snapshot.block_number) {
        Ok(context) => context,
        Err(error) => return api_error(StatusCode::INTERNAL_SERVER_ERROR, 5001, error),
    };
    let chain = encode_register_institution_admins(&args);
    let output = match build_chain_sign_output(
        &state,
        InstitutionChainSignRequest {
            account_id: ctx.account_id.as_str(),
            cid_number,
            purpose: PURPOSE_INSTITUTION_REGISTER_ADMINS,
            call_data: chain.call_data,
            chain_action: chain.action,
            context,
        },
        &passkey,
    )
    .await
    {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    onchina::audit_runtime::append_audit_log(
        &state,
        "INSTITUTION_REGISTER_ADMINS_PREPARE",
        &ctx.account_id,
        Some(cid_number.to_string()),
        serde_json::json!({ "cidNumber": cid_number, "actorIp": actor_ip_from_headers(&headers) }),
    );
    Json(ApiResponse {
        code: 0,
        message: "ok".to_string(),
        data: output,
    })
    .into_response()
}

#[cfg(test)]
mod registration_input_tests {
    use super::*;
    #[test]
    fn registration_requires_reviewed_snapshot_and_rejects_caller_provenance() {
        let base = serde_json::json!({"cidNumber":"NX013-CREGN-937676656-2026","admins":[],"expectedStateHash":format!("0x{}", "ab".repeat(32))});
        let input: PrepareRegisterInstitutionAdminsInput =
            serde_json::from_value(base.clone()).unwrap();
        assert!(input.actor_role_code.is_none());
        let mut missing = base.clone();
        missing.as_object_mut().unwrap().remove("expectedStateHash");
        assert!(serde_json::from_value::<PrepareRegisterInstitutionAdminsInput>(missing).is_err());
        let mut forged = base.clone();
        forged["assignmentChanges"] = serde_json::json!([{"roleCode":"R_ONE","assignments":[{
            "accountId":format!("0x{}", "01".repeat(32)), "assignmentSource":"GENESIS"
        }]}]);
        assert!(serde_json::from_value::<PrepareRegisterInstitutionAdminsInput>(forged).is_err());
        let mut lr = base;
        lr["representativeCid"] = "forged".into();
        assert!(serde_json::from_value::<PrepareRegisterInstitutionAdminsInput>(lr).is_err());
    }
    #[test]
    fn registration_context_keeps_the_actual_call_target_for_submit_and_recovery() {
        let mut args = RegisterInstitutionAdminsArgs {
            is_private: false,
            cid_number: b"NX013-CREGN-937676656-2026".to_vec(),
            institution_code: *b"CREG",
            action: entitybase::InstitutionGovernanceAction::ReplaceAdmins { admins: vec![] },
            actor_cid_number: b"ZS001-FRG07-249474503-2026".to_vec(),
            actor_role_code: b"R_ACTOR".to_vec(),
            expected_state_hash: [0xab; 32],
            initial_threshold: Some(2),
        };
        let context = registration_context(&args, [7; 32], 10).unwrap();
        // 统一提交/恢复按该键读取目标；不能以办理机构CID回落或返回空目标。
        let submit_target = context
            .get("cid_number")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        assert_eq!(submit_target.as_bytes(), args.cid_number);
        assert_eq!(
            context["actorCidNumber"].as_str().unwrap().as_bytes(),
            args.actor_cid_number
        );
        assert_eq!(
            context["expectedStateHash"],
            format!("0x{}", "ab".repeat(32))
        );
        assert_eq!(context["initialThreshold"], 2);
        assert_eq!(context["finalizedBlockNumber"], 10);
        args.cid_number = vec![0xff];
        assert!(registration_context(&args, [7; 32], 10).is_err());
    }
}

/// 实际广播前复核原签名中的登记方、岗位、辖区、目标与版本；不重新构造交易。
pub(crate) async fn recheck_registration(
    state: &AppState,
    ctx: &onchina::authentication::login::AdminAuthContext,
    session: &onchina::transactions::session::ChainSignSession,
) -> Result<(), axum::response::Response> {
    use codec::DecodeAll;
    let bad = || {
        api_error(
            StatusCode::FORBIDDEN,
            1003,
            "登记钱包、岗位、辖区或版本已变化",
        )
    };
    let data = session.call_data.get(2..).ok_or_else(bad)?;
    let (cid, action, actor, role, hash, initial): (
        Vec<u8>,
        entitybase::InstitutionGovernanceAction<[u8; 32]>,
        Vec<u8>,
        Vec<u8>,
        [u8; 32],
        Option<u32>,
    ) = DecodeAll::decode_all(&mut &data[..]).map_err(|_| bad())?;
    let target = core::str::from_utf8(&cid).map_err(|_| bad())?;
    let snapshot = fetch_registration_snapshot(target)
        .await
        .map_err(|_| bad())?;
    let binding = auth_repo::active_node_binding(&state.db)
        .map_err(|_| bad())?
        .ok_or_else(bad)?;
    if session.account_id != ctx.account_id
        || binding.institution_cid_number.as_bytes() != actor
        || snapshot.state_hash() != hash
        || session.call_data.get(..2)
            != Some(
                &[
                    if snapshot.module_tag() == b"pri-mgmt" {
                        31
                    } else {
                        30
                    },
                    9,
                ][..],
            )
    {
        return Err(bad());
    }
    let identity = onchina::chain_runtime::identity_from_binding_parts(
        &binding.institution_code,
        Some(&binding.institution_cid_number),
    )
    .map_err(|_| bad())?;
    let roles =
        fetch_registration_roles_at(&identity, &ctx.account_id, target, snapshot.block_hash)
            .await
            .map_err(|_| bad())?;
    if !roles.iter().any(|r| r.role_code.as_bytes() == role) {
        return Err(bad());
    }
    if matches!(
        action,
        entitybase::InstitutionGovernanceAction::RegisterAdminWalletChanges { .. }
    ) {
        if initial.is_some() {
            return Err(bad());
        }
        snapshot
            .validate_wallet_action(&action, &role)
            .map_err(|_| bad())?;
    } else if action.into_registration_parts().is_none() {
        // 不把旧完整名单输入当作钱包操作，也不重建已签名内容。
        return Err(bad());
    }
    Ok(())
}
