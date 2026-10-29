//! 公民证件资料库，保持原有接口、鉴权和文件访问行为。
#![allow(clippy::result_large_err)]
use axum::{
    body::Body,
    extract::{Multipart, Path, State},
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
                    "SELECT row_id, cid_number, file_name, document_type, file_size, file_hash,
                            uploader_account, uploaded_at
                     FROM document
                     WHERE province_code = $1 AND cid_number = $2
                     ORDER BY uploaded_at DESC, row_id DESC",
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
                    "INSERT INTO document (
                        cid_number, province_code, city_code, file_name, document_type,
                        file_size, file_path, file_hash, uploader_account, uploaded_at
                     ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
                     RETURNING row_id",
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
                    "SELECT row_id, cid_number, file_name, document_type, file_size, file_path,
                            file_hash, uploader_account, uploaded_at
                     FROM document
                     WHERE province_code = $1 AND cid_number = $2 AND row_id = $3",
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
                    "DELETE FROM document WHERE province_code = $1 AND cid_number = $2 AND row_id = $3",
                    &[&province_code, &cid_number, &doc_id],
                )
                .map_err(|e| format!("delete citizen document failed: {e}"))?;
            Ok(affected > 0)
        })
    }
}

fn ensure_citizen_document_scope(
    state: &AppState,
    auth_ctx: &crate::authentication::login::AdminAuthContext,
    cid_number: &str,
) -> Result<CitizenRecord, axum::response::Response> {
    crate::authorization::rules::ensure_registry_admin(auth_ctx)?;
    let scope = crate::authorization::get_visible_scope(auth_ctx);
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
    loop {
        let field = match multipart.next_field().await {
            Ok(Some(field)) => field,
            Ok(None) => break,
            Err(error) => return api_error(error.status(), 1001, "资料上传流无效或超过请求体限制"),
        };
        let name = field.name().unwrap_or("").to_string();
        match name.as_str() {
            "file" => {
                file_name = field.file_name().map(str::to_string);
                match field.bytes().await {
                    Ok(bytes) => file_data = Some(bytes.to_vec()),
                    Err(e) => {
                        let message = format!("读取文件失败: {e}");
                        return api_error(e.status(), 1001, message.as_str());
                    }
                }
            }
            "document_type" => match field.text().await {
                Ok(text) => document_type = Some(text.trim().to_string()),
                Err(error) => return api_error(error.status(), 1001, "资料类型字段读取失败"),
            },
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
    if !crate::documents::valid_file_size(file_data.len()) {
        return api_error(StatusCode::BAD_REQUEST, 1001, "文件大小不能超过 10MB");
    }

    // 公民资料属于本地写，必须先消费一次性 Passkey 再创建文件或元数据。
    if let Err(resp) = crate::authentication::actions::require_current_admin(&state, &ctx).await {
        return resp;
    }
    if let Err(resp) =
        crate::authentication::passkey::require_passkey_assertion(&state, &headers, &ctx.account_id)
    {
        return resp;
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
        if let Err(cleanup) = std::fs::remove_file(&stored_path) {
            if cleanup.kind() != std::io::ErrorKind::NotFound {
                tracing::error!(error = %cleanup, "remove partial upload failed");
            }
        }
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
            "cidNumber": record.cid_number,
            "documentType": doc.document_type,
            "fileName": doc.file_name,
            "fileSize": doc.file_size,
            "fileHash": doc.file_hash,
            "requestId": request_id_from_headers(&headers),
            "actorIp": actor_ip_from_headers(&headers),
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
    if let Err(resp) = crate::authentication::actions::require_current_admin(&state, &ctx).await {
        return resp;
    }
    if let Err(resp) =
        crate::authentication::passkey::require_passkey_assertion(&state, &headers, &ctx.account_id)
    {
        return resp;
    }
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
            "cidNumber": record.cid_number,
            "docId": doc_id,
            "documentType": doc.meta.document_type,
            "fileName": doc.meta.file_name,
            "fileHash": doc.meta.file_hash,
            "requestId": request_id_from_headers(&headers),
            "actorIp": actor_ip_from_headers(&headers),
        }),
    );
    Json(ApiResponse {
        code: 0,
        message: "ok".to_string(),
        data: "ok",
    })
    .into_response()
}
