//! 桌面节点奖励账户配置、链上绑定与本机矿工账户读取。

use crate::{
    settings::{
        address_utils::{decode_hex_32_with_optional_0x, decode_ss58_prefix},
        device_password,
    },
    shared::{
        constants::{EXPECTED_SS58_PREFIX, SS58_PREFIX},
        keystore, rpc, security,
        validation::{normalize_account_id, normalize_ss58_address},
    },
};
use serde::{Deserialize, Serialize};
use std::{fs, io::ErrorKind, path::PathBuf};
use tauri::{AppHandle, Emitter};

const POWR_KEY_TYPE_HEX_PREFIX: &str = "706f7772";
const REWARD_BIND_TIMEOUT_SECS: u64 = 45;
use crate::shared::constants::RPC_RESPONSE_LIMIT_LARGE;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
/// 前端展示的奖励账户配置；账户 ID 是唯一标识，SS58 仅供输入与展示。
pub struct RewardAccount {
    #[serde(rename = "account_id")]
    pub account_id: Option<String>,
    #[serde(rename = "ss58_address")]
    pub ss58_address: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredRewardAccount {
    account_id: String,
}

fn reward_account_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(security::app_data_dir(app)?.join("reward-account.json"))
}

pub(crate) fn load_reward_account_id(app: &AppHandle) -> Result<Option<String>, String> {
    let path = reward_account_path(app)?;
    let raw = match fs::read_to_string(path) {
        Ok(v) => v,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("读取奖励账户失败: {e}")),
    };
    let stored: StoredRewardAccount =
        serde_json::from_str(&raw).map_err(|e| format!("解析奖励账户失败: {e}"))?;
    normalize_account_id(&stored.account_id).map(Some)
}

fn save_reward_account_id(app: &AppHandle, account_id: &str) -> Result<(), String> {
    let account_id = normalize_account_id(account_id)?;
    let raw = serde_json::to_string_pretty(&StoredRewardAccount { account_id })
        .map_err(|e| format!("编码奖励账户失败: {e}"))?;
    security::write_text_atomic(&reward_account_path(app)?, &format!("{raw}\n"))
        .map_err(|e| format!("写入奖励账户失败: {e}"))
}

/// 仅扫描默认链（citizenchain）的 keystore 目录中的 powr 文件。
/// 不遍历其他链目录，避免旧链残留 keystore 导致矿工身份错位。
fn collect_powr_keystore_files(app: &AppHandle) -> Result<Vec<PathBuf>, String> {
    let keystore_dir = keystore::default_chain_keystore_dir(app)?;
    if !keystore_dir.is_dir() {
        return Ok(Vec::new());
    }

    let mut files = Vec::new();
    let entries = fs::read_dir(&keystore_dir).map_err(|e| {
        format!(
            "read keystore dir failed ({}): {e}",
            security::sanitize_path(&keystore_dir)
        )
    })?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("read keystore file entry failed: {e}"))?;
        let file_type = entry
            .file_type()
            .map_err(|e| format!("read keystore file type failed: {e}"))?;
        if file_type.is_symlink() {
            continue;
        }
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|v| v.to_str()) else {
            continue;
        };
        if name.starts_with(POWR_KEY_TYPE_HEX_PREFIX) {
            files.push(path);
        }
    }
    files.sort();
    Ok(files)
}

fn miner_account_id_from_keystore_filename(name: &str) -> Option<String> {
    let hex = name.strip_prefix(POWR_KEY_TYPE_HEX_PREFIX)?;
    if hex.len() != 64 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    Some(format!("0x{}", hex.to_ascii_lowercase()))
}

pub(crate) fn local_powr_miner_account_id(app: &AppHandle) -> Result<Option<String>, String> {
    for path in collect_powr_keystore_files(app)? {
        let Some(name) = path.file_name().and_then(|v| v.to_str()) else {
            continue;
        };
        if let Some(account_id) = miner_account_id_from_keystore_filename(name) {
            return Ok(Some(account_id));
        }
    }
    Ok(None)
}

fn decode_ss58_account_id(address: &str) -> Result<[u8; 32], String> {
    let data = bs58::decode(address)
        .into_vec()
        .map_err(|_| "SS58 地址解码失败".to_string())?;
    let (prefix, prefix_len) = decode_ss58_prefix(&data)?;
    if prefix != SS58_PREFIX {
        return Err("SS58 地址前缀无效，必须为 2027".to_string());
    }
    if data.len() < prefix_len + 32 + 2 {
        return Err("SS58 地址长度无效".to_string());
    }
    let payload_len = data.len() - prefix_len - 2;
    if payload_len != 32 {
        return Err("SS58 地址账户长度无效，必须是 32 字节".to_string());
    }

    let (without_checksum, checksum) = data.split_at(data.len() - 2);
    let hash = blake2b_simd::Params::new()
        .hash_length(64)
        .to_state()
        .update(b"SS58PRE")
        .update(without_checksum)
        .finalize();
    if checksum != &hash.as_bytes()[..2] {
        return Err("SS58 地址校验和无效".to_string());
    }

    let mut out = [0u8; 32];
    out.copy_from_slice(&data[prefix_len..prefix_len + 32]);
    Ok(out)
}

fn account_id_from_ss58_address(ss58_address: &str) -> Result<[u8; 32], String> {
    decode_ss58_account_id(ss58_address)
}

fn ensure_expected_reward_account_rpc_node() -> Result<(), String> {
    let properties = rpc::rpc_post(
        "system_properties",
        serde_json::Value::Array(vec![]),
        rpc::RPC_REQUEST_TIMEOUT,
        RPC_RESPONSE_LIMIT_LARGE,
    )?;
    let ss58 = properties
        .get("ss58Format")
        .and_then(|v| {
            if let Some(raw) = v.as_u64() {
                Some(raw)
            } else {
                v.as_str().and_then(|s| s.parse::<u64>().ok())
            }
        })
        .ok_or_else(|| "RPC 节点缺少 ss58Format".to_string())?;
    if ss58 != EXPECTED_SS58_PREFIX {
        return Err(format!("RPC 链前缀不匹配：expected=2027, got={ss58}"));
    }

    let name = rpc::rpc_post(
        "system_name",
        serde_json::Value::Array(vec![]),
        rpc::RPC_REQUEST_TIMEOUT,
        RPC_RESPONSE_LIMIT_LARGE,
    )?
    .as_str()
    .map(str::trim)
    .unwrap_or("")
    .to_string();
    if name.is_empty() {
        return Err("RPC 节点名称为空".to_string());
    }
    Ok(())
}

fn reward_account_storage_key(miner_account_id: &[u8; 32]) -> Vec<u8> {
    crate::shared::storage_keys::blake2_map(
        b"FullnodeIssuance",
        b"RewardAccountIdByMiner",
        miner_account_id,
    )
}

fn decode_storage_account_id(raw: &[u8]) -> Result<[u8; 32], String> {
    if raw.len() < 32 {
        return Err("链上 RewardAccountIdByMiner 数据长度无效".to_string());
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(&raw[..32]);
    Ok(out)
}

/// 通过 node 端自定义 RPC 绑定或重绑奖励账户。
/// node 端使用 keystore 中的矿工密钥直接签名并提交交易。
///
/// 本函数在以下两种场景被调用：
/// 1. 用户主动设置奖励账户（`set_reward_account`）；
/// 2. 节点启动后自动同步（`start_node` 成功后），确保清链/重装后
///    本地已保存的账户 ID 能重新绑定到链上。
pub(crate) async fn sync_saved_reward_account_inner(app: &AppHandle) -> Result<(), String> {
    let app_clone = app.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let Some(target_account_id_hex) = load_reward_account_id(&app_clone)? else {
            return Ok(None);
        };
        ensure_expected_reward_account_rpc_node()?;
        let target_account_id =
            decode_hex_32_with_optional_0x(target_account_id_hex.trim_start_matches("0x"))?;

        // 从 keystore 文件名读取矿工公钥（不读取私钥）
        let miner_account_id =
            local_powr_miner_account_id(&app_clone)?.ok_or("未找到矿工账户，请先启动节点")?;
        let miner_bytes = decode_hex_32_with_optional_0x(&miner_account_id)?;

        if target_account_id == miner_bytes {
            return Err("奖励账户不能与矿工账户相同，请使用独立收款账户".to_string());
        }

        // 查询链上当前绑定状态
        // (ADR-017):绑定状态属于链上状态读取,按 finalized 口径,禁止 best。
        let storage_key = reward_account_storage_key(&miner_bytes);
        let hex_key = format!("0x{}", hex::encode(&storage_key));
        let raw = crate::governance::chain_query::fetch_finalized_storage(&hex_key)?;
        let current_account = if let Some(hex_val) = raw.as_deref() {
            let hex_val = hex_val.trim_start_matches("0x");
            if hex_val.is_empty() {
                None
            } else {
                let bytes =
                    hex::decode(hex_val).map_err(|e| format!("解码链上绑定数据失败: {e}"))?;
                Some(decode_storage_account_id(&bytes)?)
            }
        } else {
            None
        };

        // 已是目标地址，无需操作
        if current_account == Some(target_account_id) {
            return Ok(None);
        }

        // 通过 node 端自定义 RPC 提交绑定/重绑交易
        let rpc_method = if current_account.is_some() {
            "reward_rebindAccount"
        } else {
            "reward_bindAccount"
        };
        let bind_timeout = std::time::Duration::from_secs(REWARD_BIND_TIMEOUT_SECS);
        rpc::rpc_post(
            rpc_method,
            serde_json::json!([target_account_id_hex]),
            bind_timeout,
            RPC_RESPONSE_LIMIT_LARGE,
        )?;

        Ok(Some(()))
    })
    .await
    .map_err(|e| format!("sync task failed: {e}"))??;

    let _ = result;
    Ok(())
}

#[tauri::command]
pub fn get_reward_account(app: AppHandle) -> Result<RewardAccount, String> {
    let account_id = load_reward_account_id(&app)?;
    let ss58_address = account_id
        .as_deref()
        .map(account_id_to_ss58_address)
        .transpose()?;
    Ok(RewardAccount {
        account_id,
        ss58_address,
    })
}

/// 返回本机矿工账户的 SS58 地址（前缀 2027）。
/// keystore 中没有 powr 公钥时返回 Ok(None)，由前端显示"未生成"。
#[tauri::command]
pub fn get_local_miner_ss58_address(app: AppHandle) -> Result<Option<String>, String> {
    let Some(account_id) = local_powr_miner_account_id(&app)? else {
        return Ok(None);
    };
    Ok(Some(account_id_to_ss58_address(&account_id)?))
}

fn account_id_to_ss58_address(account_id: &str) -> Result<String, String> {
    let account_id = normalize_account_id(account_id)?;
    let raw = account_id.trim_start_matches("0x");
    let account_id = hex::decode(raw).map_err(|e| format!("矿工账户 ID 解码失败: {e}"))?;
    crate::governance::signing::account_id_to_ss58(&account_id)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn set_reward_account(
    app: AppHandle,
    ss58_address: String,
    unlock_password: String,
) -> Result<RewardAccount, String> {
    if let Err(e) = security::append_audit_log(&app, "set_reward_account", "attempt") {
        eprintln!("[审计] set_reward_account attempt 日志写入失败: {e}");
    }
    let unlock = security::ensure_unlock_password(&unlock_password)?;
    device_password::verify_device_login_password(&app, unlock)?;
    let ss58_address = normalize_ss58_address(&ss58_address)?;

    // SS58 只用于输入；进入授权和存储前立即转成唯一账户 ID。
    let target_account_id = account_id_from_ss58_address(&ss58_address)?;
    let target_account_id_hex = format!("0x{}", hex::encode(target_account_id));

    // 同步路径提前拒绝：奖励账户不能与矿工账户相同。
    // 避免先存后验导致本地保存了一个链上必然被拒绝的无效地址。
    if let Some(miner_account_id) = local_powr_miner_account_id(&app)? {
        let miner_bytes = decode_hex_32_with_optional_0x(&miner_account_id)?;
        if target_account_id == miner_bytes {
            return Err("奖励账户不能与矿工账户相同，请使用独立收款账户".to_string());
        }
    }

    save_reward_account_id(&app, &target_account_id_hex)?;

    // 链上绑定在后台异步执行，通过事件通知前端结果
    let app2 = app.clone();
    tauri::async_runtime::spawn(async move {
        let sync_result = tokio::time::timeout(
            std::time::Duration::from_secs(REWARD_BIND_TIMEOUT_SECS),
            sync_saved_reward_account_inner(&app2),
        )
        .await;
        let (status, detail) = match sync_result {
            Ok(Ok(())) => ("success", String::new()),
            Ok(Err(err)) => ("failed", err),
            Err(_) => ("timeout", "链上绑定超时".to_string()),
        };
        if let Err(e) =
            security::append_audit_log(&app2, "set_reward_account", &format!("chain_bind_{status}"))
        {
            eprintln!("[审计] set_reward_account chain_bind_{status} 日志写入失败: {e}");
        }
        let _ = app2.emit(
            "reward-account-bind-result",
            serde_json::json!({ "status": status, "detail": detail }),
        );
    });

    Ok(RewardAccount {
        account_id: Some(target_account_id_hex),
        ss58_address: Some(ss58_address),
    })
}
