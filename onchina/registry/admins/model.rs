//! 注册局名册与办理请求 DTO，字段与序列化保持不变。
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CityRegistryAdminRow {
    pub(crate) id: u64,
    pub(crate) account_id: String,
    pub(crate) family_name: String,
    pub(crate) given_name: String,
    /// 链上 finalized free 余额(分);查询失败或账户不存在时为空。
    pub(crate) balance_fen: Option<String>,
    pub(crate) institution_code: String,
    pub(crate) built_in: bool,
    pub(crate) creator_account_id: String,
    pub(crate) creator_family_name: String,
    pub(crate) creator_given_name: String,
    pub(crate) created_at: DateTime<Utc>,
    pub(crate) city_name: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CityRegistryAdminListOutput {
    pub(crate) total: usize,
    pub(crate) limit: usize,
    pub(crate) offset: usize,
    pub(crate) rows: Vec<CityRegistryAdminRow>,
}

// 联邦注册局管理员对外行(API 序列化)。
// 管理员只有存在/删除,不存在停用状态。
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FederalRegistryAdminRow {
    pub(crate) id: u64,
    pub(crate) province_name: String,
    pub(crate) account_id: String,
    pub(crate) family_name: String,
    pub(crate) given_name: String,
    pub(crate) role_code: String,
    pub(crate) role_name: String,
    pub(crate) term_required: bool,
    pub(crate) term_start: u32,
    pub(crate) term_end: u32,
    pub(crate) assignment_source: u8,
    pub(crate) assignment_source_label: String,
    pub(crate) assignment_source_ref: String,
    /// 链上 finalized free 余额(分);查询失败或账户不存在时为空。
    pub(crate) balance_fen: Option<String>,
    pub(crate) built_in: bool,
    pub(crate) created_at: DateTime<Utc>,
    /// 最近一次更新时间，None 表示从未更新
    #[serde(default)]
    pub(crate) updated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CreateCityRegistryAdminInput {
    pub(crate) account_id: String,
    pub(crate) family_name: String,
    pub(crate) given_name: String,
    /// CityRegistry 所属的市，必填，且必须属于 creator_account_id 对应联邦注册局管理员的省份（不可为省辖市）
    pub(crate) city_name: String,
    /// 可选：指定该 city_registry 归属的联邦注册局管理员账户。
    /// FederalRegistry 调用时若指定则必须等于自己账户，否则 403。
    /// 不指定则默认为调用者自身。
    #[serde(default)]
    pub(crate) creator_account_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ListQuery {
    pub(crate) limit: Option<usize>,
    pub(crate) offset: Option<usize>,
}
