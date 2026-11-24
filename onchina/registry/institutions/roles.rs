//! 注册局正常岗位登记；业务权限、当前人员任职和辖区均在同一最终块复核。
use crate::*;
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use codec::DecodeAll;
use onchina::{
    authentication::login::AdminAuthContext,
    institutions::{
        build_chain_sign_output, encode_register_institution_roles, fetch_registration_roles_at,
        fetch_registration_snapshot, parse_role_action, select_registration_role,
        validate_action_people, InstitutionAssignmentChangeInput, InstitutionChainSignRequest,
        InstitutionRoleMutationInput, RegisterInstitutionRolesArgs,
    },
};
use serde::{Deserialize, Serialize};
pub(crate) const PURPOSE_INSTITUTION_REGISTER_ROLES: &str = "INSTITUTION_REGISTER_ROLES";
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PrepareRegisterInstitutionRolesInput {
    pub cid_number: String,
    pub actor_role_code: String,
    pub expected_state_hash: String,
    pub initial_threshold: Option<u32>,
    #[serde(default)]
    pub role_mutations: Vec<InstitutionRoleMutationInput>,
    #[serde(default)]
    pub assignment_changes: Vec<InstitutionAssignmentChangeInput>,
}
fn input_error(error: String) -> axum::response::Response {
    api_error(StatusCode::BAD_REQUEST, 1001, &error)
}
fn read_error(error: String) -> axum::response::Response {
    tracing::warn!(%error,"read registrar roles failed");
    api_error(StatusCode::BAD_GATEWAY, 5002, "岗位链状态读取失败")
}
pub(crate) async fn prepare_register_institution_roles(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<PrepareRegisterInstitutionRolesInput>,
) -> axum::response::Response {
    let result=async {
        let ctx=require_admin_any(&state,&headers)?;authorization::rules::ensure_registry_admin(&ctx)?;
        if input.actor_role_code!=ctx.role_code {return Err(api_error(StatusCode::FORBIDDEN,1003,"办理岗位与所选会话不一致"));}
        let snapshot=fetch_registration_snapshot(&input.cid_number).await.map_err(read_error)?;
        let hash=hex::decode(input.expected_state_hash.strip_prefix("0x").ok_or_else(||input_error("登记状态哈希无效".into()))?).map_err(|_|input_error("登记状态哈希无效".into()))?;
        if hash.as_slice()!=snapshot.state_hash() {return Err(api_error(StatusCode::CONFLICT,1007,"岗位登记状态已变化，请重新审阅"));}
        let actor=chain_runtime::identity_from_binding_parts(&ctx.institution_code,Some(&ctx.institution_cid_number)).map_err(read_error)?;
        let roles=fetch_registration_roles_at(&actor,&ctx.account_id,&input.cid_number,snapshot.block_hash).await.map_err(read_error)?;
        select_registration_role(&roles,Some(&ctx.role_code)).map_err(|e|api_error(StatusCode::FORBIDDEN,1003,e))?;
        let (role_mutations,assignment_changes)=parse_role_action(&snapshot,&input.role_mutations,&input.assignment_changes,true).map_err(input_error)?;
        if role_mutations.is_empty()&&assignment_changes.is_empty(){return Err(input_error("岗位登记内容不能为空".into()));}
        validate_action_people(&snapshot,&role_mutations,&assignment_changes).await.map_err(input_error)?;
        let call=encode_register_institution_roles(&RegisterInstitutionRolesArgs {is_private:snapshot.module_tag()==b"pri-mgmt",cid_number:input.cid_number.as_bytes().to_vec(),action:entitybase::InstitutionRoleRegistration{role_mutations,assignment_changes},institution_code:snapshot.institution.institution_code,actor_cid_number:ctx.institution_cid_number.as_bytes().to_vec(),actor_role_code:ctx.role_code.as_bytes().to_vec(),expected_state_hash:snapshot.state_hash(),initial_threshold:input.initial_threshold});
        let proof=authentication::passkey::require_passkey_assertion(&state,&headers,&ctx.account_id)?;
        build_chain_sign_output(&state,InstitutionChainSignRequest{account_id:&ctx.account_id,cid_number:&input.cid_number,purpose:PURPOSE_INSTITUTION_REGISTER_ROLES,call_data:call.call_data,chain_action:call.action,context:serde_json::json!({"cid_number":input.cid_number,"actor_cid_number":ctx.institution_cid_number,"actor_role_code":ctx.role_code,"op":"register_roles"})},&proof).await
    }.await;
    match result {
        Ok(data) => Json(ApiResponse {
            code: 0,
            message: "ok".into(),
            data,
        })
        .into_response(),
        Err(error) => error,
    }
}
pub(crate) async fn recheck_registration(
    _state: &AppState,
    ctx: &AdminAuthContext,
    session: &transactions::session::ChainSignSession,
) -> Result<(), axum::response::Response> {
    if !matches!(session.call_data.first(), Some(30) | Some(31))
        || session.call_data.get(1) != Some(&9)
    {
        return Err(input_error("会话用途与岗位登记调用不一致".into()));
    }
    let (target, action, actor, role, expected, _): (
        Vec<u8>,
        entitybase::InstitutionRoleRegistration,
        Vec<u8>,
        Vec<u8>,
        [u8; 32],
        Option<u32>,
    ) = DecodeAll::decode_all(&mut &session.call_data[2..])
        .map_err(|_| input_error("岗位登记调用损坏".into()))?;
    if actor != ctx.institution_cid_number.as_bytes() || role != ctx.role_code.as_bytes() {
        return Err(api_error(
            StatusCode::FORBIDDEN,
            1003,
            "原办理机构或岗位已变化",
        ));
    }
    let cid = session.context["cid_number"]
        .as_str()
        .ok_or_else(|| input_error("岗位登记上下文缺失".into()))?;
    if target != cid.as_bytes() {
        return Err(input_error("原调用目标与登记上下文不一致".into()));
    }
    let snapshot = fetch_registration_snapshot(cid).await.map_err(read_error)?;
    if session.call_data[0]
        != (if snapshot.module_tag() == b"pri-mgmt" {
            31
        } else {
            30
        })
    {
        return Err(input_error("原调用命名空间不一致".into()));
    }
    if snapshot.state_hash() != expected {
        return Err(api_error(StatusCode::CONFLICT, 1007, "岗位登记状态已变化"));
    }
    let identity = chain_runtime::identity_from_binding_parts(
        &ctx.institution_code,
        Some(&ctx.institution_cid_number),
    )
    .map_err(read_error)?;
    let choices = fetch_registration_roles_at(&identity, &ctx.account_id, cid, snapshot.block_hash)
        .await
        .map_err(read_error)?;
    select_registration_role(&choices, Some(&ctx.role_code))
        .map_err(|e| api_error(StatusCode::FORBIDDEN, 1003, e))?;
    validate_action_people(
        &snapshot,
        &action.role_mutations,
        &action.assignment_changes,
    )
    .await
    .map_err(input_error)
}
