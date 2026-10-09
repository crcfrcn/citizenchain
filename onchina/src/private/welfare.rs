//! 公益组织私权机构。
//!
//! 公益组织固定为 `S + GY`,具有法人资格且非营利,由成员、理事和监事构成。

use axum::{
    extract::{Query, State},
    http::HeaderMap,
    response::Response,
    Json,
};
use serde::Serialize;

use crate::domains::private::common::{
    assert_module_spec, fixed_rule, lock_input_to_rule, PrivateModuleSpec, PrivateType,
};
use crate::domains::private::participants::WELFARE_ROLES;
use crate::institution::subjects::registration::{self, ListInstitutionQuery};
use crate::institution::subjects::CreateInstitutionInput;
use crate::AppState;

#[derive(Debug, Clone, Serialize)]
pub(crate) struct WelfareProfile {
    pub(crate) identity_code: &'static str,
    pub(crate) p1: &'static str,
    pub(crate) has_legal_personality: bool,
    pub(crate) purpose_label: &'static str,
}

pub(crate) const PROFILE: WelfareProfile = WelfareProfile {
    identity_code: "SFGY",
    p1: "0",
    has_legal_personality: true,
    purpose_label: "公益目的",
};

pub(crate) const SPEC: PrivateModuleSpec = PrivateModuleSpec {
    route_segment: "welfare",
    private_type: PrivateType::Welfare,
    title: "公益组织",
    description: "具有法人资格的非营利组织,管理公益属性和成员关系。",
    allowed_participant_roles: WELFARE_ROLES,
};

fn lock_input(input: &mut CreateInstitutionInput) -> Result<(), &'static str> {
    assert_module_spec(&SPEC);
    debug_assert_eq!(PROFILE.identity_code, "SFGY");
    debug_assert_eq!(PROFILE.p1, "0");
    const { assert!(PROFILE.has_legal_personality) };
    debug_assert_eq!(PROFILE.purpose_label, "公益目的");
    let rule = fixed_rule(SPEC.private_type)?;
    lock_input_to_rule(input, rule);
    Ok(())
}

pub(crate) async fn create(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(mut input): Json<CreateInstitutionInput>,
) -> Response {
    if let Err(msg) = lock_input(&mut input) {
        return crate::api_error(axum::http::StatusCode::BAD_REQUEST, 1001, msg);
    }
    registration::create_private_institution(state, headers, input).await
}

pub(crate) async fn list(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(mut query): Query<ListInstitutionQuery>,
) -> Response {
    query.category = Some("PRIVATE_INSTITUTION".to_string());
    query.private_type = Some(SPEC.private_type.as_code().to_string());
    registration::list_private_institutions(state, headers, query).await
}
