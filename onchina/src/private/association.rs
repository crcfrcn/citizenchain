//! 注册协会私权机构。
//!
//! 注册协会固定为 `SFAS`，具有法人资格，由会员、理事和监事构成；盈利属性由每个
//! 机构实例在首次登记时显式选择，不在模块常量中固定。

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
use crate::domains::private::participants::{ParticipantRole, ASSOCIATION_ROLES};
use crate::institution::subjects::registration::{self, ListInstitutionQuery};
use crate::institution::subjects::CreateInstitutionInput;
use crate::AppState;

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AssociationProfile {
    pub(crate) identity_code: &'static str,
    pub(crate) has_legal_personality: bool,
    pub(crate) member_role: ParticipantRole,
}

pub(crate) const PROFILE: AssociationProfile = AssociationProfile {
    identity_code: "SFAS",
    has_legal_personality: true,
    member_role: ParticipantRole::Member,
};

pub(crate) const SPEC: PrivateModuleSpec = PrivateModuleSpec {
    route_segment: "association",
    private_type: PrivateType::Association,
    title: "注册协会",
    description: "具有法人资格的协会类组织,管理会员、理事和协会宗旨。",
    allowed_participant_roles: ASSOCIATION_ROLES,
};

fn lock_input(input: &mut CreateInstitutionInput) -> Result<(), &'static str> {
    assert_module_spec(&SPEC);
    debug_assert_eq!(PROFILE.identity_code, "SFAS");
    const { assert!(PROFILE.has_legal_personality) };
    debug_assert_eq!(PROFILE.member_role, ParticipantRole::Member);
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
