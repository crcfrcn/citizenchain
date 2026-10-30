use nodecontrol::bootnodes::find_genesis_bootnode_cid_short_name_by_peer_id;
use nodecontrol::bootnodes::is_genesis_bootnode_peer_id;
use nodecontrol::bootnodes::peer_id_from_node_key_hex;
use nodecontrol::bootnodes::save_bootnode_meta;
use nodecontrol::bootnodes::wait_peer_id_applied;
use nodecontrol::bootnodes::write_secret_ed25519;
use nodecontrol::bootnodes::BootnodeKey;
use nodedesktop::address::decode_hex_32_strict;
use nodedesktop::validation::normalize_node_key;

use tauri::AppHandle;

#[tauri::command]
pub(crate) fn set_bootnode_key(
    app: AppHandle,
    node_key: String,
    unlock_password: String,
) -> Result<BootnodeKey, String> {
    if let Err(e) = nodedesktop::security::append_audit_log(&app, "set_bootnode_key", "attempt") {
        eprintln!("[审计] set_bootnode_key attempt 日志写入失败: {e}");
    }
    let unlock = nodedesktop::security::ensure_unlock_password(&unlock_password)?;
    nodecontrol::password::verify_device_login_password(&app, unlock)?;
    let normalized = normalize_node_key(&node_key)?;
    let derived_peer_id = peer_id_from_node_key_hex(&normalized)?;
    if !is_genesis_bootnode_peer_id(&derived_peer_id)? {
        return Err(format!(
            "该私钥不对应任何创世引导节点（推导 Peer ID: {derived_peer_id}）"
        ));
    }
    let cid_short_name = find_genesis_bootnode_cid_short_name_by_peer_id(&derived_peer_id)?;

    let secret_bytes =
        decode_hex_32_strict(&normalized).map_err(|_| "node-key hex decode failed".to_string())?;
    write_secret_ed25519(&app, &secret_bytes)?;
    save_bootnode_meta(&app, &derived_peer_id, cid_short_name.clone())?;

    // 若节点当前在运行，保存新私钥后立即重启以应用新的 p2p 身份，
    // 并轮询确认本机 PeerId 已切换到目标引导节点。
    if nodecontrol::identity::current_status(&app)?.running {
        if let Err(err) = (|| -> Result<(), String> {
            let _ = crate::lifecycle::stop_node_blocking(app.clone())?;
            let _ = crate::lifecycle::start_node_blocking(app.clone())?;
            wait_peer_id_applied(&app, &derived_peer_id)?;
            Ok(())
        })() {
            if let Err(e) = nodedesktop::security::append_audit_log(
                &app,
                "set_bootnode_key",
                "saved_restart_failed",
            ) {
                eprintln!("[审计] set_bootnode_key saved_restart_failed 日志写入失败: {e}");
            }
            return Err(format!(
                "引导节点私钥已保存，但节点重启失败：{err}。新密钥将在下次成功启动节点时自动生效。"
            ));
        }
    }
    if let Err(e) = nodedesktop::security::append_audit_log(&app, "set_bootnode_key", "success") {
        eprintln!("[审计] set_bootnode_key success 日志写入失败: {e}");
    }

    Ok(BootnodeKey {
        node_key: None,
        peer_id: Some(derived_peer_id),
        cid_short_name,
    })
}
