//! 合伙企业私权机构。
//!
//! 无限合伙固定为 `F + GP`,有限合伙固定为 `S + LP`;两者都由本模块统一管理合伙人关系。

use axum::{
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::Response,
    Json,
};
use serde::Serialize;

use crate::domains::private::common::{
    assert_module_spec, lock_input_to_rule, resolve_private_type_rule, PartnershipKind,
    PrivateModuleSpec, PrivateType,
};
use crate::domains::private::participants::{ParticipantRole, PARTNERSHIP_ROLES};
use crate::institution::subjects::registration::{self, ListInstitutionQuery};
use crate::institution::subjects::CreateInstitutionInput;
use crate::AppState;

/// 合伙企业特有资料边界。
#[derive(Debug, Clone, Serialize)]
pub(crate) struct PartnershipProfile {
    pub(crate) kind: PartnershipKind,
    pub(crate) required_roles: &'static [ParticipantRole],
    pub(crate) identity_code: &'static str,
}

pub(crate) const GENERAL_PROFILE: PartnershipProfile = PartnershipProfile {
    kind: PartnershipKind::General,
    required_roles: &[ParticipantRole::GeneralPartner],
    identity_code: "SFGP",
};

pub(crate) const LIMITED_PROFILE: PartnershipProfile = PartnershipProfile {
    kind: PartnershipKind::Limited,
    required_roles: &[
        ParticipantRole::GeneralPartner,
        ParticipantRole::LimitedPartner,
    ],
    identity_code: "SFLP",
};

pub(crate) const SPEC: PrivateModuleSpec = PrivateModuleSpec {
    route_segment: "partnership",
    private_type: PrivateType::Partnership,
    title: "合伙企业",
    description: "合伙企业分无限合伙和有限合伙,由合伙人关系共同构成。",
    allowed_participant_roles: PARTNERSHIP_ROLES,
};

fn lock_input(input: &mut CreateInstitutionInput) -> Result<(), &'static str> {
    assert_module_spec(&SPEC);
    let partnership_kind = input
        .partnership_kind
        .as_deref()
        .ok_or("合伙企业必须选择 GENERAL 或 LIMITED")?;
    let rule = resolve_private_type_rule(SPEC.private_type.as_code(), Some(partnership_kind))?;
    let profile = match rule.partnership_kind {
        Some(PartnershipKind::General) => &GENERAL_PROFILE,
        Some(PartnershipKind::Limited) => &LIMITED_PROFILE,
        None => return Err("合伙企业必须选择 GENERAL 或 LIMITED"),
    };
    debug_assert_eq!(profile.identity_code, rule.institution_code);
    debug_assert!(!profile.required_roles.is_empty());
    lock_input_to_rule(input, rule);
    Ok(())
}

pub(crate) async fn create(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(mut input): Json<CreateInstitutionInput>,
) -> Response {
    if let Err(msg) = lock_input(&mut input) {
        return crate::api_error(StatusCode::BAD_REQUEST, 1001, msg);
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
