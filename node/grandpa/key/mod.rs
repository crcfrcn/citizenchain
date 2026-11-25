//! 组织节点GRANDPA密钥轮换、持钥证明和待生效状态。

pub mod commands;
pub mod local;
pub mod proof;
pub mod request;
pub mod rotation;
pub mod settings;
pub mod storage;
pub mod types;

use tauri::{AppHandle, Manager};

/// 主包接入原有密码检查、运行状态和启停；密钥模块不反向依赖节点控制包。
#[derive(Clone, Copy)]
pub struct Host {
    /// 沿用设备登录密码验证。
    pub verify_password: fn(&AppHandle, &str) -> Result<(), String>,
    /// 沿用节点当前运行状态。
    pub is_running: fn(&AppHandle) -> Result<bool, String>,
    /// 沿用节点同步启动。
    pub start: fn(AppHandle) -> Result<(), String>,
    /// 沿用节点同步停止。
    pub stop: fn(AppHandle) -> Result<(), String>,
}

impl Host {
    pub(crate) fn get(app: &AppHandle) -> Result<Self, String> {
        app.try_state::<Self>()
            .map(|host| *host)
            .ok_or_else(|| "GRANDPA密钥宿主未接入".to_string())
    }
}
