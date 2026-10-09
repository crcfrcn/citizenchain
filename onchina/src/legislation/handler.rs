//! 立法与表决 HTTP handler(`/api/legislation/*`)。
//!
//! 发起/表决产出扫码上链 `sign_request`；CitizenWallet 只签名一次并显示响应二维码，OnChina 回扫响应后通过统一入口提交；
//! 读法律/提案进度直读链。后端强制:① 登录绑定机构(只有该院管理员可达)② 本机构能否发起该
//! 类型提案(`category::proposable_candidates`)③ 越权前置(`service::precheck_legislation_scope`)。
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
use super::law::model::{institution_code_text, LawView, ProposeLawInput};
use super::law::{action, chain_read, service};
use super::model::ProposalCategory;
use crate::auth::login::{require_admin_any, AdminAuthContext};
use crate::cid::china::{city_code_by_name, province_code_by_name};
use crate::core::response::ApiResponse;
use crate::{api_error, AppState};
use primitives::cid::code::institution_code_from_str;

/// 法律列表查询参数(层级 + 行政区码)。
#[derive(Debug, Deserialize)]
pub(crate) struct LawListQuery {
    pub tier: u8,
    pub scope_code: u32,
}

/// 代表机构表决请求体。
#[derive(Debug, Deserialize)]
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

/// 从登录上下文派生 (admin_scope_code, province_china_code, city_china_code)。
///
/// `admin_scope_code` 与提案 `scope_code` 同口径(china.sqlite 码 u32;国家=0);
/// province/city china 码供 subjects 查机构账户。解析失败以 `u32::MAX` 兜底 → precheck fail-closed。
fn scope_codes(ctx: &AdminAuthContext) -> (u32, String, String) {
    match ctx.admin_level.as_deref() {
        Some("NATIONAL") => (0, String::new(), String::new()),
        Some("PROVINCE") => {
            let province = ctx
                .scope_province_name
                .as_deref()
                .and_then(province_code_by_name)
                .unwrap_or_default()
                .to_string();
            (
                province.parse().unwrap_or(u32::MAX),
                province,
                String::new(),
            )
        }
        Some("CITY") => {
            let province_name = ctx.scope_province_name.as_deref().unwrap_or_default();
            let province = province_code_by_name(province_name)
                .unwrap_or_default()
                .to_string();
            let city = ctx
                .scope_city_name
                .as_deref()
                .and_then(|c| city_code_by_name(province_name, c))
                .unwrap_or_default()
                .to_string();
            (city.parse().unwrap_or(u32::MAX), province, city)
        }
        _ => (u32::MAX, String::new(), String::new()),
    }
}

/// GET /api/legislation/laws?tier=&scope_code= —— 本级已生效/在册法律列表。
pub(crate) async fn list_laws(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<LawListQuery>,
) -> impl IntoResponse {
    if let Err(resp) = require_admin_any(&state, &headers) {
        return resp;
    }
    let laws = match chain_read::list_laws_by_scope(query.tier, query.scope_code).await {
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
/// 国家级并入宪法(tier 0)+ 国家法律(tier 1);省(2)/市(3)按本级 + 本辖区 china scope 码
/// (与 precheck/resolve 同口径,解掉前端拿不到 scope_code 的问题)。
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
    let (scope_code, _, _) = scope_codes(&ctx);
    let tiers: &[u8] = match admin_tier {
        1 => &[0, 1], // 宪法 + 国家法律
        2 => &[2],
        3 => &[3],
        _ => &[],
    };
    let mut laws: Vec<chain_read::OnChainLaw> = Vec::new();
    for &tier in tiers {
        match chain_read::list_laws_by_scope(tier, scope_code).await {
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

/// GET /api/legislation/laws/:law_id —— 单部法律办理端展示版本全文。
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

/// GET /api/legislation/proposals/:proposal_id —— 提案进度只读投影。
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
    let Some(admin_tier) = admin_tier(&ctx) else {
        return api_error(StatusCode::FORBIDDEN, 1003, "admin level missing");
    };
    // 会话派生 scope_code 覆盖前端(前端拿不到 china scope_code;防越权伪造表决辖区)。
    let (admin_scope_code, province_code, city_code) = scope_codes(&ctx);
    let mut input = input;
    input.scope_code = admin_scope_code;
    // ② 本机构能否发起该表决类型(参议会/非立法机构无候选 → 拒);层级由 ③ precheck 校验,
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
    // ③ 越权前置(层级;scope 经会话派生已一致)。
    if let Err(err) = service::precheck_legislation_scope(
        admin_tier,
        admin_scope_code,
        input.tier,
        input.scope_code,
    ) {
        return api_error(StatusCode::FORBIDDEN, 1003, err.code());
    }
    // houses/actor/executive/legislature 只按宪法路由 + subjects 解析机构 CID。
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
    let passkey = match crate::auth::passkey::require_passkey_assertion(
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
    let passkey = match crate::auth::passkey::require_passkey_assertion(
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
