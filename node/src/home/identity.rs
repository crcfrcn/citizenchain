// 身份管理子模块：节点身份信息、状态查询。

use crate::{desktop::node_runner::NodeHandle, settings::bootnodes_address};
use serde::Serialize;
use serde_json::Value;
use tauri::AppHandle;

use super::process::{AppState, NodeLifecycleState};
use super::rpc::rpc_post;
use tauri::Manager;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
/// 首页展示的节点运行状态。
pub struct NodeStatus {
    pub running: bool,
    pub state: String,
    pub pid: Option<u32>,
    pub last_error: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
/// 首页展示的节点身份信息。
pub struct NodeIdentity {
    pub peer_id: Option<String>,
    pub role: Option<String>,
    /// 本节点启动链的创世哈希，统一取自本机 RPC 的 block #0 哈希缓存。
    pub genesis_hash: Option<String>,
}

// 角色由 bootnode 对应机构的 `cid_short_name` 映射得出，未命中时统一按“全节点”展示。
fn role_from_peer_id(peer_id: Option<&str>) -> String {
    if let Some(pid) = peer_id {
        if let Ok(Some(cid_short_name)) =
            bootnodes_address::find_genesis_bootnode_cid_short_name_by_peer_id(pid)
        {
            return cid_short_name;
        }
    }
    "全节点".to_string()
}

pub(crate) fn current_status(app: &AppHandle) -> Result<NodeStatus, String> {
    let stale_handle = {
        let app_state = app.state::<AppState>();
        let mut state = app_state
            .0
            .lock()
            .map_err(|_| "acquire process state failed".to_string())?;
        let stale = state
            .node_handle
            .as_ref()
            .map(|handle| !handle.is_alive())
            .unwrap_or(false);
        if stale {
            state.node_state = NodeLifecycleState::Exited;
            state.last_error = Some(
                state
                    .node_handle
                    .as_ref()
                    .and_then(NodeHandle::take_exit_error)
                    .unwrap_or_else(|| "节点线程异常退出，但未返回退出详情".to_string()),
            );
            state.node_handle.take()
        } else {
            if state.node_handle.is_some() {
                // 线程存活不代表 RPC 已可用。保留 starting / initializing /
                // restarting 等中间态,只修正“不一致的 stopped + handle 存在”状态。
                if state.node_state == NodeLifecycleState::Stopped {
                    state.node_state = NodeLifecycleState::Running;
                    state.last_error = None;
                }
            }
            None
        }
    };
    // 异常退出的线程句柄在释放 state 锁后 drop，避免状态查询阻塞其它命令。
    drop(stale_handle);

    let (managed_running, state_label, last_error) = {
        let app_state = app.state::<AppState>();
        let state = app_state
            .0
            .lock()
            .map_err(|_| "acquire process state failed".to_string())?;
        let running = state
            .node_handle
            .as_ref()
            .map(|handle| handle.is_alive() && state.node_state == NodeLifecycleState::Running)
            .unwrap_or(false);
        (
            running,
            state.node_state.as_str().to_string(),
            state.last_error.clone(),
        )
    };
    let managed_pid: Option<u32> = None;
    Ok(NodeStatus {
        running: managed_running,
        state: state_label,
        pid: managed_pid,
        last_error,
    })
}

fn get_node_status_sync(app: AppHandle) -> Result<NodeStatus, String> {
    current_status(&app)
}

fn get_node_identity_sync(app: AppHandle) -> Result<NodeIdentity, String> {
    if !current_status(&app)?.running {
        return Ok(NodeIdentity {
            peer_id: None,
            role: Some("全节点".to_string()),
            genesis_hash: None,
        });
    }

    // 节点进入 Running 前已经完成 block #0 RPC 就绪校验；这里复用同一缓存，
    // 不硬编码链指纹，也不为首页三秒刷新重复请求 block #0。
    let genesis_hash = crate::shared::rpc::cached_genesis_hash()?;
    let local_peer_id = rpc_post("system_localPeerId", Value::Array(vec![]))
        .ok()
        .and_then(|v| v.as_str().map(|s| s.to_string()));
    let role = role_from_peer_id(local_peer_id.as_deref());

    Ok(NodeIdentity {
        peer_id: local_peer_id,
        role: Some(role),
        genesis_hash: Some(genesis_hash),
    })
}

pub(crate) fn get_node_identity_blocking(app: AppHandle) -> Result<NodeIdentity, String> {
    get_node_identity_sync(app)
}

#[tauri::command]
pub async fn get_node_status(app: AppHandle) -> Result<NodeStatus, String> {
    super::join_blocking_task(
        "get_node_status",
        tauri::async_runtime::spawn_blocking(move || get_node_status_sync(app)),
    )
    .await
}

#[tauri::command]
pub async fn get_node_identity(app: AppHandle) -> Result<NodeIdentity, String> {
    super::join_blocking_task(
        "get_node_identity",
        tauri::async_runtime::spawn_blocking(move || get_node_identity_sync(app)),
    )
    .await
}
