use serde::Deserialize;
use serde::Serialize;
use std::fs;
use std::io::ErrorKind;
use std::path::PathBuf;
use tauri::AppHandle;

const NODE_MODE_FILE_NAME: &str = "node-mode.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeMode {
    Archive,
    Normal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeModeStatus {
    Active,
    Pending,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeModeOption {
    pub mode: NodeMode,
    pub label: &'static str,
    pub implementation_status: NodeModeStatus,
    pub enabled: bool,
    pub description: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeModeState {
    pub selected_mode: NodeMode,
    pub effective_mode: NodeMode,
    pub options: Vec<NodeModeOption>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredNodeMode {
    selected_mode: String,
}

impl NodeMode {
    fn from_wire_value(value: &str) -> Result<Self, String> {
        match value.trim() {
            "archive" => Ok(Self::Archive),
            "normal" => Ok(Self::Normal),
            other => Err(format!("全节点模式无效：{other}。可选值为 archive、normal")),
        }
    }

    fn enabled(self) -> bool {
        matches!(self, Self::Archive)
    }

    fn ensure_enabled(self) -> Result<Self, String> {
        if self.enabled() {
            return Ok(self);
        }
        Err("该全节点模式尚未完成，当前不能选择".to_string())
    }
}

fn node_mode_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(nodedesktop::security::app_data_dir(app)?.join(NODE_MODE_FILE_NAME))
}

fn load_selected_node_mode(app: &AppHandle) -> Result<NodeMode, String> {
    let path = node_mode_path(app)?;
    let raw = match fs::read_to_string(&path) {
        Ok(value) => value,
        Err(err) if err.kind() == ErrorKind::NotFound => return Ok(crate::mode::NodeMode::Archive),
        Err(err) => return Err(format!("read node mode failed: {err}")),
    };
    let stored: StoredNodeMode =
        serde_json::from_str(&raw).map_err(|err| format!("parse node mode failed: {err}"))?;
    parse_stored_node_mode(&stored.selected_mode)
}

fn save_selected_node_mode(app: &AppHandle, selected_mode: NodeMode) -> Result<(), String> {
    let raw = serde_json::to_string_pretty(&StoredNodeMode {
        selected_mode: selected_mode.wire_value().to_string(),
    })
    .map_err(|err| format!("encode node mode failed: {err}"))?;
    nodedesktop::security::write_text_atomic(&node_mode_path(app)?, &format!("{raw}\n"))
        .map_err(|err| format!("write node mode failed: {err}"))
}

fn build_node_mode_state(selected_mode: NodeMode) -> NodeModeState {
    NodeModeState {
        selected_mode: if selected_mode.enabled() {
            selected_mode
        } else {
            crate::mode::NodeMode::Archive
        },
        // 全节点模式只描述链数据保存方式；聊天投递不再由区块链节点承载。
        effective_mode: if selected_mode.enabled() {
            selected_mode
        } else {
            crate::mode::NodeMode::Archive
        },
        options: node_mode_options(),
    }
}

impl NodeMode {
    fn wire_value(self) -> &'static str {
        match self {
            Self::Archive => "archive",
            Self::Normal => "normal",
        }
    }
}

fn parse_stored_node_mode(value: &str) -> Result<NodeMode, String> {
    match value.trim() {
        // 清理上一版错误保存的 communication 模式；通信能力现在是独立开关。
        "communication" => Ok(crate::mode::NodeMode::Archive),
        other => crate::mode::NodeMode::from_wire_value(other),
    }
}

fn node_mode_options() -> Vec<NodeModeOption> {
    vec![
        NodeModeOption {
            mode: crate::mode::NodeMode::Archive,
            label: "归档全节点",
            implementation_status: crate::mode::NodeModeStatus::Active,
            enabled: true,
            description: "保存完整链数据，当前版本实际按此模式运行。",
        },
        NodeModeOption {
            mode: crate::mode::NodeMode::Normal,
            label: "普通全节点",
            implementation_status: crate::mode::NodeModeStatus::Pending,
            enabled: false,
            description: "剪裁历史数据的全节点模式，功能后续完成。",
        },
    ]
}

#[tauri::command]
pub fn get_node_mode(app: AppHandle) -> Result<NodeModeState, String> {
    let selected_mode = load_selected_node_mode(&app)?;
    Ok(build_node_mode_state(selected_mode))
}

#[tauri::command]
pub fn set_node_mode(app: AppHandle, mode: String) -> Result<NodeModeState, String> {
    if let Err(err) = nodedesktop::security::append_audit_log(&app, "set_node_mode", "attempt") {
        eprintln!("[审计] set_node_mode attempt 日志写入失败: {err}");
    }

    let selected_mode = crate::mode::NodeMode::from_wire_value(&mode)?.ensure_enabled()?;
    save_selected_node_mode(&app, selected_mode)?;

    if let Err(err) = nodedesktop::security::append_audit_log(&app, "set_node_mode", "success") {
        eprintln!("[审计] set_node_mode success 日志写入失败: {err}");
    }
    Ok(build_node_mode_state(selected_mode))
}
#[cfg(test)]
#[path = "../tests/mode.rs"]
mod tests;
