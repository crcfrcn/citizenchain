//! 立法与表决 HTTP handler(`/api/legislation/*`)。
//!
//! 发起/表决产出扫码上链 `sign_request`；CitizenWallet 只签名一次并显示响应二维码，OnChina 回扫响应后通过统一入口提交；
//! 读法律/提案进度直读链。后端强制:① 登录绑定机构(只有该院管理员可达)② 本机构能否发起该
//! 类型提案(`category::proposable_candidates`)③ 原机构 CID 地域与目标法律范围校验。
//! 能力位是前端渲染门控,后端以此三重边界为准。

use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use serde::Deserialize;

use super::category::{legislation_role, proposable_candidates, LegislationRole};
use super::chain_read_proposal;
use super::law::model::{institution_code_text, LawActionInput, LawView, ProposeLawInput};
use super::law::{action, chain_read, service};
use super::model::ProposalCategory;
use crate::authentication::login::{require_admin_any, AdminAuthContext};
use crate::cid::china::{city_code_by_name, province_code_by_name};
use crate::core::response::ApiResponse;
use crate::{api_error, AppState};
use primitives::cid::code::institution_code_from_str;

/// 法律列表查询参数；地域由已验证会话派生。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LawListQuery {
    pub tier: u8,
}

/// 代表机构表决请求体。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CastRepresentativeVoteInput {
    pub proposal_id: u64,
    pub voter_role_code: String,
    pub approve: bool,
}

/// 管理员层级 → 立法层级(1 国家 / 2 省 / 3 市);非立法层级返回 None。
fn admin_tier(ctx: &AdminAuthContext) -> Option<u8> {
    match ctx.admin_level.as_deref() {
        Some("NATIONAL") => Some(1),
        Some("PROVINCE") => Some(2),
        Some("CITY") => Some(3),
        _ => None,
    }
}

/// 已验证会话的省、市名称按现行字典还原原码；法律侧继续按机构 CID 匹配。
fn law_scope(ctx: &AdminAuthContext, tier: u8) -> Result<chain_read::LawScope, String> {
    let own = admin_tier(ctx).ok_or("admin level missing")?;
    if !(own == tier || (own == 1 && tier == 0)) {
        return Err("law tier outside admin scope".into());
    }
    let province_code = if tier >= 2 {
        ctx.scope_province_name
            .as_deref()
            .and_then(province_code_by_name)
            .ok_or("admin province scope missing")?
            .to_string()
    } else {
        String::new()
    };
    let city_code = if tier == 3 {
        ctx.scope_province_name
            .as_deref()
            .zip(ctx.scope_city_name.as_deref())
            .and_then(|(province, city)| city_code_by_name(province, city))
            .ok_or("admin city scope missing")?
            .to_string()
    } else {
        String::new()
    };
    Ok(chain_read::LawScope {
        tier,
        province_code,
        city_code,
    })
}

/// 新法地域由原机构 CID 派生并核对会话；修法、废法继续核对目标法律。
fn proposal_scope(
    ctx: &AdminAuthContext,
    input: &ProposeLawInput,
    target_law: Option<&chain_read::OnChainLaw>,
) -> Result<chain_read::LawScope, String> {
    let scope = law_scope(ctx, input.tier)?;
    match input.law_action {
        LawActionInput::Enact => {
            let region = primitives::genesis::LawRegion::from_cid(
                input.tier,
                ctx.institution_cid_number.as_bytes(),
            )?;
            let expected = primitives::genesis::LawRegion::from_codes(
                input.tier,
                scope.province_code.as_bytes(),
                scope.city_code.as_bytes(),
            )?;
            if region != expected {
                return Err("law outside admin scope".into());
            }
        }
        LawActionInput::Amend | LawActionInput::Repeal => {
            let law_id = input
                .law_id
                .ok_or(service::LegislationError::MissingLawId.code())?;
            target_law
                .filter(|law| law.law_id == law_id && chain_read::law_matches_scope(law, &scope))
                .ok_or("law outside admin scope")?;
        }
    }
    Ok(scope)
}

/// GET /api/legislation/laws?tier= —— 本级已生效/在册法律列表。
pub(crate) async fn list_laws(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<LawListQuery>,
) -> impl IntoResponse {
    let ctx = match require_admin_any(&state, &headers) {
        Ok(ctx) => ctx,
        Err(response) => return response,
    };
    let scope = match law_scope(&ctx, query.tier) {
        Ok(scope) => scope,
        Err(error) => return api_error(StatusCode::FORBIDDEN, 1003, &error),
    };
    let laws = match chain_read::list_laws_by_scope(&scope).await {
        Ok(v) => v,
        Err(err) => return api_error(StatusCode::BAD_GATEWAY, 5002, err.as_str()),
    };
    Json(ApiResponse {
        code: 0,
        message: "ok".to_string(),
        data: build_law_views(laws).await,
    })
    .into_response()
}

/// 一条可发起候选(前端发起菜单渲染用)。
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ProposableCandidateDto {
    category: ProposalCategory,
    tier: u8,
    vote_types: Vec<u8>,
}

/// GET /api/legislation/proposable —— 本机构可发起的提案候选(category×tier×voteTypes)。
///
/// 发起菜单单源自后端 `category::proposable_candidates`(参议会/非立法机构返回空);
/// 前端据此渲染可选立法动作与表决类型,不复刻分类逻辑。
pub(crate) async fn list_proposable(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let ctx = match require_admin_any(&state, &headers) {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let candidates: Vec<ProposableCandidateDto> = proposable_candidates(&ctx.institution_code)
        .into_iter()
        .map(|c| ProposableCandidateDto {
            category: c.category,
            tier: c.tier,
            vote_types: c.vote_types,
        })
        .collect();
    Json(ApiResponse {
        code: 0,
        message: "ok".to_string(),
        data: candidates,
    })
    .into_response()
}

/// GET /api/legislation/laws/mine —— 本节点绑定机构层级/辖区的全部法律(会话派生 scope,前端不传码)。
///
/// 国家级并入宪法与国家法律；省/市用原省代码和省内市编号匹配机构 CID。
/// 范围始终取会话与机构 CID，不接受前端地域占位值。
pub(crate) async fn list_my_laws(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let ctx = match require_admin_any(&state, &headers) {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let Some(admin_tier) = admin_tier(&ctx) else {
        return api_error(StatusCode::FORBIDDEN, 1003, "admin level missing");
    };
    let tiers: &[u8] = match admin_tier {
        1 => &[0, 1], // 宪法 + 国家法律
        2 => &[2],
        3 => &[3],
        _ => &[],
    };
    let mut laws: Vec<chain_read::OnChainLaw> = Vec::new();
    for &tier in tiers {
        let scope = match law_scope(&ctx, tier) {
            Ok(scope) => scope,
            Err(error) => return api_error(StatusCode::FORBIDDEN, 1003, &error),
        };
        match chain_read::list_laws_by_scope(&scope).await {
            Ok(v) => laws.extend(v),
            Err(err) => return api_error(StatusCode::BAD_GATEWAY, 5002, err.as_str()),
        }
    }
    Json(ApiResponse {
        code: 0,
        message: "ok".to_string(),
        data: build_law_views(laws).await,
    })
    .into_response()
}

/// 逐部法律取办理端展示版本 → `LawView` 列表(`list_laws` / `list_my_laws` 共用)。
async fn build_law_views(laws: Vec<chain_read::OnChainLaw>) -> Vec<LawView> {
    let mut views = Vec::with_capacity(laws.len());
    let immutable_article_numbers = if laws.iter().any(|law| law.tier == 0) {
        chain_read::fetch_immutable_article_numbers()
            .await
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    for law in laws {
        let Some(version_id) = chain_read::operator_display_version(&law) else {
            continue;
        };
        if let Ok(Some(version)) = chain_read::fetch_law_version(law.law_id, version_id).await {
            let version_label = chain_read::fetch_law_version_label(law.law_id, version_id)
                .await
                .unwrap_or_default();
            views.push(chain_read::build_law_view(
                &law,
                &version,
                version_label.as_ref(),
                &immutable_article_numbers,
            ));
        }
    }
    views
}

/// GET /api/legislation/laws/{law_id} —— 单部法律办理端展示版本全文。
pub(crate) async fn law(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(law_id): Path<u64>,
) -> impl IntoResponse {
    if let Err(resp) = require_admin_any(&state, &headers) {
        return resp;
    }
    let law = match chain_read::fetch_law(law_id).await {
        Ok(Some(v)) => v,
        Ok(None) => return api_error(StatusCode::NOT_FOUND, 1004, "law not found"),
        Err(err) => return api_error(StatusCode::BAD_GATEWAY, 5002, err.as_str()),
    };
    let Some(version_id) = chain_read::operator_display_version(&law) else {
        return api_error(StatusCode::NOT_FOUND, 1004, "law version not found");
    };
    let version = match chain_read::fetch_law_version(law.law_id, version_id).await {
        Ok(Some(v)) => v,
        Ok(None) => return api_error(StatusCode::NOT_FOUND, 1004, "law version not found"),
        Err(err) => return api_error(StatusCode::BAD_GATEWAY, 5002, err.as_str()),
    };
    let version_label = match chain_read::fetch_law_version_label(law.law_id, version_id).await {
        Ok(v) => v,
        Err(err) => return api_error(StatusCode::BAD_GATEWAY, 5002, err.as_str()),
    };
    let immutable_article_numbers = if law.tier == 0 {
        match chain_read::fetch_immutable_article_numbers().await {
            Ok(v) => v,
            Err(err) => return api_error(StatusCode::BAD_GATEWAY, 5002, err.as_str()),
        }
    } else {
        Vec::new()
    };
    Json(ApiResponse {
        code: 0,
        message: "ok".to_string(),
        data: chain_read::build_law_view(
            &law,
            &version,
            version_label.as_ref(),
            &immutable_article_numbers,
        ),
    })
    .into_response()
}

/// GET /api/legislation/proposals/{proposal_id} —— 提案进度只读投影。
pub(crate) async fn get_proposal_state(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(proposal_id): Path<u64>,
) -> impl IntoResponse {
    if let Err(resp) = require_admin_any(&state, &headers) {
        return resp;
    }
    match chain_read_proposal::fetch_proposal_state(proposal_id).await {
        Ok(Some(state)) => Json(ApiResponse {
            code: 0,
            message: "ok".to_string(),
            data: state,
        })
        .into_response(),
        Ok(None) => api_error(StatusCode::NOT_FOUND, 1004, "proposal not found"),
        Err(err) => api_error(StatusCode::BAD_GATEWAY, 5002, err.as_str()),
    }
}

/// POST /api/legislation/propose —— 发起法律案，返回统一链签名准备结果。
pub(crate) async fn propose_legislation(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<ProposeLawInput>,
) -> impl IntoResponse {
    let ctx = match require_admin_any(&state, &headers) {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let Some(proposer_code) = institution_code_from_str(&ctx.institution_code) else {
        return api_error(StatusCode::FORBIDDEN, 1003, "unknown institution code");
    };
    if admin_tier(&ctx).is_none() {
        return api_error(StatusCode::FORBIDDEN, 1003, "admin level missing");
    }
    // ② 本机构能否发起该表决类型；层级及目标法律地域由 proposal_scope 校验，
    //    故此处只判 vote_type 成员,放行国家级修宪(tier 0)。
    let can_propose = proposable_candidates(&ctx.institution_code)
        .iter()
        .any(|c| c.vote_types.contains(&input.vote_type));
    if !can_propose {
        return api_error(
            StatusCode::FORBIDDEN,
            1003,
            "institution cannot propose this legislation",
        );
    }
    let target_law = match input.law_action {
        LawActionInput::Enact => None,
        LawActionInput::Amend | LawActionInput::Repeal => {
            let Some(law_id) = input.law_id else {
                return api_error(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    2001,
                    service::LegislationError::MissingLawId.code(),
                );
            };
            match chain_read::fetch_law(law_id).await {
                Ok(law) => law,
                Err(error) => return api_error(StatusCode::BAD_GATEWAY, 5002, &error),
            }
        }
    };
    let scope = match proposal_scope(&ctx, &input, target_law.as_ref()) {
        Ok(scope) => scope,
        Err(error) => return api_error(StatusCode::FORBIDDEN, 1003, &error),
    };
    let chain_read::LawScope {
        province_code,
        city_code,
        ..
    } = scope;
    // 发起 CID 直接使用登录绑定；其他路由机构继续按原路由和省、市字符串唯一查询。
    let db_state = state.clone();
    let resolve_cid_number = move |code: &[u8; 4]| {
        chain_read::resolve_institution_cid_number(
            &db_state.db,
            &institution_code_text(code),
            &province_code,
            &city_code,
        )
    };
    // 链上写(PasskeyColdSign 档):会话 + passkey + 钱包冷签,三者缺一不可。
    // passkey 凭证向下传给冷签会话创建入口,由类型保证这一步不会被跳过。
    let passkey = match crate::authentication::passkey::require_passkey_assertion(
        &state,
        &headers,
        ctx.account_id.as_str(),
    ) {
        Ok(proof) => proof,
        Err(resp) => return resp,
    };
    match action::prepare_propose_law_sign(
        &state,
        &input,
        proposer_code,
        ctx.account_id.as_str(),
        ctx.institution_cid_number.as_str(),
        resolve_cid_number,
        &passkey,
    )
    .await
    {
        Ok(output) => Json(ApiResponse {
            code: 0,
            message: "ok".to_string(),
            data: output,
        })
        .into_response(),
        Err(resp) => resp,
    }
}

/// POST /api/legislation/representative-vote —— 当前代表机构表决。
pub(crate) async fn cast_representative_vote(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<CastRepresentativeVoteInput>,
) -> impl IntoResponse {
    let ctx = match require_admin_any(&state, &headers) {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    // 只有当前制度允许参加代表表决的机构可投；仅提案机构不可投。
    let can_vote = matches!(
        legislation_role(&ctx.institution_code),
        Some(LegislationRole::ProposerHouse | LegislationRole::ReviewHouse)
    );
    if !can_vote {
        return api_error(
            StatusCode::FORBIDDEN,
            1003,
            "institution cannot cast representative vote",
        );
    }
    // 链上写(PasskeyColdSign 档):会话 + passkey + 钱包冷签,三者缺一不可。
    let passkey = match crate::authentication::passkey::require_passkey_assertion(
        &state,
        &headers,
        ctx.account_id.as_str(),
    ) {
        Ok(proof) => proof,
        Err(resp) => return resp,
    };
    match action::prepare_representative_vote_sign(
        &state,
        input.proposal_id,
        input.voter_role_code.as_str(),
        input.approve,
        ctx.account_id.as_str(),
        ctx.institution_cid_number.as_str(),
        &passkey,
    )
    .await
    {
        Ok(output) => Json(ApiResponse {
            code: 0,
            message: "ok".to_string(),
            data: output,
        })
        .into_response(),
        Err(resp) => resp,
    }
}

#[cfg(test)]
// 夹具按既有 CID 规则构造，仅用于内存中的范围与调用回归。
#[allow(clippy::unwrap_used)]
mod scope_tests {
    use super::*;

    fn cid(institution: &str, province: &str, city: &str) -> String {
        primitives::cid::generator::generate_cid_number(
            primitives::cid::generator::GenerateCidNumberInput {
                public_key: "0x1234",
                p1: "0",
                province_code: province,
                province_name: "测试省",
                city_code: city,
                city_name: "测试市",
                year: "2026",
                institution,
            },
        )
        .unwrap()
    }

    fn context(level: &str, institution: &str, province: &str, city: &str) -> AdminAuthContext {
        AdminAuthContext {
            account_id: format!("0x{}", "11".repeat(32)),
            institution_cid_number: cid(institution, province, city),
            institution_code: institution.into(),
            admin_level: Some(level.into()),
            family_name: String::new(),
            given_name: String::new(),
            scope_province_name: Some("广东省".into()),
            scope_city_name: None,
            scope_town_name: None,
            cid_short_name: None,
        }
    }

    fn city_context() -> AdminAuthContext {
        let mut ctx = context("CITY", "CSLF", "GD", "001");
        ctx.scope_city_name = Some(
            crate::cid::china::provinces()
                .iter()
                .find(|province| province.province_code == "GD")
                .unwrap()
                .cities
                .iter()
                .find(|city| city.city_code == "001")
                .unwrap()
                .city_name
                .to_string(),
        );
        ctx
    }

    fn input(action: LawActionInput, tier: u8) -> ProposeLawInput {
        ProposeLawInput {
            law_action: action,
            proposer_role_code: "REPRESENTATIVE".into(),
            tier,
            vote_type: 2,
            title: "法律".into(),
            title_en: None,
            chapters: Vec::new(),
            effective_at: 1000,
            law_id: Some(7),
        }
    }

    fn law(tier: u8, institution: &str, province: &str, city: &str) -> chain_read::OnChainLaw {
        chain_read::OnChainLaw {
            law_id: 7,
            tier,
            region: primitives::genesis::LawRegion::from_codes(
                tier,
                province.as_bytes(),
                city.as_bytes(),
            )
            .unwrap(),
            houses: vec![cid(institution, province, city).into_bytes()],
            effective_version: Some(1),
            latest_version: 1,
            pending_version: None,
            status: 1,
        }
    }

    #[test]
    fn province_scope_keeps_verified_session_scope_and_original_code() {
        let ctx = context("PROVINCE", "PRP", "GD", "002");
        let scope = law_scope(&ctx, 2).unwrap();
        assert_eq!(scope.province_code, "GD");
        assert!(scope.city_code.is_empty());
        assert!(law_scope(&ctx, 3).is_err());
    }

    #[test]
    fn fixed_province_letters_are_preserved_without_numeric_conversion() {
        for province in primitives::cid::code::PROVINCE_CODE_INFOS {
            let code = primitives::cid::code::province_code_text(&province.province_code).unwrap();
            assert_eq!(code.len(), 2);
            assert!(code.bytes().all(|byte| byte.is_ascii_uppercase()));
            let mut ctx = context("PROVINCE", "PRP", code, "001");
            ctx.scope_province_name = Some(province.province_name.into());
            let mut request = input(LawActionInput::Enact, 2);
            request.law_id = None;
            let scope = proposal_scope(&ctx, &request, None).unwrap();
            let region =
                primitives::genesis::LawRegion::from_cid(2, ctx.institution_cid_number.as_bytes())
                    .unwrap();
            assert_eq!(region.province_code.as_slice(), code.as_bytes());
            assert_eq!(region.city_number, 0);
            assert_eq!(scope.province_code, code);
            assert!(scope.city_code.is_empty());
        }
    }

    #[test]
    fn new_law_rejects_cid_region_different_from_session() {
        let mut ctx = context("PROVINCE", "PRP", "GZ", "001");
        let request = input(LawActionInput::Enact, 2);
        assert!(proposal_scope(&ctx, &request, None).is_err());
        ctx.institution_cid_number = "invalid-cid".into();
        assert!(proposal_scope(&ctx, &request, None).is_err());
    }

    #[test]
    fn missing_session_scope_and_wrong_tier_are_rejected() {
        let mut ctx = context("PROVINCE", "PRP", "GD", "001");
        ctx.scope_province_name = None;
        assert!(law_scope(&ctx, 2).is_err());
        ctx.scope_province_name = Some("广东省".into());
        ctx.admin_level = Some("CITY".into());
        assert!(law_scope(&ctx, 3).is_err());
        assert!(law_scope(&ctx, 2).is_err());
    }

    #[test]
    fn amend_and_repeal_validate_target_id_and_original_cid_scope() {
        let ctx = context("PROVINCE", "PRP", "GD", "001");
        let target = law(2, "PRP", "GD", "002");
        for action in [LawActionInput::Amend, LawActionInput::Repeal] {
            let request = input(action, 2);
            let scope = proposal_scope(&ctx, &request, Some(&target)).unwrap();
            assert_eq!(scope.province_code, "GD");
            assert_eq!(request.law_id, Some(7));
        }
    }

    #[test]
    fn target_law_requires_same_id_tier_province_and_city() {
        let ctx = city_context();
        for action in [LawActionInput::Amend, LawActionInput::Repeal] {
            let request = input(action, 3);
            let same_city = law(3, "CLEG", "GD", "001");
            assert!(proposal_scope(&ctx, &request, Some(&same_city)).is_ok());
            let mut target = law(3, "CLEG", "GZ", "001");
            assert!(proposal_scope(&ctx, &request, Some(&target)).is_err());
            target = law(3, "CLEG", "GD", "002");
            assert!(proposal_scope(&ctx, &request, Some(&target)).is_err());
            target = law(2, "PRP", "GD", "001");
            assert!(proposal_scope(&ctx, &request, Some(&target)).is_err());
            target = law(3, "CLEG", "GD", "001");
            target.law_id = 8;
            assert!(proposal_scope(&ctx, &request, Some(&target)).is_err());
        }
    }

    #[test]
    fn amend_and_repeal_require_existing_target_and_id() {
        let ctx = context("PROVINCE", "PRP", "GD", "001");
        for action in [LawActionInput::Amend, LawActionInput::Repeal] {
            let mut request = input(action, 2);
            assert!(proposal_scope(&ctx, &request, None).is_err());
            request.law_id = None;
            assert_eq!(
                proposal_scope(&ctx, &request, None).unwrap_err(),
                service::LegislationError::MissingLawId.code()
            );
        }
    }

    #[test]
    fn new_law_derives_original_cid_region_and_keeps_cn() {
        let national = context("NATIONAL", "NRP", "ZS", "001");
        let mut request = input(LawActionInput::Enact, 1);
        request.law_id = None;
        let scope = proposal_scope(&national, &request, None).unwrap();
        assert_eq!(
            primitives::genesis::LawRegion::from_cid(1, national.institution_cid_number.as_bytes())
                .unwrap(),
            primitives::genesis::LawRegion::NATIONAL
        );
        assert!(scope.province_code.is_empty() && scope.city_code.is_empty());
        assert_eq!(primitives::cid::code::COUNTRY_CN, *b"CN");
        assert!(law_scope(&national, 0).is_ok());
        assert!(law_scope(&national, 2).is_err());
        let city = city_context();
        let mut request = input(LawActionInput::Enact, 3);
        request.law_id = None;
        let scope = proposal_scope(&city, &request, None).unwrap();
        assert_eq!(
            primitives::genesis::LawRegion::from_cid(3, city.institution_cid_number.as_bytes())
                .unwrap()
                .city_number,
            1
        );
        assert_eq!(scope.province_code, "GD");
        assert_eq!(scope.city_code, "001");
    }
}
