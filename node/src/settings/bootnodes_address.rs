use crate::{
    home,
    settings::{address_utils::decode_hex_32_strict, device_password, grandpa_address},
    shared::{security, validation::normalize_node_key},
};
use libp2p_identity::PeerId;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::ErrorKind,
    path::PathBuf,
    thread,
    time::{Duration, Instant},
};
use tauri::AppHandle;

const PEER_ID_WAIT_TIMEOUT: Duration = Duration::from_secs(20);
const STATUS_POLL_INTERVAL: Duration = Duration::from_millis(250);
const SUBSTRATE_SECRET_ED25519: &str = "secret_ed25519";
/// 节点身份密钥统一存放目录（不依赖链 ID，dev/正式链共用）。
const NODE_KEY_DIR: &str = "node-key";

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
/// 前端展示的引导节点私钥绑定状态。
pub struct BootnodeKey {
    pub node_key: Option<String>,
    pub peer_id: Option<String>,
    pub cid_short_name: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
/// 创世引导节点选项，供前端/首页做 PeerId 到机构简称映射。
pub struct GenesisBootnodeOption {
    pub cid_short_name: String,
    pub role: String,
    pub peer_id: String,
    pub domain: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredBootnodeMeta {
    peer_id: String,
    #[serde(default)]
    cid_short_name: Option<String>,
}

fn bootnode_meta_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(security::app_data_dir(app)?.join("bootnode-meta.json"))
}

fn load_bootnode_meta(app: &AppHandle) -> Result<Option<StoredBootnodeMeta>, String> {
    let path = bootnode_meta_path(app)?;
    let raw = match fs::read_to_string(path) {
        Ok(v) => v,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("read bootnode meta failed: {e}")),
    };
    let record: StoredBootnodeMeta =
        serde_json::from_str(&raw).map_err(|e| format!("parse bootnode meta failed: {e}"))?;
    Ok(Some(record))
}

fn save_bootnode_meta(
    app: &AppHandle,
    peer_id: &str,
    cid_short_name: Option<String>,
) -> Result<(), String> {
    let raw = serde_json::to_string_pretty(&StoredBootnodeMeta {
        peer_id: peer_id.to_string(),
        cid_short_name,
    })
    .map_err(|e| format!("encode bootnode meta failed: {e}"))?;
    security::write_text_atomic(&bootnode_meta_path(app)?, &format!("{raw}\n"))
        .map_err(|e| format!("write bootnode meta failed: {e}"))
}

fn clear_bootnode_meta(app: &AppHandle) -> Result<(), String> {
    match fs::remove_file(bootnode_meta_path(app)?) {
        Ok(_) => Ok(()),
        Err(err) if err.kind() == ErrorKind::NotFound => Ok(()),
        Err(err) => Err(format!("remove bootnode meta failed: {err}")),
    }
}

fn has_secret_ed25519(app: &AppHandle) -> Result<bool, String> {
    let secret_path = node_key_path(app)?;
    Ok(secret_path.is_file())
}

/// 返回节点身份密钥的统一路径：`<app_data>/node-key/secret_ed25519`。
/// 不依赖链 ID，但正式版与开发版各自使用独立 app_data 命名空间。
fn node_key_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(crate::shared::keystore::node_data_dir(app)?
        .join(NODE_KEY_DIR)
        .join(SUBSTRATE_SECRET_ED25519))
}

pub(crate) fn genesis_bootnode_options() -> Result<Vec<GenesisBootnodeOption>, String> {
    let options = grandpa_address::load_institution_catalog()?
        .into_iter()
        .map(|entry| GenesisBootnodeOption {
            cid_short_name: entry.cid_short_name,
            role: entry.role,
            peer_id: entry.peer_id,
            domain: entry.domain,
        })
        .collect::<Vec<GenesisBootnodeOption>>();
    if options.is_empty() {
        return Err("未配置创世引导节点".to_string());
    }
    Ok(options)
}

pub(crate) fn find_genesis_bootnode_cid_short_name_by_peer_id(
    peer_id: &str,
) -> Result<Option<String>, String> {
    Ok(genesis_bootnode_options()?
        .into_iter()
        .find(|n| n.peer_id == peer_id)
        .map(|n| n.cid_short_name))
}

fn is_genesis_bootnode_peer_id(peer_id: &str) -> Result<bool, String> {
    Ok(genesis_bootnode_options()?
        .iter()
        .any(|node| node.peer_id == peer_id))
}

fn peer_id_from_node_key_hex(node_key_hex: &str) -> Result<String, String> {
    let secret_bytes =
        decode_hex_32_strict(node_key_hex).map_err(|_| "node-key 格式无效".to_string())?;
    let secret = libp2p_identity::ed25519::SecretKey::try_from_bytes(secret_bytes)
        .map_err(|_| "无效 node-key，无法生成 ed25519 私钥".to_string())?;
    let keypair = libp2p_identity::ed25519::Keypair::from(secret);
    let public = libp2p_identity::PublicKey::from(keypair.public());
    let peer_id: PeerId = public.to_peer_id();
    Ok(peer_id.to_string())
}

/// 将引导节点私钥以原始 32 字节写入 `<app_data>/node-key/secret_ed25519`。
///
/// 密钥存放在节点根目录下而非 `chains/<id>/network/` 中，
/// 使同一数据命名空间内的链配置共用同一个 Peer ID 身份。
/// 节点启动时通过 `--node-key-file` 参数显式加载。
fn write_secret_ed25519(app: &AppHandle, secret_bytes: &[u8]) -> Result<(), String> {
    let secret_path = node_key_path(app)?;
    let key_dir = secret_path.parent().ok_or("node-key dir resolve failed")?;
    fs::create_dir_all(key_dir)
        .map_err(|e| format!("create node-key dir failed ({}): {e}", key_dir.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(key_dir, fs::Permissions::from_mode(0o700))
            .map_err(|e| format!("set node-key dir permission failed: {e}"))?;
    }
    security::write_secret_bytes_atomic(&secret_path, secret_bytes)
        .map_err(|e| format!("write secret_ed25519 failed: {e}"))
}

fn wait_peer_id_applied(app: &AppHandle, expected_peer_id: &str) -> Result<(), String> {
    let deadline = Instant::now() + PEER_ID_WAIT_TIMEOUT;
    let mut last_peer_id: Option<String> = None;
    while Instant::now() < deadline {
        if let Ok(identity) = home::get_node_identity_blocking(app.clone()) {
            last_peer_id = identity.peer_id.clone();
            if identity.peer_id.as_deref() == Some(expected_peer_id) {
                return Ok(());
            }
        }
        thread::sleep(STATUS_POLL_INTERVAL);
    }
    let observed = last_peer_id.unwrap_or_else(|| "<none>".to_string());
    Err(format!(
        "等待 {} 秒后节点重启后的 PeerId 仍未切换到目标引导节点（expected={expected_peer_id}, actual={observed}）",
        PEER_ID_WAIT_TIMEOUT.as_secs()
    ))
}

#[tauri::command]
pub fn get_bootnode_key(app: AppHandle) -> Result<BootnodeKey, String> {
    match load_bootnode_meta(&app)? {
        Some(meta) => {
            // 若 meta 存在但 secret_ed25519 文件已不存在（如链数据被清除），
            // 自动清除过期 meta，返回空状态（等同全新安装）。
            if !has_secret_ed25519(&app)? {
                eprintln!("[bootnode] secret_ed25519 缺失，自动清除 bootnode-meta.json");
                clear_bootnode_meta(&app)?;
                return Ok(BootnodeKey {
                    node_key: None,
                    peer_id: None,
                    cid_short_name: None,
                });
            }
            Ok(BootnodeKey {
                // 私钥不回传给前端，避免二次暴露。
                node_key: None,
                peer_id: Some(meta.peer_id),
                cid_short_name: meta.cid_short_name,
            })
        }
        None => Ok(BootnodeKey {
            node_key: None,
            peer_id: None,
            cid_short_name: None,
        }),
    }
}

#[tauri::command]
pub fn set_bootnode_key(
    app: AppHandle,
    node_key: String,
    unlock_password: String,
) -> Result<BootnodeKey, String> {
    if let Err(e) = security::append_audit_log(&app, "set_bootnode_key", "attempt") {
        eprintln!("[审计] set_bootnode_key attempt 日志写入失败: {e}");
    }
    let unlock = security::ensure_unlock_password(&unlock_password)?;
    device_password::verify_device_login_password(&app, unlock)?;
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
    if home::current_status(&app)?.running {
        if let Err(err) = (|| -> Result<(), String> {
            let _ = home::stop_node_blocking(app.clone())?;
            let _ = home::start_node_blocking(app.clone())?;
            wait_peer_id_applied(&app, &derived_peer_id)?;
            Ok(())
        })() {
            if let Err(e) =
                security::append_audit_log(&app, "set_bootnode_key", "saved_restart_failed")
            {
                eprintln!("[审计] set_bootnode_key saved_restart_failed 日志写入失败: {e}");
            }
            return Err(format!(
                "引导节点私钥已保存，但节点重启失败：{err}。新密钥将在下次成功启动节点时自动生效。"
            ));
        }
    }
    if let Err(e) = security::append_audit_log(&app, "set_bootnode_key", "success") {
        eprintln!("[审计] set_bootnode_key success 日志写入失败: {e}");
    }

    Ok(BootnodeKey {
        node_key: None,
        peer_id: Some(derived_peer_id),
        cid_short_name,
    })
}

#[tauri::command]
pub fn get_genesis_bootnode_options() -> Result<Vec<GenesisBootnodeOption>, String> {
    genesis_bootnode_options()
}

#[cfg(test)]
mod tests {
    use super::StoredBootnodeMeta;

    #[test]
    fn old_duplicate_name_field_is_rejected_instead_of_silently_accepted() {
        let old_field = concat!("authority", "NodeLabel");
        let raw = format!(r#"{{"peer_id":"peer","{old_field}":"旧名称"}}"#);
        assert!(serde_json::from_str::<StoredBootnodeMeta>(&raw).is_err());
    }
}
