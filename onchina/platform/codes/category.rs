//! 定义机构展示分类及其数据库和接口表示。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum InstitutionCategory {
    /// 公权机构 tab 桶(公法人类)。
    GovInstitution,
    /// 私权机构 tab 桶(私法人类;非法人最终按父级属性分流)。
    PrivateInstitution,
}
