//! 机构展示分类 — 公权/私权 tab 桶。
//!
//! 分类值写入 DB `category` 列并序列化给前端;列表 SQL 直接按 `category` 过滤
//! (见 `subjects::model::InstitutionListFilter::sql_clause`),Rust 侧不再逐条计算分类。
//!
//! 注意:这不是法律主体分类。公法人、私法人、非法人、公民人、自然人、智能人
//! 是独立主体类型;非法人可从属于公法人或私法人,具体列表归属由 subjects 的
//! 父级属性规则分流。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum InstitutionCategory {
    /// 公权机构 tab 桶(公法人类)。
    GovInstitution,
    /// 私权机构 tab 桶(私法人类;非法人最终按父级属性分流)。
    PrivateInstitution,
}
