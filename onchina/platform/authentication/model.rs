//! 平台管理员登录元数据；名册业务 DTO 由所属功能持有。
//!
//! 管理员按准确机构 CID 归属名册，机构码仅分类；内置初始联邦注册局管理员
//! 承担不可删除安全根职责。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct AdminUser {
    pub id: u64,
    pub account_id: String,
    pub family_name: String,
    pub given_name: String,
    /// 所属机构码(3/4 字符文本,如 FRG/CREG/NLG)。
    pub institution_code: String,
    /// 名册业务归属的准确机构 CID；只作数据隔离元数据，绝不授予链上权限。
    #[serde(default)]
    pub institution_cid_number: Option<String>,
    /// 初始联邦注册局管理员由代码内置,不可删除;代码以外新增管理员为 false。
    pub built_in: bool,
    pub creator_account_id: String,
    pub created_at: DateTime<Utc>,
    #[serde(default)]
    pub updated_at: Option<DateTime<Utc>>,
    /// 市级机构所属的市名称(市级机构必填,其它机构为空字符串)。
    #[serde(default)]
    pub city_name: String,
}
