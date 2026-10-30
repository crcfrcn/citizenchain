use nodeclient::network::LocalNetworkReader;

/// 仅给 Rust 内部调用者返回固定查询入口，释放 AppState 锁后再执行查询。
pub fn local_network_reader(app: &tauri::AppHandle) -> Result<LocalNetworkReader, String> {
    use tauri::Manager;
    let state = app.state::<crate::state::AppState>();
    let state = state.0.lock().map_err(|_| "节点运行状态锁异常")?;
    state
        .node_handle
        .as_ref()
        .filter(|node| node.is_alive())
        .map(nodeclient::handle::NodeHandle::network_reader)
        .ok_or_else(|| "本机节点未运行".into())
}
