use crate::types::PendingKeyChange;

use std::fs;
use std::io::ErrorKind;

use tauri::AppHandle;

const PENDING_STATE_FILE: &str = "grandpa-key-change.json";

fn pending_state_path(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    Ok(nodedesktop::security::app_data_dir(app)?.join(PENDING_STATE_FILE))
}

pub fn load_pending(app: &AppHandle) -> Result<Option<PendingKeyChange>, String> {
    let path = pending_state_path(app)?;
    let raw = match fs::read_to_string(&path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("读取 GRANDPA 换钥状态失败: {error}")),
    };
    serde_json::from_str(&raw)
        .map(Some)
        .map_err(|error| format!("解析 GRANDPA 换钥状态失败: {error}"))
}

pub(crate) fn save_pending(app: &AppHandle, pending: &PendingKeyChange) -> Result<(), String> {
    let raw = serde_json::to_string_pretty(pending)
        .map_err(|error| format!("编码 GRANDPA 换钥状态失败: {error}"))?;
    nodedesktop::security::write_text_atomic(&pending_state_path(app)?, &format!("{raw}\n"))
        .map_err(|error| format!("保存 GRANDPA 换钥状态失败: {error}"))
}

pub fn clear_pending(app: &AppHandle) -> Result<(), String> {
    match fs::remove_file(pending_state_path(app)?) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("删除 GRANDPA 换钥状态失败: {error}")),
    }
}
