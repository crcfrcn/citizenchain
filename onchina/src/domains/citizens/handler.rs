//! 公民列表 / 公开身份查询 handlers
//!
//! 公民查询能力属于 citizens 模块,不属于权限范围规则。
//! 因此本文件承接后台公民列表和公开身份查询入口。

// 公民 handler 的失败分支统一携带完整 Axum Response，避免公开与管理入口形成双轨错误模型。
#![allow(clippy::result_large_err)]

use axum::{
    body::Body,
    extract::{Multipart, Path, Query, State},
    http::{header, HeaderMap, StatusCode},
    response::IntoResponse,
};
use chrono::Utc;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::*;

struct StoredCitizenDocument {
    meta: CitizenDocument,
    file_path: String,
}

pub(crate) async fn admin_list_citizens(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<CitizensQuery>,
) -> impl IntoResponse {
    let auth_ctx = match require_admin_any(&state, &headers) {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let (scope_province_code, scope_city_code) =
        match resolve_citizen_query_scope(&auth_ctx, &query) {
            Ok(v) => v,
            Err(resp) => return resp,
        };

    let keyword = query.keyword.unwrap_or_default();
    let page_size = query.page_size.or(query.limit).unwrap_or(50).clamp(1, 100);
    if query.offset.unwrap_or(0) > 0 {
        return api_error(
            StatusCode::BAD_REQUEST,
            1001,
            "offset pagination is not supported",
        );
    }
    let page = match state.db.list_citizens_page(
        keyword.as_str(),
        scope_province_code.as_deref(),
        scope_city_code.as_deref(),
        query.cursor.as_deref(),
        page_size,
    ) {
        Ok(v) => v,
        Err(e) if e == "invalid page cursor" => {
            return api_error(StatusCode::BAD_REQUEST, 1001, "invalid page cursor");
        }
        Err(e) => {
            tracing::warn!(error = %e, "admin_list_citizens failed");
            return api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                1004,
                "citizen query failed",
            );
        }
    };

    Json(ApiResponse {
        code: 0,
        message: "ok".to_string(),
        data: page,
    })
    .into_response()
}

fn resolve_citizen_query_scope(
    auth_ctx: &crate::auth::login::AdminAuthContext,
    query: &CitizensQuery,
) -> Result<(Option<String>, Option<String>), axum::response::Response> {
    let scope = crate::scope::get_visible_scope(auth_ctx);
    if !scope.can_write {
        return Err(api_error(
            StatusCode::FORBIDDEN,
            1003,
            "当前登录无公民办理权限",
        ));
    }
    let province_name = query
        .province_name
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_string)
        .or_else(|| scope.locked_province_name.clone())
        .ok_or_else(|| api_error(StatusCode::BAD_REQUEST, 1001, "province_name is required"))?;
    let city_name = query
        .city_name
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_string)
        .or_else(|| scope.locked_city_name.clone())
        .ok_or_else(|| api_error(StatusCode::BAD_REQUEST, 1001, "city_name is required"))?;
    if !scope.includes_province(province_name.as_str()) {
        return Err(api_error(
            StatusCode::FORBIDDEN,
            1003,
            "province_name out of current admin scope",
        ));
    }
    if !scope.includes_city(city_name.as_str()) {
        return Err(api_error(
            StatusCode::FORBIDDEN,
            1003,
            "city_name out of current admin scope",
        ));
    }
    let Some(province) = crate::cid::china::provinces()
        .iter()
        .find(|p| p.province_name == province_name)
    else {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            1001,
            "unknown province_name",
        ));
    };
    let Some(city) = province.cities.iter().find(|c| c.city_name == city_name) else {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            1001,
            "unknown city_name",
        ));
    };
    Ok((
        Some(province.province_code.to_string()),
        Some(city.city_code.to_string()),
    ))
}

#[derive(Debug, serde::Deserialize)]
pub(crate) struct LegalRepresentativeCitizenQuery {
    pub q: Option<String>,
    pub page_size: Option<usize>,
    pub target_cid_number: Option<String>,
    pub province_name: Option<String>,
    pub city_name: Option<String>,
    pub institution: Option<String>,
    pub education_type: Option<String>,
    pub parent_cid_number: Option<String>,
}

fn legal_representative_scope_from_existing_target(
    state: &AppState,
    auth_ctx: &crate::auth::login::AdminAuthContext,
    target_cid_number: &str,
) -> Result<
    crate::institution::subjects::service::LegalRepresentativeCitizenScope,
    axum::response::Response,
> {
    let Some((target, _)) = (match state.db.get_institution_with_accounts(target_cid_number) {
        Ok(v) => v,
        Err(err) => {
            let message = format!("query institution failed: {err}");
            return Err(api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                5001,
                message.as_str(),
            ));
        }
    }) else {
        return Err(api_error(
            StatusCode::NOT_FOUND,
            1004,
            "institution not found",
        ));
    };
    crate::institution::subjects::http::ensure_institution_visible_to_admin(&target, auth_ctx)?;

    let parent = match target.parent_cid_number.as_deref() {
        Some(parent_cid) if !parent_cid.trim().is_empty() => {
            match state.db.get_institution_with_accounts(parent_cid) {
                Ok(v) => v.map(|(parent, _)| parent),
                Err(err) => {
                    let message = format!("query parent institution failed: {err}");
                    return Err(api_error(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        5001,
                        message.as_str(),
                    ));
                }
            }
        }
        _ => None,
    };
    Ok(
        crate::institution::subjects::service::resolve_legal_representative_scope_for_institution(
            &target,
            parent.as_ref(),
        ),
    )
}

fn legal_representative_scope_from_create_context(
    state: &AppState,
    auth_ctx: &crate::auth::login::AdminAuthContext,
    query: &LegalRepresentativeCitizenQuery,
) -> Result<
    crate::institution::subjects::service::LegalRepresentativeCitizenScope,
    axum::response::Response,
> {
    let province_name = query
        .province_name
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| api_error(StatusCode::BAD_REQUEST, 1001, "province_name is required"))?;
    let city_name = query
        .city_name
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| api_error(StatusCode::BAD_REQUEST, 1001, "city_name is required"))?;
    let scope = crate::scope::get_visible_scope(auth_ctx);
    if !scope.includes_province(province_name) {
        return Err(api_error(
            StatusCode::FORBIDDEN,
            1003,
            "province_name out of current admin scope",
        ));
    }
    if !scope.includes_city(city_name) {
        return Err(api_error(
            StatusCode::FORBIDDEN,
            1003,
            "city_name out of current admin scope",
        ));
    }
    let institution = query
        .institution
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| api_error(StatusCode::BAD_REQUEST, 1001, "institution is required"))?;
    let Some(province_code) = crate::cid::china::province_code_by_name(province_name) else {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            1001,
            "unknown province_name",
        ));
    };
    let Some(city_code) = crate::cid::china::city_code_by_name(province_name, city_name) else {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            1001,
            "unknown city_name",
        ));
    };

    let parent = match query.parent_cid_number.as_deref().map(str::trim) {
        Some(parent_cid) if !parent_cid.is_empty() => {
            let Some((parent, _)) = (match state.db.get_institution_with_accounts(parent_cid) {
                Ok(v) => v,
                Err(err) => {
                    let message = format!("query parent institution failed: {err}");
                    return Err(api_error(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        5001,
                        message.as_str(),
                    ));
                }
            }) else {
                return Err(api_error(StatusCode::NOT_FOUND, 1004, "所属法人机构不存在"));
            };
            if !crate::institution::subjects::unincorporated_org::can_attach_to_parent(
                parent.institution_code.as_str(),
            ) {
                return Err(api_error(
                    StatusCode::BAD_REQUEST,
                    1001,
                    crate::institution::subjects::unincorporated_org::parent_subject_requirement_message(),
                ));
            }
            Some(parent)
        }
        _ if crate::institution::subjects::unincorporated_org::requires_parent(institution) => {
            return Err(api_error(StatusCode::BAD_REQUEST, 1001, "请先选择所属法人"));
        }
        _ => None,
    };

    Ok(
        crate::institution::subjects::service::resolve_legal_representative_scope_for_codes(
            institution,
            query.education_type.as_deref().map(str::trim),
            province_code,
            city_code,
            parent.as_ref(),
        ),
    )
}

pub(crate) async fn admin_search_legal_representative_citizens(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<LegalRepresentativeCitizenQuery>,
) -> impl IntoResponse {
    let auth_ctx = match require_admin_any(&state, &headers) {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let q = query.q.as_deref().map(str::trim).unwrap_or("").to_string();
    if q.is_empty() {
        return Json(ApiResponse {
            code: 0,
            message: "ok".to_string(),
            data: Vec::<String>::new(),
        })
        .into_response();
    }
    let legal_representative_scope = match query
        .target_cid_number
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
    {
        Some(target_cid_number) => {
            match legal_representative_scope_from_existing_target(
                &state,
                &auth_ctx,
                target_cid_number,
            ) {
                Ok(v) => v,
                Err(resp) => return resp,
            }
        }
        None => match legal_representative_scope_from_create_context(&state, &auth_ctx, &query) {
            Ok(v) => v,
            Err(resp) => return resp,
        },
    };
    let page_size = query.page_size.unwrap_or(20).clamp(1, 50);
    let rows = match state.db.search_legal_representative_citizens_in_scope(
        &q,
        page_size,
        &legal_representative_scope,
    ) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(error = %e, "legal representative citizen search failed");
            return api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                1004,
                "citizen query failed",
            );
        }
    };
    Json(ApiResponse {
        code: 0,
        message: "ok".to_string(),
        data: rows,
    })
    .into_response()
}

impl Db {
    fn list_citizen_documents(
        &self,
        province_code: &str,
        cid_number: &str,
    ) -> Result<Vec<CitizenDocument>, String> {
        let province_code = province_code.trim().to_string();
        let cid_number = cid_number.trim().to_string();
        self.with_client(move |conn| {
            let rows = conn
                .query(
                    "SELECT id, cid_number, file_name, document_type, file_size, file_hash,
                            uploader_account_id, uploaded_at
                     FROM citizen_documents
                     WHERE province_code = $1 AND cid_number = $2
                     ORDER BY uploaded_at DESC, id DESC",
                    &[&province_code, &cid_number],
                )
                .map_err(|e| format!("query citizen documents failed: {e}"))?;
            rows.iter()
                .map(|row| {
                    let id: i64 = row.get(0);
                    let file_size: i64 = row.get(4);
                    Ok(CitizenDocument {
                        id: u64::try_from(id).unwrap_or(0),
                        cid_number: row.get(1),
                        file_name: row.get(2),
                        document_type: row.get(3),
                        file_size: u64::try_from(file_size).unwrap_or(0),
                        file_hash: row.get(5),
                        uploader_account_id: row.get(6),
                        uploaded_at: row.get(7),
                    })
                })
                .collect()
        })
    }

    fn insert_citizen_document(
        &self,
        doc: &CitizenDocument,
        province_code: &str,
        city_code: &str,
        file_path: &str,
    ) -> Result<CitizenDocument, String> {
        let doc = doc.clone();
        let province_code = province_code.to_string();
        let city_code = city_code.to_string();
        let file_path = file_path.to_string();
        self.with_client(move |conn| {
            let file_size = i64::try_from(doc.file_size)
                .map_err(|_| "citizen document file size too large".to_string())?;
            let row = conn
                .query_one(
                    "INSERT INTO citizen_documents (
                        cid_number, province_code, city_code, file_name, document_type,
                        file_size, file_path, file_hash, uploader_account_id, uploaded_at
                     ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
                     RETURNING id",
                    &[
                        &doc.cid_number,
                        &province_code,
                        &city_code,
                        &doc.file_name,
                        &doc.document_type,
                        &file_size,
                        &file_path,
                        &doc.file_hash,
                        &doc.uploader_account_id,
                        &doc.uploaded_at,
                    ],
                )
                .map_err(|e| format!("insert citizen document failed: {e}"))?;
            let id: i64 = row.get(0);
            Ok(CitizenDocument {
                id: u64::try_from(id).unwrap_or(0),
                ..doc
            })
        })
    }

    fn get_citizen_document(
        &self,
        province_code: &str,
        cid_number: &str,
        doc_id: u64,
    ) -> Result<Option<StoredCitizenDocument>, String> {
        let province_code = province_code.trim().to_string();
        let cid_number = cid_number.trim().to_string();
        let doc_id =
            i64::try_from(doc_id).map_err(|_| "citizen document id too large".to_string())?;
        self.with_client(move |conn| {
            let row = conn
                .query_opt(
                    "SELECT id, cid_number, file_name, document_type, file_size, file_path,
                            file_hash, uploader_account_id, uploaded_at
                     FROM citizen_documents
                     WHERE province_code = $1 AND cid_number = $2 AND id = $3",
                    &[&province_code, &cid_number, &doc_id],
                )
                .map_err(|e| format!("query citizen document failed: {e}"))?;
            Ok(row.map(|row| {
                let id: i64 = row.get(0);
                let file_size: i64 = row.get(4);
                StoredCitizenDocument {
                    meta: CitizenDocument {
                        id: u64::try_from(id).unwrap_or(0),
                        cid_number: row.get(1),
                        file_name: row.get(2),
                        document_type: row.get(3),
                        file_size: u64::try_from(file_size).unwrap_or(0),
                        file_hash: row.get(6),
                        uploader_account_id: row.get(7),
                        uploaded_at: row.get(8),
                    },
                    file_path: row.get(5),
                }
            }))
        })
    }

    fn delete_citizen_document(
        &self,
        province_code: &str,
        cid_number: &str,
        doc_id: u64,
    ) -> Result<bool, String> {
        let province_code = province_code.trim().to_string();
        let cid_number = cid_number.trim().to_string();
        let doc_id =
            i64::try_from(doc_id).map_err(|_| "citizen document id too large".to_string())?;
        self.with_client(move |conn| {
            let affected = conn
                .execute(
                    "DELETE FROM citizen_documents WHERE province_code = $1 AND cid_number = $2 AND id = $3",
                    &[&province_code, &cid_number, &doc_id],
                )
                .map_err(|e| format!("delete citizen document failed: {e}"))?;
            Ok(affected > 0)
        })
    }
}

fn ensure_citizen_document_scope(
    state: &AppState,
    auth_ctx: &crate::auth::login::AdminAuthContext,
    cid_number: &str,
) -> Result<CitizenRecord, axum::response::Response> {
    let scope = crate::scope::get_visible_scope(auth_ctx);
    if !scope.can_write {
        return Err(api_error(
            StatusCode::FORBIDDEN,
            1003,
            "当前登录无公民资料库权限",
        ));
    }
    let record = match state.db.find_citizen_by_cid(cid_number) {
        Ok(Some(v)) => v,
        Ok(None) => {
            return Err(api_error(StatusCode::NOT_FOUND, 1004, "公民档案不存在"));
        }
        Err(err) => {
            tracing::error!(error = %err, "query citizen for document scope failed");
            return Err(api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                1004,
                "公民档案查询失败",
            ));
        }
    };
    let province_name =
        crate::cid::china::area_name_by_codes(record.province_code.as_str(), None, None)
            .map(|(province, _, _)| province.to_string())
            .unwrap_or_default();
    let city_name = crate::cid::china::area_name_by_codes(
        record.province_code.as_str(),
        Some(record.city_code.as_str()),
        None,
    )
    .and_then(|(_, city, _)| city.map(str::to_string))
    .unwrap_or_default();
    if !scope.includes_province(province_name.as_str()) || !scope.includes_city(city_name.as_str())
    {
        return Err(api_error(
            StatusCode::FORBIDDEN,
            1003,
            "公民档案不在当前注册局办理范围内",
        ));
    }
    Ok(record)
}

pub(crate) async fn list_citizen_documents(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(cid_number): Path<String>,
) -> impl IntoResponse {
    let ctx = match require_admin_any(&state, &headers) {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let record = match ensure_citizen_document_scope(&state, &ctx, cid_number.as_str()) {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let docs = match state
        .db
        .list_citizen_documents(record.province_code.as_str(), cid_number.as_str())
    {
        Ok(v) => v,
        Err(err) => {
            tracing::error!(error = %err, "list citizen documents failed");
            return api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                1004,
                "公民资料库查询失败",
            );
        }
    };
    Json(ApiResponse {
        code: 0,
        message: "ok".to_string(),
        data: docs,
    })
    .into_response()
}

pub(crate) async fn upload_citizen_document(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(cid_number): Path<String>,
    mut multipart: Multipart,
) -> impl IntoResponse {
    let ctx = match require_admin_any(&state, &headers) {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let record = match ensure_citizen_document_scope(&state, &ctx, cid_number.as_str()) {
        Ok(v) => v,
        Err(resp) => return resp,
    };

    let mut file_name: Option<String> = None;
    let mut file_data: Option<Vec<u8>> = None;
    let mut document_type: Option<String> = None;
    while let Ok(Some(field)) = multipart.next_field().await {
        let name = field.name().unwrap_or("").to_string();
        match name.as_str() {
            "file" => {
                file_name = field.file_name().map(str::to_string);
                match field.bytes().await {
                    Ok(bytes) => file_data = Some(bytes.to_vec()),
                    Err(e) => {
                        let message = format!("读取文件失败: {e}");
                        return api_error(StatusCode::BAD_REQUEST, 1001, message.as_str());
                    }
                }
            }
            "document_type" => {
                if let Ok(text) = field.text().await {
                    document_type = Some(text.trim().to_string());
                }
            }
            _ => {}
        }
    }

    let file_name = match file_name.filter(|s| !s.trim().is_empty()) {
        Some(v) => v,
        None => return api_error(StatusCode::BAD_REQUEST, 1001, "file is required"),
    };
    let file_data = match file_data.filter(|d| !d.is_empty()) {
        Some(v) => v,
        None => return api_error(StatusCode::BAD_REQUEST, 1001, "file is empty"),
    };
    let document_type =
        match document_type.filter(|value| CITIZEN_DOCUMENT_TYPES.contains(&value.as_str())) {
            Some(v) => v,
            None => {
                return api_error(
                    StatusCode::BAD_REQUEST,
                    1001,
                    "invalid citizen document_type",
                )
            }
        };
    if file_data.len() > 10 * 1024 * 1024 {
        return api_error(StatusCode::BAD_REQUEST, 1001, "文件大小不能超过 10MB");
    }

    let mut hasher = Sha256::new();
    hasher.update(&file_data);
    let file_hash = format!("0x{}", hex::encode(hasher.finalize()));
    let doc_dir = format!("data/citizen-documents/{}", record.cid_number);
    if let Err(e) = std::fs::create_dir_all(&doc_dir) {
        tracing::error!(error = %e, "create citizen document dir failed");
        return api_error(StatusCode::INTERNAL_SERVER_ERROR, 1004, "create dir failed");
    }
    let file_ext = std::path::Path::new(&file_name)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("bin");
    let stored_name = format!(
        "{}_{}.{}",
        Utc::now().format("%Y%m%d%H%M%S"),
        Uuid::new_v4().as_simple(),
        file_ext
    );
    let stored_path = format!("{doc_dir}/{stored_name}");
    if let Err(e) = std::fs::write(&stored_path, &file_data) {
        tracing::error!(error = %e, "write citizen document file failed");
        return api_error(StatusCode::INTERNAL_SERVER_ERROR, 1004, "write file failed");
    }

    let doc = CitizenDocument {
        id: 0,
        cid_number: record.cid_number.clone(),
        file_name,
        document_type,
        file_size: file_data.len() as u64,
        file_hash,
        uploader_account_id: ctx.account_id.clone(),
        uploaded_at: Utc::now(),
    };
    let doc = match state.db.insert_citizen_document(
        &doc,
        record.province_code.as_str(),
        record.city_code.as_str(),
        stored_path.as_str(),
    ) {
        Ok(v) => v,
        Err(err) => {
            tracing::error!(error = %err, "insert citizen document failed");
            if let Err(remove_err) = std::fs::remove_file(stored_path.as_str()) {
                if remove_err.kind() != std::io::ErrorKind::NotFound {
                    tracing::warn!(
                        error = %remove_err,
                        path = %stored_path,
                        "remove orphan citizen document file failed"
                    );
                }
            }
            return api_error(StatusCode::INTERNAL_SERVER_ERROR, 1004, "公民资料落库失败");
        }
    };
    crate::core::runtime_ops::append_audit_log(
        &state,
        "CITIZEN_DOCUMENT_UPLOAD",
        &ctx.account_id,
        Some(record.cid_number.clone()),
        serde_json::json!({
            "cid_number": record.cid_number,
            "document_type": doc.document_type,
            "file_name": doc.file_name,
            "file_size": doc.file_size,
            "file_hash": doc.file_hash,
            "request_id": request_id_from_headers(&headers),
            "actor_ip": actor_ip_from_headers(&headers),
        }),
    );
    Json(ApiResponse {
        code: 0,
        message: "ok".to_string(),
        data: doc,
    })
    .into_response()
}

pub(crate) async fn download_citizen_document(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((cid_number, doc_id)): Path<(String, u64)>,
) -> impl IntoResponse {
    let ctx = match require_admin_any(&state, &headers) {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let record = match ensure_citizen_document_scope(&state, &ctx, cid_number.as_str()) {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let Some(doc) = (match state.db.get_citizen_document(
        record.province_code.as_str(),
        cid_number.as_str(),
        doc_id,
    ) {
        Ok(v) => v,
        Err(err) => {
            tracing::error!(error = %err, "get citizen document failed");
            return api_error(StatusCode::INTERNAL_SERVER_ERROR, 1004, "公民资料查询失败");
        }
    }) else {
        return api_error(StatusCode::NOT_FOUND, 1004, "公民资料不存在");
    };
    let bytes = match std::fs::read(doc.file_path.as_str()) {
        Ok(v) => v,
        Err(err) => {
            tracing::error!(error = %err, path = %doc.file_path, "read citizen document failed");
            return api_error(StatusCode::NOT_FOUND, 1004, "公民资料文件不存在");
        }
    };
    let file_name = doc.meta.file_name.replace(['\r', '\n', '"'], "_");
    (
        [
            (header::CONTENT_TYPE, "application/octet-stream".to_string()),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{file_name}\""),
            ),
        ],
        Body::from(bytes),
    )
        .into_response()
}

pub(crate) async fn delete_citizen_document(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((cid_number, doc_id)): Path<(String, u64)>,
) -> impl IntoResponse {
    let ctx = match require_admin_any(&state, &headers) {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let record = match ensure_citizen_document_scope(&state, &ctx, cid_number.as_str()) {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let Some(doc) = (match state.db.get_citizen_document(
        record.province_code.as_str(),
        cid_number.as_str(),
        doc_id,
    ) {
        Ok(v) => v,
        Err(err) => {
            tracing::error!(error = %err, "get citizen document before delete failed");
            return api_error(StatusCode::INTERNAL_SERVER_ERROR, 1004, "公民资料查询失败");
        }
    }) else {
        return api_error(StatusCode::NOT_FOUND, 1004, "公民资料不存在");
    };
    match state.db.delete_citizen_document(
        record.province_code.as_str(),
        cid_number.as_str(),
        doc_id,
    ) {
        Ok(true) => {}
        Ok(false) => return api_error(StatusCode::NOT_FOUND, 1004, "公民资料不存在"),
        Err(err) => {
            tracing::error!(error = %err, "delete citizen document failed");
            return api_error(StatusCode::INTERNAL_SERVER_ERROR, 1004, "公民资料删除失败");
        }
    }
    if let Err(err) = std::fs::remove_file(doc.file_path.as_str()) {
        if err.kind() != std::io::ErrorKind::NotFound {
            tracing::warn!(error = %err, path = %doc.file_path, "remove citizen document file failed");
        }
    }
    crate::core::runtime_ops::append_audit_log(
        &state,
        "CITIZEN_DOCUMENT_DELETE",
        &ctx.account_id,
        Some(record.cid_number.clone()),
        serde_json::json!({
            "cid_number": record.cid_number,
            "doc_id": doc_id,
            "document_type": doc.meta.document_type,
            "file_name": doc.meta.file_name,
            "file_hash": doc.meta.file_hash,
            "request_id": request_id_from_headers(&headers),
            "actor_ip": actor_ip_from_headers(&headers),
        }),
    );
    Json(ApiResponse {
        code: 0,
        message: "ok".to_string(),
        data: "ok",
    })
    .into_response()
}

pub(crate) async fn public_identity_search(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<PublicIdentitySearchQuery>,
) -> impl IntoResponse {
    // 查询结果仅含公开信息（CID 码等），无需 token 认证。
    // 全局 rate limiter 已防滥用。
    // 公开查询只返回公民档案已登记后的公开字段。
    let identity_code = query.identity_code.as_deref().map(str::trim).unwrap_or("");
    let account_id = query.account_id.as_deref().map(str::trim).unwrap_or("");
    if identity_code.is_empty() && account_id.is_empty() {
        return api_error(
            StatusCode::BAD_REQUEST,
            1001,
            "identity_code or account_id is required",
        );
    }
    if !account_id.is_empty() && crate::crypto::pubkey::normalize_account_id(account_id).is_none() {
        return api_error(
            StatusCode::BAD_REQUEST,
            1001,
            "account_id must be lowercase 0x plus 64 hexadecimal characters",
        );
    }

    let actor_ip = actor_ip_from_headers(&headers);
    let request_id = request_id_from_headers(&headers);
    let found = match state.db.with_client({
        let identity_code = identity_code.to_string();
        let account_id = account_id.to_string();
        move |conn| {
            let row = conn
                .query_opt(
                    "SELECT cid_number, account_id
                     FROM citizens
                     WHERE (
                            ($1::text <> '' AND cid_number = $1)
                            OR ($2::text <> '' AND account_id = $2)
                       )
                     ORDER BY created_at DESC
                     LIMIT 1",
                    &[&identity_code, &account_id],
                )
                .map_err(|e| format!("public citizen lookup failed: {e}"))?;
            Ok(row.map(|row| (row.get::<_, String>(0), row.get::<_, Option<String>>(1))))
        }
    }) {
        Ok(v) => v,
        Err(err) => {
            tracing::warn!(error = %err, "public_identity_search failed");
            return api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                1004,
                "identity query failed",
            );
        }
    };
    let output = PublicIdentitySearchOutput {
        found: found.is_some(),
        identity_code: found.as_ref().map(|r| r.0.clone()),
        account_id: found.as_ref().and_then(|r| r.1.clone()),
    };
    crate::core::runtime_ops::append_audit_log(
        &state,
        "PUBLIC_IDENTITY_SEARCH",
        "public",
        output.account_id.clone(),
        serde_json::json!({
            "found": output.found,
            "request_id": request_id.clone(),
            "actor_ip": actor_ip.clone(),
        }),
    );
    Json(ApiResponse {
        code: 0,
        message: "ok".to_string(),
        data: output,
    })
    .into_response()
}
