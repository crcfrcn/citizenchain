//! 股份公司私权机构。
//!
//! 股份公司固定为 `S + GF`,具有法人资格,由股份、发起人和股东关系构成。

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
use crate::domains::private::participants::{ParticipantRole, CORPORATION_ROLES};
use crate::institution::subjects::registration::{self, ListInstitutionQuery};
use crate::institution::subjects::CreateInstitutionInput;
use crate::AppState;

#[derive(Debug, Clone, Serialize)]
pub(crate) struct CorporationProfile {
    pub(crate) identity_code: &'static str,
    pub(crate) has_legal_personality: bool,
    pub(crate) equity_unit_label: &'static str,
    pub(crate) shareholder_role: ParticipantRole,
}

pub(crate) const PROFILE: CorporationProfile = CorporationProfile {
    identity_code: "SFGF",
    has_legal_personality: true,
    equity_unit_label: "股份",
    shareholder_role: ParticipantRole::Shareholder,
};

pub(crate) const SPEC: PrivateModuleSpec = PrivateModuleSpec {
    route_segment: "corporation",
    private_type: PrivateType::Corporation,
    title: "股份公司",
    description: "股份有限公司,管理股份类别、发起人和股东关系。",
    allowed_participant_roles: CORPORATION_ROLES,
};

fn lock_input(input: &mut CreateInstitutionInput) -> Result<(), &'static str> {
    assert_module_spec(&SPEC);
    debug_assert_eq!(PROFILE.identity_code, "SFGF");
    debug_assert_eq!(PROFILE.equity_unit_label, "股份");
    debug_assert_eq!(PROFILE.shareholder_role, ParticipantRole::Shareholder);
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
