//! 股权公司私权机构。
//!
//! 股权公司固定为 `S + GQ`,具有法人资格,由股东和出资关系构成。

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
use crate::domains::private::participants::{ParticipantRole, COMPANY_ROLES};
use crate::institution::subjects::registration::{self, ListInstitutionQuery};
use crate::institution::subjects::CreateInstitutionInput;
use crate::AppState;

#[derive(Debug, Clone, Serialize)]
pub(crate) struct CompanyProfile {
    pub(crate) identity_code: &'static str,
    pub(crate) has_legal_personality: bool,
    pub(crate) shareholder_role: ParticipantRole,
}

pub(crate) const PROFILE: CompanyProfile = CompanyProfile {
    identity_code: "SFGQ",
    has_legal_personality: true,
    shareholder_role: ParticipantRole::EquityShareholder,
};

pub(crate) const SPEC: PrivateModuleSpec = PrivateModuleSpec {
    route_segment: "company",
    private_type: PrivateType::Company,
    title: "股权公司",
    description: "有限责任/股权有限公司,管理股东类型和出资关系。",
    allowed_participant_roles: COMPANY_ROLES,
};

fn lock_input(input: &mut CreateInstitutionInput) -> Result<(), &'static str> {
    assert_module_spec(&SPEC);
    debug_assert_eq!(PROFILE.identity_code, "SFGQ");
    const { assert!(PROFILE.has_legal_personality) };
    debug_assert_eq!(PROFILE.shareholder_role, ParticipantRole::EquityShareholder);
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
