// ALL_CODES 与两张文本表同属 primitives 单源；缺项属于构建期不变量破坏，必须立即失败。
#![allow(clippy::expect_used)]

use axum::{extract::State, http::HeaderMap, response::IntoResponse, Json};

use crate::cid::china::provinces;
use crate::cid::code as institution_code;
use crate::*;

// 本文件保留 CID 编码元信息接口:管理端 meta(含行政区,需登录) + 公开机构码标签(免登录单源下发)。
// 城市列表由 cid::china::admin 提供。

/// 机构码→中文标签全集(primitives code.rs 单源,ALL_CODES = 104 码)。
/// admin_cid_meta 与 public_cid_labels 共用,保证前后端标签同源。
fn institution_code_items() -> Vec<CidInstitutionCodeItem> {
    institution_code::ALL_CODES
        .iter()
        .map(|c| CidInstitutionCodeItem {
            institution_code: institution_code::institution_code_text(c)
                .expect("known CID institution code"),
            institution_code_label: institution_code::institution_code_label(c)
                .expect("known CID institution code"),
        })
        .collect()
}

pub(crate) async fn admin_cid_meta(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let admin_ctx = match require_admin_any(&state, &headers) {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let scoped = admin_ctx.scope_province_name.clone();
    let mut all_provinces: Vec<CidProvinceItem> = provinces()
        .iter()
        .map(|p| CidProvinceItem {
            province_name: p.province_name.to_string(),
            province_code: p.province_code.to_string(),
        })
        .collect();
    let mut provinces_rows: Vec<CidProvinceItem> = provinces()
        .iter()
        .filter(|p| {
            scoped
                .as_deref()
                .map(|v| v == p.province_name)
                .unwrap_or(true)
        })
        .map(|p| CidProvinceItem {
            province_name: p.province_name.to_string(),
            province_code: p.province_code.to_string(),
        })
        .collect();
    provinces_rows.sort_by(|a, b| a.province_code.cmp(&b.province_code));
    all_provinces.sort_by(|a, b| a.province_code.cmp(&b.province_code));
    Json(ApiResponse {
        code: 0,
        message: "ok".to_string(),
        data: AdminCidMetaOutput {
            // 机构码选项由 primitives code.rs 单源派生(104 码,与 public_cid_labels 同源);
            // 机构类别由机构码派生,不单列。
            institution_options: institution_code_items(),
            provinces: provinces_rows,
            all_provinces,
            scoped_province_name: scoped,
        },
    })
    .into_response()
}

/// 公开机构码标签表(免登录):前端 `useInstitutionCodeLabels` 拉取,替代原硬编码 INSTITUTION_CODE_LABEL。
/// 数据即 primitives code.rs 单源,内容与已编译前端旧硬编码等价(且更全),公开无敏感性。
pub(crate) async fn public_cid_labels() -> impl IntoResponse {
    Json(ApiResponse {
        code: 0,
        message: "ok".to_string(),
        data: CidLabelsOutput {
            institution_labels: institution_code_items(),
        },
    })
    .into_response()
}
