//! 本地安全动作调度；管理员名册增删统一走现有链上登记，旧挑战也不能本地落库。
use onchina::authentication::login::AdminAuthContext;
// 原安全动作业务分支归所属模块，共用哈希和挑战协议仍在平台。
use crate::institutions::registration::precheck_institution_create_scope;
use onchina::authentication::actions::{
    apply_node_binding_unbind_conn, precheck_institution_target_scope_conn,
    preview_node_binding_unbind_conn, preview_security_action, ActionPreview,
};
use onchina::authentication::operation_auth::{parse_action_type, AdminActionType};
use onchina::authentication::security_model::AdminActionChallenge;
pub(crate) fn preview_action_conn(
    conn: &mut impl postgres::GenericClient,
    ctx: &AdminAuthContext,
    action_type: &AdminActionType,
    payload: &serde_json::Value,
) -> Result<ActionPreview, String> {
    require_chain_roster_action(action_type)?;
    match action_type {
        AdminActionType::NodeBindingUnbind => preview_node_binding_unbind_conn(conn, action_type),
        AdminActionType::InstitutionCreate => {
            precheck_institution_create_scope(ctx, payload)?;
            preview_security_action(action_type, payload)
        }
        AdminActionType::InstitutionCreateAccount
        | AdminActionType::InstitutionDeleteAccount
        | AdminActionType::InstitutionDeleteDocument => {
            precheck_institution_target_scope_conn(conn, ctx, payload)?;
            preview_security_action(action_type, payload)
        }
        _ => preview_security_action(action_type, payload),
    }
}
pub(crate) fn apply_action_conn(
    conn: &mut impl postgres::GenericClient,
    _ctx: &AdminAuthContext,
    challenge: &AdminActionChallenge,
) -> Result<serde_json::Value, String> {
    let action_type = parse_action_type(challenge.action_type.as_str())
        .map_err(|_| "http:bad_request:unknown action_type".to_string())?;
    require_chain_roster_action(&action_type)?;
    match action_type {
        AdminActionType::NodeBindingUnbind => apply_node_binding_unbind_conn(conn),
        _ => Err(
            "http:bad_request:business action cannot be applied by admin governance endpoint"
                .to_string(),
        ),
    }
}

/// 两个旧本地动作都拒绝；不能用本地目录存在与否替代链上名册。
pub(crate) fn require_chain_roster_action(action: &AdminActionType) -> Result<(), String> {
    match action {
        AdminActionType::CreateCityRegistry | AdminActionType::DeleteCityRegistry => {
            Err("http:conflict:管理员增删必须使用机构详情页的链上登记签名".into())
        }
        _ => Ok(()),
    }
}
