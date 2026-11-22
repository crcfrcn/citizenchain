//! 定义以机构CID和名册锚点定位的注册局管理员响应模型。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CityRegistryAdminRow {
    pub(crate) id: String,
    pub(crate) institution_cid_number: String,
    pub(crate) roster_account_id: String,
    pub(crate) citizen_cid_number: String,
    pub(crate) role_name: String,
    pub(crate) account_id: String,
    pub(crate) family_name: String,
    pub(crate) given_name: String,
    /// 链上 finalized free 余额(分);查询失败或账户不存在时为空。
    pub(crate) balance_fen: Option<String>,
    pub(crate) institution_code: String,
    pub(crate) creator_account_id: String,
    pub(crate) creator_family_name: String,
    pub(crate) creator_given_name: String,
    pub(crate) created_at: Option<DateTime<Utc>>,
    pub(crate) city_name: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CityRegistryAdminListOutput {
    pub(crate) total: usize,
    pub(crate) limit: usize,
    pub(crate) offset: usize,
    pub(crate) rows: Vec<CityRegistryAdminRow>,
    pub(crate) cid_number: String,
    pub(crate) finalized_block_hash: String,
    pub(crate) finalized_block_number: u32,
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

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct ListQuery {
    /// 只取当前详情页目标，不从城市名称或本地管理员行猜测。
    pub(crate) cid_number: String,
    pub(crate) limit: Option<usize>,
    pub(crate) offset: Option<usize>,
}
