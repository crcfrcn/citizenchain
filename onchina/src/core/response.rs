//! HTTP API 通用响应 / 错误 / 健康检查输出包装。

use serde::Serialize;

#[derive(Serialize)]
pub(crate) struct ApiResponse<T: Serialize> {
    pub(crate) code: u32,
    pub(crate) message: String,
    pub(crate) data: T,
}

#[derive(Serialize)]
pub(crate) struct PageResult<T: Serialize> {
    pub(crate) items: Vec<T>,
    pub(crate) page_size: usize,
    pub(crate) next_cursor: Option<String>,
    pub(crate) has_more: bool,
    /// 目录型列表使用的版本游标。普通分页接口保持 None,序列化时省略。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) manifest_version: Option<String>,
    /// 目录型列表使用的状态标记。OK 表示当前响应来自已同步/已校验投影。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) catalog_status: Option<String>,
}

#[derive(Serialize)]
pub(crate) struct ApiError {
    pub(crate) code: u32,
    /// 稳定业务错误码给前端判断逻辑使用;message 只给用户展示。
    pub(crate) error_code: &'static str,
    pub(crate) message: String,
    pub(crate) trace_id: String,
}

#[derive(Serialize)]
pub(crate) struct HealthData {
    pub(crate) service: &'static str,
    pub(crate) status: &'static str,
    pub(crate) checked_at: i64,
}
