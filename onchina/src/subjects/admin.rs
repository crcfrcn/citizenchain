//! 主体管理 HTTP handler。
//!
//! 跨公权/私权共用的主体查名、详情、更新和父机构查询只读写
//! `subjects/accounts` 结构化表。

use axum::{
    extract::{Multipart, Path, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use serde::Serialize;

use crate::auth::actions::require_admin_security_grant;
use crate::auth::login::require_admin_any;
use crate::auth::operation_auth::AdminActionType;
use crate::institution::subjects::http::{resolve_created_by, service_error_to_response};
use crate::institution::subjects::model::{
    InstitutionDetailOutput, LegalRepresentative, ParentInstitutionRow,
    UpdateInstitutionInput,
};
use crate::institution::subjects::service::{
    resolve_legal_representative_scope_for_institution, validate_cid_full_name,
    validate_legal_representative_required,
};
use crate::institution::subjects::unincorporated_org;
use crate::scope::get_visible_scope;
use crate::*;

pub(crate) async fn check_cid_full_name(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Query(params): axum::extract::Query<CheckNameQuery>,
) -> impl IntoResponse {
    let ctx = match require_admin_any(&state, &headers) {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let cid_full_name = params.cid_full_name.trim().to_string();
    if cid_full_name.is_empty() {
        return api_error(StatusCode::BAD_REQUEST, 1001, "cid_full_name is required");
    }
    // 公法人/公权机构走"同市同全称"查重;私权机构仍全国查重。
    let is_public_legal = params.subject_property.as_deref().map(str::trim) == Some("G");
    let city = params.city_name.as_deref().unwrap_or("").trim().to_string();
    let exists = if is_public_legal {
        if city.is_empty() {
            return api_error(StatusCode::BAD_REQUEST, 1001, "公权机构查重需要 city 参数");
        }
        // 公权机构"同市同全称"查重:行政区单源 china.sqlite,按 province_code + city_code 比对
        // (subjects 不再存地名,名称仅在展示层派生)。省级作用域取自当前管理员会话。
        let province_name = match ctx.scope_province_name.as_deref().map(str::trim) {
            Some(p) if !p.is_empty() => p.to_string(),
            _ => return api_error(StatusCode::BAD_REQUEST, 1001, "公权机构查重需要省级作用域"),
        };
        let province_code = match crate::cid::china::province_code_by_name(&province_name) {
            Some(code) => code.to_string(),
            None => {
                return api_error(
                    StatusCode::BAD_REQUEST,
                    1001,
                    "省级作用域无法解析为行政区编码",
                );
            }
        };
        let city_code = match crate::cid::china::city_code_by_name(&province_name, &city) {
            Some(code) => code.to_string(),
            None => {
                return api_error(
                    StatusCode::BAD_REQUEST,
                    1001,
                    "city 参数无法解析为行政区编码",
                );
            }
        };
        let cid_full_name = cid_full_name.clone();
        match state.db.with_client(move |conn| {
            let row = conn
                .query_one(
                    "SELECT EXISTS (
                        SELECT 1 FROM subjects
                        WHERE kind = 'PUBLIC' AND cid_full_name = $1
                          AND province_code = $2 AND city_code = $3
                     )",
                    &[&cid_full_name, &province_code, &city_code],
                )
                .map_err(|e| format!("query city name conflict failed: {e}"))?;
            Ok(row.get(0))
        }) {
            Ok(v) => v,
            Err(err) => {
                let message = format!("query institution name failed: {err}");
                return api_error(StatusCode::INTERNAL_SERVER_ERROR, 5001, message.as_str());
            }
        }
    } else {
        match state
            .db
            .cid_full_name_exists(&cid_full_name, None, None, None)
        {
            Ok(v) => v,
            Err(err) => {
                let message = format!("query institution name failed: {err}");
                return api_error(StatusCode::INTERNAL_SERVER_ERROR, 5001, message.as_str());
            }
        }
    };
    Json(ApiResponse {
        code: 0,
        message: "ok".to_string(),
        data: CheckNameResult { exists },
    })
    .into_response()
}

#[derive(Debug, serde::Deserialize)]
pub struct CheckNameQuery {
    pub cid_full_name: String,
    /// 主体属性:G 公法人机构走"同市同全称"查重。
    pub subject_property: Option<String>,
    pub city_name: Option<String>,
}

#[derive(Debug, Serialize)]
struct CheckNameResult {
    exists: bool,
}

pub(crate) async fn upload_legal_representative_photo(
    State(state): State<AppState>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> impl IntoResponse {
    let ctx = match require_admin_any(&state, &headers) {
        Ok(ctx) => ctx,
        Err(resp) => return resp,
    };
    let mut upload = None;
    loop {
        let field = match multipart.next_field().await {
            Ok(Some(field)) => field,
            Ok(None) => break,
            Err(err) => return api_error(err.status(), 1001, "读取证件照失败"),
        };
        if field.name().unwrap_or("") != "file" { continue; }
        if upload.is_some() {
            return api_error(StatusCode::BAD_REQUEST, 1001, "一次只能上传一张证件照");
        }
        let name = field.file_name().unwrap_or("").trim().to_string();
        let mime = field.content_type().unwrap_or("").to_string();
        if name.is_empty() {
            return api_error(StatusCode::BAD_REQUEST, 1001, "file name is required");
        }
        if !matches!(mime.as_str(), "image/jpeg" | "image/png" | "image/webp") {
            return api_error(StatusCode::BAD_REQUEST, 1001, "证件照只支持 JPEG/PNG/WebP");
        }
        let bytes = match field.bytes().await {
            Ok(bytes) => bytes,
            Err(err) => return api_error(err.status(), 1001, "读取证件照失败"),
        };
        if bytes.is_empty() {
            return api_error(StatusCode::BAD_REQUEST, 1001, "file is empty");
        }
        if bytes.len() > super::service::MAX_LEGAL_REP_PHOTO_BYTES as usize {
            return api_error(StatusCode::PAYLOAD_TOO_LARGE, 1001, "证件照不能超过 5MB");
        }
        upload = Some((name, mime, bytes.to_vec()));
    }
    let Some((name, mime, bytes)) = upload else {
        return api_error(StatusCode::BAD_REQUEST, 1001, "file is required");
    };
    match state.db.insert_legal_representative_photo(name, mime, bytes, ctx.account_id.clone()) {
        Ok(photo) => Json(ApiResponse { code: 0, message: "ok".into(), data: photo }).into_response(),
        Err(err) => {
            tracing::error!(error = %err, "save legal representative photo failed");
            api_error(StatusCode::INTERNAL_SERVER_ERROR, 1004, "证件照保存失败")
        }
    }
}

pub(crate) async fn read_legal_representative_photo(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(photo_id): Path<String>,
) -> impl IntoResponse {
    let ctx = match require_admin_any(&state, &headers) {
        Ok(ctx) => ctx,
        Err(resp) => return resp,
    };
    let path = format!("{}{photo_id}", super::photos::PHOTO_URL_PREFIX);
    if !super::photos::valid_photo_path(&path) {
        return api_error(StatusCode::NOT_FOUND, 1004, "photo not found");
    }
    let photo = match state.db.read_legal_representative_photo(path.clone()) {
        Ok(Some(photo)) => photo,
        Ok(None) => return api_error(StatusCode::NOT_FOUND, 1004, "photo not found"),
        Err(err) => {
            tracing::error!(error = %err, "read legal representative photo failed");
            return api_error(StatusCode::INTERNAL_SERVER_ERROR, 1004, "证件照读取失败");
        }
    };
    if let Some(cid) = &photo.institution_cid_number {
        let inst = match state.db.get_institution_with_accounts(cid) {
            Ok(Some((inst, _))) => inst,
            Ok(None) => return api_error(StatusCode::NOT_FOUND, 1004, "institution not found"),
            Err(_) => return api_error(StatusCode::INTERNAL_SERVER_ERROR, 1004, "机构读取失败"),
        };
        let scope = get_visible_scope(&ctx);
        if inst.legal_representative_photo_path.as_deref() != Some(path.as_str())
            || !scope.includes_province(&inst.province_name)
            || !scope.includes_city(&inst.city_name)
            || !scope.includes_town(&inst.town_name)
        {
            return api_error(StatusCode::FORBIDDEN, 1003, "out of admin scope");
        }
    } else if photo.uploader_account_id != ctx.account_id {
        return api_error(StatusCode::FORBIDDEN, 1003, "photo belongs to another uploader");
    }
    (
        [
            (axum::http::header::CONTENT_TYPE, photo.mime_type.as_str()),
            (axum::http::header::CACHE_CONTROL, "private, no-store"),
            (axum::http::header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
        ],
        photo.content,
    ).into_response()
}

pub(crate) async fn update_institution(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(cid_number): Path<String>,
    Json(input): Json<UpdateInstitutionInput>,
) -> impl IntoResponse {
    let ctx = match require_admin_any(&state, &headers) {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let grant_payload = serde_json::json!({
        "target": cid_number.clone(),
        "cid_number": cid_number.clone(),
        "cid_full_name": input.cid_full_name.clone(),
        "parent_cid_number": input.parent_cid_number.clone(),
        "family_name": input.family_name.clone(),
        "given_name": input.given_name.clone(),
        "legal_representative_cid_number": input.legal_representative_cid_number.clone(),
        "legal_representative_photo_path": input.legal_representative_photo_path.clone(),
    });
    if let Err(resp) = require_admin_security_grant(
        &state,
        &headers,
        &ctx,
        AdminActionType::InstitutionUpdate,
        cid_number.as_str(),
        Some(&grant_payload),
    ) {
        return resp;
    }
    let Some((mut existing, _accounts)) = (match state.db.get_institution_with_accounts(&cid_number)
    {
        Ok(v) => v,
        Err(err) => {
            let message = format!("query institution failed: {err}");
            return api_error(StatusCode::INTERNAL_SERVER_ERROR, 5001, message.as_str());
        }
    }) else {
        return api_error(StatusCode::NOT_FOUND, 1004, "institution not found");
    };
    let old_cid_full_name = existing.cid_full_name.clone().unwrap_or_default();
    let old_parent_cid_number = existing.parent_cid_number.clone().unwrap_or_default();
    let scope = get_visible_scope(&ctx);
    if !scope.includes_province(&existing.province_name)
        || !scope.includes_city(&existing.city_name)
        || !scope.includes_town(&existing.town_name)
    {
        return api_error(StatusCode::FORBIDDEN, 1003, "out of admin scope");
    }

    if let Some(raw) = input
        .cid_full_name
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
    {
        let new_name = match validate_cid_full_name(raw) {
            Ok(v) => v,
            Err(e) => return service_error_to_response(e),
        };
        let conflict = match state
            .db
            .cid_full_name_exists(&new_name, None, None, Some(&cid_number))
        {
            Ok(v) => v,
            Err(err) => {
                let message = format!("query institution name failed: {err}");
                return api_error(StatusCode::INTERNAL_SERVER_ERROR, 5001, message.as_str());
            }
        };
        if conflict {
            return api_error(StatusCode::CONFLICT, 1007, "该机构全称已被使用");
        }
        existing.cid_full_name = Some(new_name);
    }
    if input.parent_cid_number.is_some() {
        let raw = input
            .parent_cid_number
            .as_deref()
            .unwrap_or("")
            .trim()
            .to_string();
        if !unincorporated_org::requires_parent(existing.institution_code.as_str()) {
            return api_error(StatusCode::BAD_REQUEST, 1001, "该主体类型不接受所属法人");
        }
        if raw.is_empty() {
            return api_error(StatusCode::BAD_REQUEST, 1001, "所属法人不能为空");
        }
        let Some((target, _)) = (match state.db.get_institution_with_accounts(&raw) {
            Ok(v) => v,
            Err(err) => {
                let message = format!("query parent institution failed: {err}");
                return api_error(StatusCode::INTERNAL_SERVER_ERROR, 5001, message.as_str());
            }
        }) else {
            return api_error(StatusCode::NOT_FOUND, 1004, "所属法人机构不存在");
        };
        if !unincorporated_org::can_attach_to_parent(target.institution_code.as_str()) {
            return api_error(
                StatusCode::BAD_REQUEST,
                1001,
                unincorporated_org::parent_subject_requirement_message(),
            );
        }
        // 改挂与创建同源校验(subjects/unincorporated_org 单一权威源):
        // 代码一致性(分校⇔学校本部) + 地域规则 + 盈利属性继承,缺一处就有绕过口。
        if let Some(msg) = unincorporated_org::code_consistency_violation(
            existing.institution_code.as_str(),
            target.institution_code.as_str(),
        ) {
            return api_error(StatusCode::BAD_REQUEST, 1001, msg);
        }
        let rule = unincorporated_org::parent_locality_rule(target.institution_code.as_str());
        if let Some(msg) = unincorporated_org::locality_violation(
            rule,
            &target.province_name,
            &target.city_name,
            &existing.province_name,
            &existing.city_name,
        ) {
            return api_error(StatusCode::BAD_REQUEST, 1001, msg);
        }
        let expected_p1 =
            unincorporated_org::inherited_p1(target.institution_code.as_str(), &target.p1);
        if existing.p1 != expected_p1 {
            return api_error(
                StatusCode::BAD_REQUEST,
                1001,
                "非法人盈利属性必须继承所属法人,该机构盈利属性与新所属法人不一致",
            );
        }
        existing.parent_cid_number = Some(raw);
    }
    let legal_rep = match validate_legal_representative_required(
        input.family_name.as_deref(),
        input.given_name.as_deref(),
        input.legal_representative_cid_number.as_deref(),
        input.legal_representative_photo_path.as_deref(),
        input.legal_representative_photo_name.as_deref(),
        input.legal_representative_photo_mime.as_deref(),
        input.legal_representative_photo_size,
    ) {
        Ok(v) => v,
        Err(e) => return service_error_to_response(e),
    };
    let parent_for_legal_representative_scope = match existing.parent_cid_number.as_deref() {
        Some(parent_cid) if !parent_cid.trim().is_empty() => {
            match state.db.get_institution_with_accounts(parent_cid) {
                Ok(v) => v.map(|(parent, _)| parent),
                Err(err) => {
                    let message = format!("query parent institution failed: {err}");
                    return api_error(StatusCode::INTERNAL_SERVER_ERROR, 5001, message.as_str());
                }
            }
        }
        _ => None,
    };
    let legal_representative_scope = resolve_legal_representative_scope_for_institution(
        &existing,
        parent_for_legal_representative_scope.as_ref(),
    );
    let legal_representative_account_id = match state.db.legal_representative_account_in_scope(
        legal_rep.cid_number.as_str(),
        &legal_representative_scope,
    ) {
        Ok(Some(account)) => account,
        Ok(None) => {
            return api_error(
                StatusCode::BAD_REQUEST,
                1001,
                legal_representative_scope.legal_representative_error_message(),
            );
        }
        Err(err) => {
            let message = format!("query legal representative failed: {err}");
            return api_error(StatusCode::INTERNAL_SERVER_ERROR, 5001, message.as_str());
        }
    };
    existing.legal_representative = Some(LegalRepresentative {
        family_name: legal_rep.family_name,
        given_name: legal_rep.given_name,
        cid_number: legal_rep.cid_number,
        account_id: legal_representative_account_id,
    });
    existing.legal_representative_photo_path = Some(legal_rep.photo_path);
    existing.legal_representative_photo_name = Some(legal_rep.photo_name);
    existing.legal_representative_photo_mime = Some(legal_rep.photo_mime);
    existing.legal_representative_photo_size = Some(legal_rep.photo_size);
    existing = match state.db.save_institution_with_photo(existing, ctx.account_id.clone()) {
        Ok(Some(inst)) => inst,
        Ok(None) => return api_error(StatusCode::BAD_REQUEST, 1001, "证件照不存在、已过期或不属于本次办理，请重新上传"),
        Err(err) => {
            tracing::error!(error = %err, "save institution and photo failed");
            return api_error(StatusCode::INTERNAL_SERVER_ERROR, 5001, "机构资料保存失败");
        }
    };
    crate::core::runtime_ops::append_audit_log(
        &state,
        "INSTITUTION_UPDATE",
        &ctx.account_id,
        Some(cid_number.clone()),
        serde_json::json!({
            "cid_number": cid_number.clone(),
            "old_cid_full_name": old_cid_full_name,
            "new_cid_full_name": existing.cid_full_name.clone().unwrap_or_default(),
            "old_parent_cid_number": old_parent_cid_number,
            "parent_cid_number": existing.parent_cid_number.clone().unwrap_or_default(),
            "family_name": existing.legal_representative.as_ref().map(|value| value.family_name.clone()).unwrap_or_default(),
            "given_name": existing.legal_representative.as_ref().map(|value| value.given_name.clone()).unwrap_or_default(),
            "legal_representative_cid_number": existing.legal_representative.as_ref().map(|value| value.cid_number.clone()).unwrap_or_default(),
        }),
    );
    Json(ApiResponse {
        code: 0,
        message: "ok".to_string(),
        data: existing,
    })
    .into_response()
}

#[derive(Debug, serde::Deserialize)]
pub(crate) struct SearchParentsQuery {
    pub q: Option<String>,
    /// 非法人的落位省/市,用于地域预过滤(规则同 subjects/unincorporated_org::parent_locality_rule)。
    pub province_name: Option<String>,
    pub city_name: Option<String>,
    /// 限定父级属性:S=仅私法人(私权入口) / G=仅公法人(公权入口);不传=两者(详情页改挂)。
    pub parent_property: Option<String>,
}

/// 所属法人搜索。SQL 预过滤与 `subjects/unincorporated_org` 规则同源(注释互引):
/// - f_institution=JY → 仅本市法人教育机构(手动 JY 行,G/S);
/// - 其它 → 私法人 S(非学校,全国) ∪ 公法人 G(非学校;手动行/市镇级同市、省级同省、国家级不限);
/// - parent_property 进一步收窄到单边。
pub(crate) async fn search_parent_institutions(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Query(query): axum::extract::Query<SearchParentsQuery>,
) -> impl IntoResponse {
    let ctx = match require_admin_any(&state, &headers) {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let q = query.q.as_deref().unwrap_or("").trim().to_lowercase();
    if q.is_empty() {
        return Json(ApiResponse {
            code: 0,
            message: "ok".to_string(),
            data: Vec::<ParentInstitutionRow>::new(),
        })
        .into_response();
    }
    let province = query
        .province_name
        .as_deref()
        .map(str::trim)
        .unwrap_or("")
        .to_string();
    let city = query
        .city_name
        .as_deref()
        .map(str::trim)
        .unwrap_or("")
        .to_string();
    if province.is_empty() || city.is_empty() {
        // 地域预过滤依赖非法人落位省市;缺参直接拒绝,不退化成全国搜索
        return api_error(
            StatusCode::BAD_REQUEST,
            1001,
            "province_name/city_name are required",
        );
    }
    // 管辖校验:父机构候选搜索必须落在当前管理员可见域内,绝不允许跨省/市探查他辖区机构。
    // 不校验镇:SearchParentsQuery 无镇维度,父机构粒度到市;镇级管理员已被 includes_city 限定本市。
    let scope = get_visible_scope(&ctx);
    if !scope.includes_province(&province) || !scope.includes_city(&city) {
        return api_error(StatusCode::FORBIDDEN, 1003, "out of admin scope");
    }
    // subjects 已不存行政区名字,地域预过滤改按 china.sqlite 派生的 code 走单源。
    let Some(province_code) = crate::cid::china::province_code_by_name(&province) else {
        return api_error(StatusCode::BAD_REQUEST, 1001, "unknown province");
    };
    let Some(city_code) = crate::cid::china::city_code_by_name(&province, &city) else {
        return api_error(StatusCode::BAD_REQUEST, 1001, "unknown city");
    };
    let province_code = province_code.to_string();
    let city_code = city_code.to_string();
    let parent_property = match query.parent_property.as_deref().map(str::trim) {
        None | Some("") => None,
        Some("S") => Some("S".to_string()),
        Some("G") => Some("G".to_string()),
        Some(_) => return api_error(StatusCode::BAD_REQUEST, 1001, "parent_property 仅 S/G"),
    };
    let result = state.db.with_client(move |conn| {
        // 候选父级按非法人规则预过滤(与 unincorporated_org::parent_locality_rule 同源)。
        // UNIN 是通用从属,可挂任意法人父级(私法人 S / 公法人 G)。地域规则:学校/大学父级 → 同市,
        // 非学校私法人 → 全国,非学校公法人 → 按机构码行政层级。$4 = parent_property 过滤(S/G)。
        // 私法人由机构码集合判定,公法人由 category 判定,层级由 institution_code 集合判定。
        let candidate_clause = "(
                  (s.institution_code IN ('GUN', 'SUN', 'JUN', 'GSCH', 'SFSC', 'JSCH')
                   AND s.province_code = $2 AND s.city_code = $3
                   AND ((s.institution_code IN ('SFLP', 'SFGQ', 'SFGF', 'SFGY', 'SFAS', 'SUN', 'JUN', 'SFSC', 'JSCH')
                         AND $4::text IS DISTINCT FROM 'G')
                        OR (s.category = 'GOV_INSTITUTION'
                            AND $4::text IS DISTINCT FROM 'S')))
                  OR (s.institution_code IN ('SFLP', 'SFGQ', 'SFGF', 'SFGY', 'SFAS', 'SUN', 'JUN', 'SFSC', 'JSCH')
                      AND s.institution_code NOT IN ('GUN', 'SUN', 'JUN', 'GSCH', 'SFSC', 'JSCH')
                      AND $4::text IS DISTINCT FROM 'G')
                  OR (s.category = 'GOV_INSTITUTION'
                      AND s.institution_code NOT IN ('GUN', 'SUN', 'JUN', 'GSCH', 'SFSC', 'JSCH')
                      AND $4::text IS DISTINCT FROM 'S'
                      AND (
                           s.institution_code IN (
                               'PRS','FSC','FIB','FSS','FPR','FRG','MFA','MDF','ARM','NAV','AIR',
                               'SPF','JOS','ARC','NVC','AFC','SFC','MHS','NGB','NGC','MCW','FDA',
                               'MHU','MAG','MCM','MFT','MEN','MTR','NLG','NSN','NRP','NJD','NSP',
                               'FAC','FAU','FIV','NED','NRC')
                           OR (s.institution_code IN (
                                   'PGV','PLG','PSN','PRP','PJD','PSP','PRC','PRB','PDF','PHS','PCW',
                                   'PHU','PAG','PCM','PFT','PEN','PTR')
                               AND s.province_code = $2)
                           OR (s.institution_code IN (
                                   'CGOV','CLEG','CSUP','CJUD','CEDU','CSLF','CDEF','CHSC','CCWF',
                                   'CHUD','CAGR','CCOM','CFIN','CENR','CTRN','CREG','CPOL',
                                   'TGOV','TCWF','THUD','TAGR','TFIN','TDEF','THSC','TCOM','TENR','TTRN',
                                   'TPOL','TSLF','TSUP','TJUD')
                               AND s.province_code = $2 AND s.city_code = $3)
                      ))
             )";
        let sql = format!(
            "SELECT s.cid_number, s.cid_full_name, s.private_type, s.partnership_kind, s.category,
                    s.p1, s.province_code, s.city_code, COALESCE(s.town_code, '')
             FROM subjects s
             WHERE s.kind IN ('PUBLIC', 'PRIVATE')
               AND COALESCE(s.cid_full_name, '') <> ''
               AND {candidate_clause}
               AND (lower(s.cid_number) LIKE '%' || $1 || '%'
                    OR lower(COALESCE(s.cid_full_name, '')) LIKE '%' || $1 || '%'
                    OR lower(COALESCE(s.cid_short_name, '')) LIKE '%' || $1 || '%')
             ORDER BY COALESCE(s.cid_short_name, '') ASC, COALESCE(s.cid_full_name, '') ASC, s.cid_number ASC
             LIMIT 20"
        );
        let rows = conn
            .query(
                sql.as_str(),
                &[&q, &province_code, &city_code, &parent_property],
            )
            .map_err(|e| format!("query parent institutions failed: {e}"))?;
        let mut output = Vec::with_capacity(rows.len());
        for row in rows {
            let category_text: String = row.get(4);
            let Some(category) = crate::institution_category_from_text(category_text.as_str())
            else {
                continue;
            };
            // 省/市/镇名字按 code 现场从 china.sqlite 派生,DTO 仍带名字。
            let row_province_code: String = row.get(6);
            let row_city_code: Option<String> = row.get(7);
            let row_town_code: String = row.get(8);
            let (province_name, city_name, town_name) = crate::cid::china::area_display_names(
                row_province_code.as_str(),
                row_city_code.as_deref(),
                Some(row_town_code.as_str()),
            );
            output.push(ParentInstitutionRow {
                cid_number: row.get(0),
                cid_full_name: row.get(1),
                private_type: row.get(2),
                partnership_kind: row.get(3),
                category,
                p1: row.get(5),
                province_name,
                city_name,
                town_name,
            });
        }
        Ok(output)
    });
    let hits = match result {
        Ok(v) => v,
        Err(err) => {
            let message = format!("query parent institutions failed: {err}");
            return api_error(StatusCode::INTERNAL_SERVER_ERROR, 5001, message.as_str());
        }
    };
    Json(ApiResponse {
        code: 0,
        message: "ok".to_string(),
        data: hits,
    })
    .into_response()
}

pub(crate) async fn get_institution(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(cid_number): Path<String>,
) -> impl IntoResponse {
    let ctx = match require_admin_any(&state, &headers) {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let Some((inst, accounts)) = (match state.db.get_institution_with_accounts(&cid_number) {
        Ok(v) => v,
        Err(err) => {
            let message = format!("query institution failed: {err}");
            return api_error(StatusCode::INTERNAL_SERVER_ERROR, 5001, message.as_str());
        }
    }) else {
        return api_error(StatusCode::NOT_FOUND, 1004, "institution not found");
    };
    let scope = get_visible_scope(&ctx);
    if !scope.includes_province(&inst.province_name)
        || !scope.includes_city(&inst.city_name)
        || !scope.includes_town(&inst.town_name)
    {
        return api_error(StatusCode::FORBIDDEN, 1003, "out of admin scope");
    }
    let (creator_family_name, creator_given_name, creator_institution_code) =
        resolve_created_by(&state, inst.creator_account_id.as_deref().unwrap_or(""));
    Json(ApiResponse {
        code: 0,
        message: "ok".to_string(),
        data: InstitutionDetailOutput {
            institution: inst,
            accounts,
            creator_family_name,
            creator_given_name,
            creator_institution_code,
        },
    })
    .into_response()
}

/// 联邦注册局机构详情(只读)。
/// 联邦注册局是全国唯一机构(位于中枢省),所有省份管理员都需要进入它的机构详情页查看
/// 本省联邦注册局机构管理员列表,因此这里**不做 scope 校验**(与 get_institution 的唯一区别)。
/// 仍要求已登录管理员;只返回 FEDERAL_REGISTRY 这一个机构。
pub(crate) async fn get_federal_registry(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err(resp) = require_admin_any(&state, &headers) {
        return resp;
    }
    // 联邦注册局 CID 也来自链上公权机构投影,按机构码查唯一 CID 后绕过 scope 展示。
    let cid_number = match state.db.chain_public_institution_cid_by_code("FRG") {
        Ok(Some(cid_number)) => cid_number,
        Ok(None) => {
            return api_error(
                StatusCode::NOT_FOUND,
                1004,
                "federal registry not configured",
            );
        }
        Err(err) => {
            let message = format!("query federal registry code failed: {err}");
            return api_error(StatusCode::INTERNAL_SERVER_ERROR, 5001, message.as_str());
        }
    };
    let Some((inst, accounts)) = (match state.db.get_institution_with_accounts(&cid_number) {
        Ok(v) => v,
        Err(err) => {
            let message = format!("query federal registry failed: {err}");
            return api_error(StatusCode::INTERNAL_SERVER_ERROR, 5001, message.as_str());
        }
    }) else {
        return api_error(StatusCode::NOT_FOUND, 1004, "federal registry not found");
    };
    let (creator_family_name, creator_given_name, creator_institution_code) =
        resolve_created_by(&state, inst.creator_account_id.as_deref().unwrap_or(""));
    Json(ApiResponse {
        code: 0,
        message: "ok".to_string(),
        data: InstitutionDetailOutput {
            institution: inst,
            accounts,
            creator_family_name,
            creator_given_name,
            creator_institution_code,
        },
    })
    .into_response()
}
