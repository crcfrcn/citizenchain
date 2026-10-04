//! 机构账户 HTTP handler。
//!
//! 机构自定义账户的新增/删除都不再本地直写:改为构造本机构内部投票提案的裸 call,
//! 由发起管理员使用签名钱包冷签一笔普通 extrinsic 上链,授权由 runtime 在 origin 处以
//! `is_institution_admin` + 岗位码(proposer_role_code)校验。账户读侧真源在链上
//! `PublicManage/PrivateManage::InstitutionAccounts`,本地 `accounts` 表不再作为读侧。

use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use serde::Serialize;

use crate::auth::login::{require_admin_any, AdminAuthContext};
use crate::auth::repo as auth_repo;
use crate::core::chain_runtime::{institution_accounts_lookup, OnChainInstitutionAccount};
use crate::core::institution_call::{
    encode_propose_add_institution_account, encode_propose_close_institution,
    ProposeAddInstitutionAccountArgs, ProposeCloseInstitutionArgs,
};
use crate::institution::accounts::derive::derive_account_bytes;
use crate::institution::admins::{
    build_chain_sign_output, code_bytes, InstitutionChainSignRequest,
};
use crate::institution::subjects::model::{CreateAccountInput, DeleteAccountInput};
use crate::institution::subjects::service::{
    institution_account_kind_label, is_protocol_account_name, validate_account_name,
    ACCOUNT_NAME_MAIN,
};
use crate::scope::get_visible_scope;
use crate::*;

/// 新增机构自定义账户提案用途。提交成功后读侧从链读,不落本地副本。
pub(crate) const PURPOSE_INSTITUTION_ADD_ACCOUNT: &str = "INSTITUTION_ADD_ACCOUNT";
/// 关闭机构自定义账户提案用途。提交成功后读侧从链读,不落本地副本。
pub(crate) const PURPOSE_INSTITUTION_CLOSE_ACCOUNT: &str = "INSTITUTION_CLOSE_ACCOUNT";

/// 新增/关闭账户提案共用的鉴权前置:节点绑定 + 只能操作本机构 + 岗位码合法 + 作用域校验。
///
/// 授权终局在 runtime(origin 处 `is_institution_admin` + 岗位码);本端只保证登录会话有效、
/// 存在链上机构绑定、目标机构就是本机构、岗位码非空且不超长、机构落在管理员作用域内。
/// 返回本机构 4 字节机构码(编码器据此选 pallet:公 30 / 私 31)。
async fn authorize_own_institution_proposal(
    state: &AppState,
    ctx: &AdminAuthContext,
    cid_number: &str,
    proposer_role_code: &str,
) -> Result<[u8; 4], axum::response::Response> {
    let binding = match auth_repo::active_node_binding(&state.db) {
        Ok(Some(binding)) => binding,
        Ok(None) => {
            return Err(api_error(
                StatusCode::FORBIDDEN,
                2002,
                "not an on-chain admin",
            ))
        }
        Err(err) => {
            tracing::error!(error = %err, "query node binding failed");
            return Err(api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                5001,
                "node binding query failed",
            ));
        }
    };
    if proposer_role_code.is_empty() || proposer_role_code.len() > 64 {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            1001,
            "proposer_role_code 长度必须为 1 到 64 字节",
        ));
    }
    if cid_number != binding.institution_cid_number
        || binding.institution_code != ctx.institution_code
    {
        return Err(api_error(StatusCode::FORBIDDEN, 1003, "只能操作本机构账户"));
    }
    let Some((inst, _)) = (match state.db.get_institution_with_accounts(cid_number) {
        Ok(value) => value,
        Err(err) => {
            tracing::error!(error = %err, "query institution failed");
            return Err(api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                5001,
                "机构查询失败",
            ));
        }
    }) else {
        return Err(api_error(StatusCode::NOT_FOUND, 1004, "机构不存在"));
    };
    let scope = get_visible_scope(ctx);
    if !scope.includes_province(&inst.province_name)
        || !scope.includes_city(&inst.city_name)
        || !scope.includes_town(&inst.town_name)
    {
        return Err(api_error(
            StatusCode::FORBIDDEN,
            1003,
            "institution out of current admin scope",
        ));
    }
    code_bytes(&inst.institution_code)
}

pub(crate) async fn create_account(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(cid_number): Path<String>,
    Json(input): Json<CreateAccountInput>,
) -> impl IntoResponse {
    let ctx = match require_admin_any(&state, &headers) {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    // 链上写(PasskeyColdSign 档):会话 + passkey + 钱包冷签,三者缺一不可。
    // 凭证向下传给冷签会话创建入口,由类型保证这一步不会被跳过。
    let passkey = match crate::auth::passkey::require_passkey_assertion(
        &state,
        &headers,
        ctx.account_id.as_str(),
    ) {
        Ok(proof) => proof,
        Err(resp) => return resp,
    };
    // 账户名格式校验 + 制度专属保留名拒绝(永久质押/安全基金/两和基金)。
    let account_name = match validate_account_name(&input.account_name) {
        Ok(v) => v,
        Err(e) => return crate::institution::subjects::http::service_error_to_response(e),
    };
    // 协议账户(主/费用/两和基金/安全基金/永久质押/清算)永不可由管理员新增。
    if is_protocol_account_name(&account_name) {
        return api_error(StatusCode::CONFLICT, 1007, "协议账户不可新增");
    }
    let cid_number = cid_number.trim().to_string();
    let proposer_role_code = input.proposer_role_code.trim().to_string();
    let code =
        match authorize_own_institution_proposal(&state, &ctx, &cid_number, &proposer_role_code)
            .await
        {
            Ok(v) => v,
            Err(resp) => return resp,
        };
    // 新增机构自定义账户 = 发起本机构内部投票提案(runtime call 7);由发起管理员使用签名钱包冷签,
    // OnChina 不再签发独立凭证,授权由 runtime 在 origin 处以 is_institution_admin + 岗位码校验。
    let chain = encode_propose_add_institution_account(&ProposeAddInstitutionAccountArgs {
        cid_number: cid_number.clone().into_bytes(),
        account_names: vec![account_name.clone().into_bytes()],
        institution_code: code,
        proposer_role_code: proposer_role_code.into_bytes(),
    });
    let output = match build_chain_sign_output(
        &state,
        InstitutionChainSignRequest {
            account_id: ctx.account_id.as_str(),
            cid_number: &cid_number,
            purpose: PURPOSE_INSTITUTION_ADD_ACCOUNT,
            call_data: chain.call_data,
            chain_action: chain.action,
            context: serde_json::json!({
                "cid_number": cid_number.clone(),
                "op": "add_account",
                "account_name": account_name.clone(),
            }),
        },
        &passkey,
    )
    .await
    {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    crate::core::runtime_ops::append_audit_log(
        &state,
        "INSTITUTION_ACCOUNT_ADD_PREPARE",
        &ctx.account_id,
        Some(cid_number.clone()),
        serde_json::json!({
            "cid_number": cid_number,
            "account_name": account_name,
        }),
    );
    Json(ApiResponse {
        code: 0,
        message: "ok".to_string(),
        data: output,
    })
    .into_response()
}

pub(crate) async fn list_accounts(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(cid_number): Path<String>,
) -> impl IntoResponse {
    let ctx = match require_admin_any(&state, &headers) {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    // 只读查询(Session 档):三档铁律规定读操作只要有效会话,不得要求 passkey。
    let cid_number = cid_number.trim().to_string();
    // 机构存在性与作用域仍以本地 subjects 表确认;账户明细一律从链上读取。
    let Some((inst, _)) = (match state.db.get_institution_with_accounts(&cid_number) {
        Ok(v) => v,
        Err(err) => {
            let message = format!("query institution failed: {err}");
            return api_error(StatusCode::INTERNAL_SERVER_ERROR, 5001, message.as_str());
        }
    }) else {
        return api_error(StatusCode::NOT_FOUND, 1004, "institution not found");
    };
    let scope = get_visible_scope(&ctx);
    if !scope.includes_province(&inst.province_name)
        || !scope.includes_city(&inst.city_name)
        || !scope.includes_town(&inst.town_name)
    {
        return api_error(StatusCode::FORBIDDEN, 1003, "out of admin scope");
    }
    let code = match code_bytes(&inst.institution_code) {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let accounts = match institution_accounts_lookup(&code, &cid_number).await {
        Ok(v) => v,
        Err(err) => {
            tracing::error!(error = %err, "read chain institution accounts failed");
            return api_error(StatusCode::BAD_GATEWAY, 1004, "链上账户读取失败");
        }
    };
    let rows = accounts.iter().map(chain_account_row).collect::<Vec<_>>();
    Json(ApiResponse {
        code: 0,
        message: "ok".to_string(),
        data: rows,
    })
    .into_response()
}

pub(crate) async fn delete_account(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((cid_number, account_name)): Path<(String, String)>,
    Json(input): Json<DeleteAccountInput>,
) -> impl IntoResponse {
    let ctx = match require_admin_any(&state, &headers) {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    // 链上写(PasskeyColdSign 档):会话 + passkey + 钱包冷签,三者缺一不可。
    // 凭证向下传给冷签会话创建入口,由类型保证这一步不会被跳过。
    let passkey = match crate::auth::passkey::require_passkey_assertion(
        &state,
        &headers,
        ctx.account_id.as_str(),
    ) {
        Ok(proof) => proof,
        Err(resp) => return resp,
    };
    // 协议账户(主/费用/两和基金/安全基金/永久质押/清算)永不可关闭。
    if is_protocol_account_name(&account_name) {
        return api_error(StatusCode::CONFLICT, 1007, "协议账户不可删除");
    }
    let cid_number = cid_number.trim().to_string();
    let proposer_role_code = input.proposer_role_code.trim().to_string();
    let code =
        match authorize_own_institution_proposal(&state, &ctx, &cid_number, &proposer_role_code)
            .await
        {
            Ok(v) => v,
            Err(resp) => return resp,
        };
    // 关闭账户提案需要待关闭账户与受益账户(本机构主账户)的裸 32 字节 AccountId。
    let Some(institution_account_id) = derive_account_bytes(&cid_number, &account_name) else {
        return api_error(StatusCode::INTERNAL_SERVER_ERROR, 5001, "账户地址派生失败");
    };
    let Some(beneficiary_account_id) = derive_account_bytes(&cid_number, ACCOUNT_NAME_MAIN) else {
        return api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            5001,
            "主账户地址派生失败",
        );
    };
    // 关闭机构自定义账户 = 发起本机构内部投票提案(runtime call 1);关闭执行时余额扫入本机构主账户,
    // 授权由 runtime 在 origin 处以 is_institution_admin + 岗位码校验。
    let chain = encode_propose_close_institution(&ProposeCloseInstitutionArgs {
        actor_cid_number: cid_number.clone().into_bytes(),
        proposer_role_code: proposer_role_code.into_bytes(),
        institution_account_id,
        beneficiary_account_id,
        institution_code: code,
    });
    let output = match build_chain_sign_output(
        &state,
        InstitutionChainSignRequest {
            account_id: ctx.account_id.as_str(),
            cid_number: &cid_number,
            purpose: PURPOSE_INSTITUTION_CLOSE_ACCOUNT,
            call_data: chain.call_data,
            chain_action: chain.action,
            context: serde_json::json!({
                "cid_number": cid_number.clone(),
                "op": "close_account",
                "account_name": account_name.clone(),
            }),
        },
        &passkey,
    )
    .await
    {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    crate::core::runtime_ops::append_audit_log(
        &state,
        "INSTITUTION_ACCOUNT_CLOSE_PREPARE",
        &ctx.account_id,
        Some(cid_number.clone()),
        serde_json::json!({
            "cid_number": cid_number,
            "account_name": account_name,
        }),
    );
    Json(ApiResponse {
        code: 0,
        message: "ok".to_string(),
        data: output,
    })
    .into_response()
}

/// 链读账户列表行。链上存在即视为 `ACTIVE_ON_CHAIN`;
/// `creator_account_id/created_at/tx_hash/block_number` 链上无对应字段,一律置空。
#[derive(Serialize)]
struct ChainAccountRow {
    cid_number: String,
    account_name: String,
    account_id: Option<String>,
    account_kind: &'static str,
    can_close: bool,
    can_delete: bool,
    chain_status: &'static str,
    chain_tx_hash: Option<String>,
    chain_block_number: Option<u32>,
    creator_account_id: Option<String>,
    created_at: Option<String>,
}

/// 把链上账户投影成规范 `account_id`。
fn chain_account_row(account_id: &OnChainInstitutionAccount) -> ChainAccountRow {
    let cid_number = String::from_utf8_lossy(&account_id.cid_number).into_owned();
    let account_name = String::from_utf8_lossy(&account_id.account_name).into_owned();
    // 只有自定义命名账户可关闭;协议账户 can_close/can_delete 恒为 false。
    let account_kind =
        institution_account_kind_label(&cid_number, &account_name).unwrap_or("named");
    let closable = account_kind == "named";
    ChainAccountRow {
        account_id: Some(format!("0x{}", hex::encode(account_id.account_id))),
        account_kind,
        can_close: closable,
        can_delete: closable,
        chain_status: "ACTIVE_ON_CHAIN",
        chain_tx_hash: None,
        chain_block_number: None,
        creator_account_id: None,
        created_at: None,
        cid_number,
        account_name,
    }
}
