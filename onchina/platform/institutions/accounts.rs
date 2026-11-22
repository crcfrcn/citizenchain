//! 查询和派生机构账户，并构造账户新增、关闭的治理提案。

use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use serde::Serialize;

use crate::authentication::login::{require_admin_any, AdminAuthContext};
use crate::authentication::repo as auth_repo;
use crate::authorization::get_visible_scope;
use crate::chain_runtime::{institution_accounts_lookup, OnChainInstitutionAccount};
use crate::institutions::{build_chain_sign_output, code_bytes, InstitutionChainSignRequest};
use crate::institutions::{
    encode_propose_add_institution_account, encode_propose_close_institution,
    ProposeAddInstitutionAccountArgs, ProposeCloseInstitutionArgs,
};
use crate::institutions::{
    institution_account_kind_label, is_protocol_account_name, validate_account_name,
    ACCOUNT_NAME_MAIN,
};
use crate::institutions::{CreateAccountInput, DeleteAccountInput};
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
    if !scope.includes_institution(
        &inst.cid_number,
        &inst.province_name,
        &inst.city_name,
        &inst.town_name,
    ) {
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
    let passkey = match crate::authentication::passkey::require_passkey_assertion(
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
        Err(e) => return crate::institutions::service_error_to_response(e),
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
    crate::audit_runtime::append_audit_log(
        &state,
        "INSTITUTION_ACCOUNT_ADD_PREPARE",
        &ctx.account_id,
        Some(cid_number.clone()),
        serde_json::json!({
            "cidNumber": cid_number,
            "accountName": account_name,
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
    if !scope.includes_institution(
        &inst.cid_number,
        &inst.province_name,
        &inst.city_name,
        &inst.town_name,
    ) {
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
    let rows = chain_account_rows(&accounts).await;
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
    let passkey = match crate::authentication::passkey::require_passkey_assertion(
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
    crate::audit_runtime::append_audit_log(
        &state,
        "INSTITUTION_ACCOUNT_CLOSE_PREPARE",
        &ctx.account_id,
        Some(cid_number.clone()),
        serde_json::json!({
            "cidNumber": cid_number,
            "accountName": account_name,
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
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChainAccountRow {
    cid_number: String,
    account_name: String,
    account_id: Option<String>,
    /// finalized System.Account.data.free，单位为分；不可用时保持 None。
    balance_fen: Option<String>,
    account_kind: &'static str,
    can_close: bool,
    can_delete: bool,
    chain_status: &'static str,
    chain_tx_hash: Option<String>,
    chain_block_number: Option<u32>,
    creator_account_id: Option<String>,
    created_at: Option<String>,
}

/// 详情接口与账户列表使用同一 DTO 和真实链读，链上缺少的账户不伪造激活状态。
pub(crate) async fn detail_accounts(code: &str, cid: &str) -> Result<Vec<ChainAccountRow>, String> {
    let code =
        primitives::cid::code::institution_code_from_str(code).ok_or("invalid institution code")?;
    let accounts = institution_accounts_lookup(&code, cid).await?;
    Ok(chain_account_rows(&accounts).await)
}

/// 列表和所有机构详情共用一次批量余额查询；不把 RPC 失败伪装成零余额。
async fn chain_account_rows(accounts: &[OnChainInstitutionAccount]) -> Vec<ChainAccountRow> {
    let mut rows = accounts.iter().map(chain_account_row).collect::<Vec<_>>();
    let account_ids = rows
        .iter()
        .filter_map(|row| row.account_id.clone())
        .collect::<Vec<_>>();
    match crate::chain_runtime::fetch_account_balances_onchain(&account_ids).await {
        Ok(balances) => attach_account_balances(&mut rows, &balances),
        Err(error) => {
            tracing::warn!(%error, "institution account display balances unavailable");
        }
    }
    rows
}

fn attach_account_balances(
    rows: &mut [ChainAccountRow],
    balances: &std::collections::BTreeMap<String, Option<String>>,
) {
    for row in rows {
        row.balance_fen = row
            .account_id
            .as_ref()
            .and_then(|account| balances.get(account))
            .cloned()
            .flatten();
    }
}

/// 把链上账户投影成规范 `account_id`。
pub(crate) fn chain_account_row(account_id: &OnChainInstitutionAccount) -> ChainAccountRow {
    let cid_number = String::from_utf8_lossy(&account_id.cid_number).into_owned();
    let account_name = String::from_utf8_lossy(&account_id.account_name).into_owned();
    // 只有自定义命名账户可关闭;协议账户 can_close/can_delete 恒为 false。
    let account_kind =
        institution_account_kind_label(&cid_number, &account_name).unwrap_or("named");
    let closable = account_kind == "named";
    ChainAccountRow {
        account_id: Some(format!("0x{}", hex::encode(account_id.account_id))),
        balance_fen: None,
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

#[cfg(test)]
#[allow(clippy::expect_used)]
#[path = "../tests/institutions_accounts_handler_detail_tests.rs"]
mod detail_tests;

use primitives::account_derive::{self, RESERVED_ACCOUNT_NAMES as RESERVED_ACCOUNT_NAME_BYTES};
use primitives::core_const::SS58_FORMAT;

/// 全部 6 个受限保留账户名(单一源 = 链端 `account_derive`,UTF-8 字符串视图)。
///
/// 自定义账户判定:account_name 命中其一即非自定义(走各自 op_tag),否则为
/// `OP_NAME` 自定义命名账户。CitizenApp BFF 据此过滤 custom_account_names。
pub(crate) fn reserved_account_names() -> [String; 6] {
    [
        String::from_utf8_lossy(RESERVED_ACCOUNT_NAME_BYTES[0]).into_owned(),
        String::from_utf8_lossy(RESERVED_ACCOUNT_NAME_BYTES[1]).into_owned(),
        String::from_utf8_lossy(RESERVED_ACCOUNT_NAME_BYTES[2]).into_owned(),
        String::from_utf8_lossy(RESERVED_ACCOUNT_NAME_BYTES[3]).into_owned(),
        String::from_utf8_lossy(RESERVED_ACCOUNT_NAME_BYTES[4]).into_owned(),
        String::from_utf8_lossy(RESERVED_ACCOUNT_NAME_BYTES[5]).into_owned(),
    ]
}

/// 按 `account_name` 路由并派生机构账户地址的 32 字节 `AccountId`(唯一真源同上)。
///
/// 返回 `None` 当 `account_name` 为空串(与链端 `EmptyAccountName` 对齐)。
/// 关闭账户提案需要账户与主账户的裸 32 字节 AccountId(SCALE 无长度前缀),用本函数取。
pub(crate) fn derive_account_bytes(cid_number: &str, account_name: &str) -> Option<[u8; 32]> {
    account_derive::institution_kind_by_name(cid_number.as_bytes(), account_name.as_bytes())
        .map(|kind| kind.derive(SS58_FORMAT))
}
