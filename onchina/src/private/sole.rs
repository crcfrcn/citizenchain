//! 个体经营私权机构。
//!
//! 个体经营固定为 `F + GT`,无法人资格,负责人完全负责。

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
use crate::domains::private::participants::{ParticipantRole, SOLE_ROLES};
use crate::institution::subjects::registration::{self, ListInstitutionQuery};
use crate::institution::subjects::CreateInstitutionInput;
use crate::AppState;

/// 个体经营特有资料边界。
#[derive(Debug, Clone, Serialize)]
pub(crate) struct SoleProfile {
    pub(crate) responsible_role: ParticipantRole,
    pub(crate) has_legal_personality: bool,
    pub(crate) liability_description: &'static str,
}

pub(crate) const PROFILE: SoleProfile = SoleProfile {
    responsible_role: ParticipantRole::ResponsiblePerson,
    has_legal_personality: false,
    liability_description: "经营者完全负责",
};

pub(crate) const SPEC: PrivateModuleSpec = PrivateModuleSpec {
    route_segment: "sole",
    private_type: PrivateType::Sole,
    title: "个体经营",
    description: "个人经营的商户,无法人资格,由负责人完全负责。",
    allowed_participant_roles: SOLE_ROLES,
};

fn lock_input(input: &mut CreateInstitutionInput) -> Result<(), &'static str> {
    assert_module_spec(&SPEC);
    debug_assert_eq!(PROFILE.identity_code(), "SFGT");
    debug_assert_eq!(PROFILE.responsible_role, ParticipantRole::ResponsiblePerson);
    const { assert!(!PROFILE.has_legal_personality) };
    debug_assert!(!PROFILE.liability_description.is_empty());
    let rule = fixed_rule(SPEC.private_type)?;
    lock_input_to_rule(input, rule);
    Ok(())
}

impl SoleProfile {
    fn identity_code(&self) -> &'static str {
        "SFGT"
    }
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
