//! 公权机构目录 —— CitizenApp 公民端匿名只读 BFF。
//!
//! 数据来自 OnChina 本地链投影(subjects + gov + accounts),唯一真源是链上
//! `PublicManage::Institutions` / `PublicManage::InstitutionAccounts`。
//! 本层只做:① 去鉴权 / 去 scope 锁(公民可查任意省/市);② 公开 DTO 白名单(丢弃
//! 管理员/运营/PII 字段);③ custom_account_names 批量补。
//!
//! ### 量级与混合模式
//! 创世目录覆盖国家/省/市公权机构;镇级和后续新增机构由注册局运行期注册上链。
//! 客户端走「发布期创世数据包打底 + 在线增量」混合:
//! - **keyset 翻页**(`after_cid`,`WHERE cid_number > $after`):恒定快,避免 OFFSET
//!   深翻 O(n²),供生成器高效全量导出。
//! - **增量同步**(`since_version`):客户端版本等于当前链投影版本时返回空窗口;
//!   版本不同则按 keyset 窗口刷新。
//! - **真实版本号**:`chain_projection_state(public-gov)` 投影版本。链上新增、删除、
//!   账户变化或重同步都会推进版本,客户端据此刷新。
//!
//! 路由(挂 app_routes,非 admin):
//!   GET /api/app/public-institutions?province_name=&city_name=&since_version=&after_cid=&page_size=
//!   GET /api/app/public-institutions/version?province_name=&city_name=

use std::collections::HashMap;

use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::cid::china::{city_code_by_name, province_code_by_name};
use crate::cid::InstitutionCategory;
use crate::core::response::{ApiResponse, PageResult};
use crate::domains::gov::service::{
    current_chain_projection_snapshot, current_chain_projection_version,
};
use crate::*;

/// 公民端完整公权目录过滤。
/// CitizenApp 公民端“公权机构”必须显示:
/// ① 链上 PublicManage 投影出的公权机构;
/// ② 上级为链上公法人的非法人。
/// 参数 $1=province_code、$2=city_code。
const GOV_FROM_WHERE: &str = "
    FROM subjects s
    LEFT JOIN gov g ON g.province_code = s.province_code AND g.cid_number = s.cid_number
    LEFT JOIN subjects par ON par.cid_number = s.parent_cid_number
    LEFT JOIN gov pg ON pg.province_code = par.province_code AND pg.cid_number = par.cid_number
    WHERE s.kind IN ('PUBLIC', 'PRIVATE')
      AND (
            (s.kind = 'PUBLIC'
             AND g.source = 'CHAIN'
             AND s.category = 'GOV_INSTITUTION')
            OR (s.institution_code IN ('SFGT', 'SFGP', 'UNIN')
                AND par.kind = 'PUBLIC'
                AND par.category = 'GOV_INSTITUTION'
                AND pg.source = 'CHAIN')
          )
      AND s.province_code = $1
      AND ($2::text IS NULL OR s.city_code = $2)
";

fn parse_category(value: &str) -> InstitutionCategory {
    match value {
        "GOV_INSTITUTION" => InstitutionCategory::GovInstitution,
        _ => InstitutionCategory::PrivateInstitution,
    }
}

/// 公权机构目录公开行(白名单 DTO)。
///
/// 安全红线：显式不含创建管理员姓、名、角色，也不含 private_type / partnership_kind。
/// 新增字段前必须确认其可公开。
/// 已确认可公开:法定代表人的姓、名、CID 和账户 ID属于机构公开信息；无任免时不下发。
#[derive(Debug, Serialize)]
pub(crate) struct PublicInstitutionRow {
    pub cid_number: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cid_full_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cid_short_name: Option<String>,
    pub category: InstitutionCategory,
    pub p1: String,
    /// 行政区**唯一真源键**:省/市/镇 code(= subjects province_code/city_code/town_code)。
    /// 名字一律由客户端按 (province_code,city_code,town_code) 查行政区字典(china.sqlite 派生)得到,
    /// **本接口不下发任何行政区名字**(单一真源:名字别处零独立副本)。
    pub province_code: String,
    pub city_code: String,
    #[serde(default)]
    pub town_code: String,
    pub institution_code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_legal_personality: Option<bool>,
    /// 法定代表人公开身份；人的姓名只下发分离的姓、名。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub legal_representative: Option<crate::institution::subjects::model::LegalRepresentative>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_cid_number: Option<String>,
    pub account_count: usize,
    /// 自定义账户名(op_tag=0x07)。主/费可本地派生不带;绝大多数机构为空。
    pub custom_account_names: Vec<String>,
    pub created_at: DateTime<Utc>,
}

impl PublicInstitutionRow {
    /// 从目录查询行映射公开行(只取白名单列;custom_account_names 后续批量补)。
    /// **按列名取**(非裸位置索引):SELECT 增删列不会错位/panic。
    fn from_pg_row(row: &postgres::Row) -> Result<Self, String> {
        let account_count = row.get::<_, i64>("account_count").max(0) as usize;
        let legal_representative = match (
            row.get::<_, Option<String>>("family_name"),
            row.get::<_, Option<String>>("given_name"),
            row.get::<_, Option<String>>("legal_representative_cid_number"),
            row.get::<_, Option<String>>("legal_representative_account_id"),
        ) {
            (Some(family_name), Some(given_name), Some(cid_number), Some(account_id)) => {
                Some(crate::institution::subjects::model::LegalRepresentative {
                    family_name,
                    given_name,
                    cid_number,
                    account_id,
                })
            }
            (None, None, None, None) => None,
            _ => return Err("incomplete public legal representative projection".to_string()),
        };
        Ok(Self {
            cid_number: row.get("cid_number"),
            cid_full_name: row.get("cid_full_name"),
            cid_short_name: row.get("cid_short_name"),
            category: parse_category(row.get::<_, String>("category").as_str()),
            p1: row.get("p1"),
            province_code: row.get("province_code"),
            city_code: row.get("city_code"),
            town_code: row.get("town_code"),
            institution_code: row.get("institution_code"),
            parent_cid_number: row.get("parent_cid_number"),
            has_legal_personality: row.get("has_legal_personality"),
            legal_representative,
            account_count,
            custom_account_names: Vec::new(),
            created_at: row.get("created_at"),
        })
    }
}

#[derive(Debug, serde::Deserialize)]
pub(crate) struct PublicInstitutionListQuery {
    pub province_name: Option<String>,
    pub city_name: Option<String>,
    /// 增量游标:客户端保存的链投影版本。当前实现收到旧版本时仍走 keyset 返回全量窗口。
    pub since_version: Option<String>,
    /// keyset 翻页游标:仅回 cid_number 严格大于此值的行。
    pub after_cid: Option<String>,
    pub page_size: Option<usize>,
}

/// GET /api/app/public-institutions —— 匿名公权机构目录(keyset 翻页 + 可选增量)。
pub(crate) async fn list_public_institutions(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<PublicInstitutionListQuery>,
) -> impl IntoResponse {
    let Some(province) = query
        .province_name
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    else {
        return api_error(StatusCode::BAD_REQUEST, 1001, "province required");
    };
    let Some(province_code) = province_code_by_name(province) else {
        return api_error(StatusCode::BAD_REQUEST, 1001, "unknown province");
    };
    let city = query
        .city_name
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let city_code = match city {
        Some(name) => match city_code_by_name(province, name) {
            Some(code) => Some(code),
            None => return api_error(StatusCode::BAD_REQUEST, 1001, "unknown city"),
        },
        None => None,
    };
    let page_size = query.page_size.unwrap_or(300).clamp(1, 500);
    let after_cid = query
        .after_cid
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let since_version = query
        .since_version
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);

    let (mut items, has_more, next_cursor) = match query_public_institutions(
        &state,
        province_code,
        city_code,
        after_cid,
        since_version,
        page_size,
    ) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(error = %e, "public institution list failed");
            return api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                1004,
                "public institution query failed",
            );
        }
    };

    // custom_account_names 批量补(不动主查询)。
    let cid_numbers: Vec<String> = items.iter().map(|r| r.cid_number.clone()).collect();
    let custom_map =
        custom_account_names_for(&state, province_code, &cid_numbers).unwrap_or_else(|e| {
            tracing::warn!(error = %e, "custom account names query failed; default empty");
            HashMap::new()
        });
    for row in items.iter_mut() {
        if let Some(names) = custom_map.get(&row.cid_number) {
            row.custom_account_names = names.clone();
        }
    }

    let manifest_version = scope_version(&state, province_code, city_code);

    let data = PageResult::<PublicInstitutionRow> {
        items,
        page_size,
        next_cursor,
        has_more,
        manifest_version,
        catalog_status: Some("OK".to_string()),
    };
    Json(ApiResponse {
        code: 0,
        message: "ok".to_string(),
        data,
    })
    .into_response()
}

#[derive(Debug, serde::Deserialize)]
pub(crate) struct PublicInstitutionVersionQuery {
    pub province_name: Option<String>,
    pub city_name: Option<String>,
}

#[derive(Debug, Serialize)]
struct PublicInstitutionVersion {
    province_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    city_name: Option<String>,
    /// 目录版本 = `chain_projection_state(public-gov)` 投影版本;无投影时 null。
    #[serde(skip_serializing_if = "Option::is_none")]
    manifest_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    chain_genesis_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    chain_block_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    chain_block_number: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    synced_at: Option<String>,
    /// 机构总数(供客户端粗判增删,删除兜底)。
    count: i64,
}

/// GET /api/app/public-institutions/version —— 某省/市目录版本(增量比对用)。
pub(crate) async fn public_institutions_version(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<PublicInstitutionVersionQuery>,
) -> impl IntoResponse {
    let Some(province) = query
        .province_name
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    else {
        return api_error(StatusCode::BAD_REQUEST, 1001, "province required");
    };
    let Some(province_code) = province_code_by_name(province) else {
        return api_error(StatusCode::BAD_REQUEST, 1001, "unknown province");
    };
    let city = query
        .city_name
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let city_code = match city {
        Some(name) => match city_code_by_name(province, name) {
            Some(code) => Some(code),
            None => return api_error(StatusCode::BAD_REQUEST, 1001, "unknown city"),
        },
        None => None,
    };
    let (version, count) = match scope_version_and_count(&state, province_code, city_code) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(error = %e, "public institutions version failed");
            return api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                1004,
                "public institutions version query failed",
            );
        }
    };
    let projection = current_chain_projection_snapshot(&state.db);
    let data = PublicInstitutionVersion {
        province_name: province.to_string(),
        city_name: city.map(str::to_string),
        manifest_version: version,
        chain_genesis_hash: projection.as_ref().map(|v| v.chain_genesis_hash.clone()),
        chain_block_hash: projection.as_ref().map(|v| v.chain_block_hash.clone()),
        chain_block_number: projection.as_ref().and_then(|v| v.chain_block_number),
        synced_at: projection.map(|v| v.synced_at),
        count,
    };
    Json(ApiResponse {
        code: 0,
        message: "ok".to_string(),
        data,
    })
    .into_response()
}

/// keyset 翻页 + 可选 since 增量查询。返回 (rows, has_more, next_cursor)。
fn query_public_institutions(
    state: &AppState,
    province_code: &str,
    city_code: Option<&str>,
    after_cid: Option<String>,
    since_version: Option<String>,
    page_size: usize,
) -> Result<(Vec<PublicInstitutionRow>, bool, Option<String>), String> {
    let province_code = province_code.to_string();
    let city_code = city_code.map(str::to_string);
    let limit = i64::try_from(page_size.saturating_add(1))
        .map_err(|_| "page_size too large".to_string())?;
    let current_version = current_chain_projection_version(&state.db)
        .ok_or_else(|| "chain public institution projection is not initialized".to_string())?;
    if since_version.as_deref() == Some(current_version.as_str()) {
        return Ok((Vec::new(), false, None));
    }
    // 全列显式 AS 别名,from_pg_row 按列名取;行政区只下发 code(province_code/city_code/town_code),不吐名字。
    let sql = format!(
        "SELECT s.cid_number,
                s.cid_full_name, s.cid_short_name, s.category,
                s.p1,
                s.province_code AS province_code, s.city_code AS city_code,
                COALESCE(s.town_code, '') AS town_code,
                s.institution_code, s.parent_cid_number, s.has_legal_personality,
                (SELECT COUNT(*) FROM accounts a
                   WHERE a.province_code = s.province_code AND a.cid_number = s.cid_number) AS account_count,
	                s.created_at, s.family_name, s.given_name,
	                s.legal_representative_cid_number, s.legal_representative_account_id
	         {GOV_FROM_WHERE}
	           AND ($3::text IS NULL OR s.cid_number > $3)
	         ORDER BY s.cid_number ASC
	         LIMIT $4"
    );
    state.db.with_client(move |conn| {
        let rows = conn
            .query(
                sql.as_str(),
                &[&province_code, &city_code, &after_cid, &limit],
            )
            .map_err(|e| format!("public institution keyset query failed: {e}"))?;
        let mut items: Vec<PublicInstitutionRow> = rows
            .iter()
            .map(PublicInstitutionRow::from_pg_row)
            .collect::<Result<_, _>>()?;
        let has_more = items.len() > page_size;
        if has_more {
            items.truncate(page_size);
        }
        let next_cursor = if has_more {
            items.last().map(|r| r.cid_number.clone())
        } else {
            None
        };
        Ok((items, has_more, next_cursor))
    })
}

/// 目录版本 = 链上公权投影版本(单查,list 用)。
fn scope_version(state: &AppState, province_code: &str, city_code: Option<&str>) -> Option<String> {
    scope_version_and_count(state, province_code, city_code)
        .ok()
        .and_then(|(v, _)| v)
}

/// 目录版本 + 机构总数:链投影版本 + 当前 scope COUNT。
fn scope_version_and_count(
    state: &AppState,
    province_code: &str,
    city_code: Option<&str>,
) -> Result<(Option<String>, i64), String> {
    let province_code = province_code.to_string();
    let city_code = city_code.map(str::to_string);
    let projection_version = current_chain_projection_version(&state.db);
    let sql = format!("SELECT COUNT(*) {GOV_FROM_WHERE}");
    state.db.with_client(move |conn| {
        let row = conn
            .query_one(sql.as_str(), &[&province_code, &city_code])
            .map_err(|e| format!("public institution version query failed: {e}"))?;
        let count: i64 = row.get(0);
        Ok((projection_version, count))
    })
}

/// 批量查机构自定义账户名(op_tag=0x07,即非 6 保留名)。空列表短路。
fn custom_account_names_for(
    state: &AppState,
    province_code: &str,
    cid_numbers: &[String],
) -> Result<HashMap<String, Vec<String>>, String> {
    if cid_numbers.is_empty() {
        return Ok(HashMap::new());
    }
    let province_code = province_code.to_string();
    let cids: Vec<String> = cid_numbers.to_vec();
    let reserved: Vec<String> =
        crate::institution::accounts::derive::reserved_account_names().to_vec();
    state.db.with_client(move |conn| {
        let rows = conn
            .query(
                "SELECT cid_number, account_name
                 FROM accounts
                 WHERE province_code = $1
                   AND cid_number = ANY($2)
                   AND account_name <> ALL($3)
                 ORDER BY cid_number ASC, account_name ASC",
                &[&province_code, &cids, &reserved],
            )
            .map_err(|e| format!("custom account names query failed: {e}"))?;
        let mut map: HashMap<String, Vec<String>> = HashMap::new();
        for row in rows {
            let cid: String = row.get(0);
            let name: String = row.get(1);
            map.entry(cid).or_default().push(name);
        }
        Ok(map)
    })
}

#[cfg(test)]
// 公共机构映射夹具异常应直接中止测试，断言式解包不进入生产路径。
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    fn sample_row() -> PublicInstitutionRow {
        PublicInstitutionRow {
            cid_number: "AH001-PGV0C-123456789-2026".to_string(),
            cid_full_name: Some("安徽省人民政府".to_string()),
            cid_short_name: Some("皖府".to_string()),
            category: InstitutionCategory::GovInstitution,
            p1: "0".to_string(),
            province_code: "AH".to_string(),
            city_code: "001".to_string(),
            town_code: String::new(),
            institution_code: "PGV".to_string(),
            has_legal_personality: Some(true),
            legal_representative: Some(crate::institution::subjects::model::LegalRepresentative {
                family_name: "张".to_string(),
                given_name: "三".to_string(),
                cid_number: "110000CTZN1000000001".to_string(),
                account_id: "11".repeat(32),
            }),
            parent_cid_number: None,
            account_count: 2,
            custom_account_names: vec!["业务专户A".to_string()],
            created_at: Utc::now(),
        }
    }

    #[test]
    fn public_dto_excludes_sensitive_admin_fields() {
        let json = serde_json::to_string(&sample_row()).expect("serialize public row");
        for forbidden in ["creator_account_id", "private_type", "partnership_kind"] {
            assert!(
                !json.contains(forbidden),
                "公开 DTO 泄露敏感字段: {forbidden}"
            );
        }
        assert!(json.contains("custom_account_names"));
        assert!(json.contains("业务专户A"));
        assert!(json.contains("安徽省人民政府"));
        // 法定代表人姓名属已确认可公开的目录字段,必须随公开行下发。
        assert!(json.contains("legal_representative"));
        assert!(json.contains("family_name"));
        assert!(json.contains("given_name"));
        assert!(json.contains("张"));
        assert!(json.contains("三"));
    }

    #[test]
    fn public_dto_keeps_directory_fields() {
        let row = sample_row();
        assert_eq!(row.cid_number, "AH001-PGV0C-123456789-2026");
        assert_eq!(row.account_count, 2);
        assert_eq!(row.institution_code, "PGV");
    }

    #[test]
    fn citizen_public_filter_keeps_all_public_institutions() {
        assert!(GOV_FROM_WHERE.contains("s.category = 'GOV_INSTITUTION'"));
        assert!(GOV_FROM_WHERE.contains("s.institution_code IN ('SFGT', 'SFGP', 'UNIN')"));
        assert!(GOV_FROM_WHERE.contains("par.category = 'GOV_INSTITUTION'"));
        assert!(!GOV_FROM_WHERE.contains("CITY_POLICE"));
        // 公民端公权视图不排除教育机构(教育也在此展示)。
        assert!(!GOV_FROM_WHERE.contains("institution_code NOT IN ('NED'"));
    }
}
