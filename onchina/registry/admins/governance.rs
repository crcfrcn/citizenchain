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
use onchina::institutions::admins::{
    build_chain_sign_output, code_bytes, parse_admin_inputs, InstitutionAdminInput,
    InstitutionChainSignRequest,
};
use onchina::institutions_call::{
    encode_register_institution_admins, RegisterInstitutionAdminsArgs,
};
use serde::Deserialize;
pub(crate) const PURPOSE_INSTITUTION_REGISTER_ADMINS: &str = "INSTITUTION_REGISTER_ADMINS";

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PrepareRegisterInstitutionAdminsInput {
    pub(crate) cid_number: String,
    pub(crate) admins: Vec<InstitutionAdminInput>,
    pub(crate) actor_role_code: String,
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
    let actor_role_code = input.actor_role_code.trim();
    if actor_role_code.is_empty()
        || actor_role_code.len() > entitybase::INSTITUTION_ROLE_CODE_MAX_BYTES as usize
    {
        return api_error(
            StatusCode::BAD_REQUEST,
            1001,
            "actor_role_code 长度必须为 1 到 64 字节",
        );
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
    if binding.institution_code != ctx.institution_code {
        return api_error(StatusCode::FORBIDDEN, 1003, "permission denied");
    }
    let cid_number = input.cid_number.trim();
    let Some((inst, _)) = (match state.db.get_institution_with_accounts(cid_number) {
        Ok(value) => value,
        Err(err) => {
            tracing::error!(error = %err, "query institution failed");
            return api_error(StatusCode::INTERNAL_SERVER_ERROR, 5001, "机构查询失败");
        }
    }) else {
        return api_error(StatusCode::NOT_FOUND, 1004, "机构不存在");
    };
    let code = match code_bytes(&inst.institution_code) {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let is_public = !primitives::cid::code::is_private_legal_code(&code);
    let admins = match parse_admin_inputs(&state, &input.admins, is_public) {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    // 管理员登记 = 注册局管理员使用签名钱包直接冷签这笔 extrinsic,
    // 授权由 runtime 在 origin 处以 `can_register_institution_origin`(注册局在册管理员 +
    // 对目标机构有登记权)校验。
    let chain = encode_register_institution_admins(&RegisterInstitutionAdminsArgs {
        cid_number: cid_number.as_bytes().to_vec(),
        admins,
        institution_code: code,
        actor_cid_number: binding.institution_cid_number.as_bytes().to_vec(),
        actor_role_code: actor_role_code.as_bytes().to_vec(),
    });
    let output = match build_chain_sign_output(
        &state,
        InstitutionChainSignRequest {
            account_id: ctx.account_id.as_str(),
            cid_number,
            purpose: PURPOSE_INSTITUTION_REGISTER_ADMINS,
            call_data: chain.call_data,
            chain_action: chain.action,
            context: serde_json::json!({
                "cid_number": cid_number,
                "actor_cid_number": binding.institution_cid_number,
                "actor_role_code": actor_role_code,
                "op": "register_admins"
            }),
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
