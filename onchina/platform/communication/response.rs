//! HTTP API 通用响应 / 错误 / 健康检查输出包装。

use serde::Serialize;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiResponse<T: Serialize> {
    pub code: u32,
    pub message: String,
    pub data: T,
}

#[derive(Serialize)]
// CitizenApp 与管理端共用的既有分页合同保持 snake_case。
pub struct PageResult<T: Serialize> {
    pub items: Vec<T>,
    pub page_size: usize,
    pub next_cursor: Option<String>,
    pub has_more: bool,
    /// 目录型列表使用的版本游标。普通分页接口保持 None,序列化时省略。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manifest_version: Option<String>,
    /// 目录型列表使用的状态标记。OK 表示当前响应来自已同步/已校验投影。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub catalog_status: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ApiError {
    pub(crate) code: u32,
    /// 稳定业务错误码给前端判断逻辑使用;message 只给用户展示。
    pub(crate) error_code: &'static str,
    pub(crate) message: String,
    pub(crate) trace_id: String,
}

#[path = "health.rs"]
mod health;
pub(crate) use health::*;

/// 提案 ID 和年内序号在 JSON 边界使用十进制字符串，避免 JavaScript 丢失精度。
pub fn serialize_decimal<S: serde::Serializer, T: core::fmt::Display>(value: &T, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.collect_str(value)
}

/// 十进制字符串解析为目标无符号整数；拒绝越界。
pub fn deserialize_decimal<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where D: serde::Deserializer<'de>, T: core::str::FromStr, T::Err: core::fmt::Display {
    let text = <String as serde::Deserialize>::deserialize(deserializer)?;
    text.parse().map_err(serde::de::Error::custom)
}
