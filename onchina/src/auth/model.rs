//! 机构管理员实体 + 管理员列表与维护接口 DTO。
//!
//! 管理员按机构码(`institution_code`,3/4 字符文本)归属机构;内置初始联邦注册局管理员
//! 承担不可删除安全根职责。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct AdminUser {
    pub(crate) id: u64,
    pub(crate) account_id: String,
    pub(crate) family_name: String,
    pub(crate) given_name: String,
    /// 所属机构码(3/4 字符文本,如 FRG/CREG/NLG)。
    pub(crate) institution_code: String,
    /// 初始联邦注册局管理员由代码内置,不可删除;代码以外新增管理员为 false。
    pub(crate) built_in: bool,
    pub(crate) creator_account_id: String,
    pub(crate) created_at: DateTime<Utc>,
    #[serde(default)]
    pub(crate) updated_at: Option<DateTime<Utc>>,
    /// 市级机构所属的市名称(市级机构必填,其它机构为空字符串)。
    #[serde(default)]
    pub(crate) city_name: String,
}

#[derive(Debug, Clone, Serialize)]
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
pub(crate) struct CityRegistryAdminListOutput {
    pub(crate) total: usize,
    pub(crate) limit: usize,
    pub(crate) offset: usize,
    pub(crate) rows: Vec<CityRegistryAdminRow>,
}

// 联邦注册局管理员对外行(API 序列化)。
// 管理员只有存在/删除,不存在停用状态。
#[derive(Serialize, Clone)]
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

#[derive(Serialize)]
pub(crate) struct OwnInstitutionAdminRow {
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
    pub(crate) is_self: bool,
}

#[derive(Serialize)]
pub(crate) struct OwnInstitutionAdminListOutput {
    pub(crate) institution_code: String,
    pub(crate) cid_short_name: Option<String>,
    pub(crate) rows: Vec<OwnInstitutionAdminRow>,
}

#[derive(Debug, Clone, Deserialize)]
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
pub(crate) struct ListQuery {
    pub(crate) limit: Option<usize>,
    pub(crate) offset: Option<usize>,
}
