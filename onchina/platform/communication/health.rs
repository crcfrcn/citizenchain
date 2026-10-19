//! 平台健康响应保留原字段，追加当前可用模块与本进程软件版本。
use serde::Serialize;
#[derive(Serialize)]
pub(crate) struct HealthData {
    pub(crate) service: &'static str,
    pub(crate) status: &'static str,
    pub(crate) checked_at: i64,
    pub(crate) modules: Vec<crate::modules::ModuleAsset>,
    pub(crate) module_id: Option<String>,
    pub(crate) module_version: Option<String>,
}
