use serde::Deserialize;
use serde::Serialize;
use std::fs;
use std::io::ErrorKind;
use std::path::PathBuf;
use tauri::AppHandle;

/// 钱包账户签名模式闭集。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(crate) enum SignMode {
    /// 本机 powr 私钥签名。
    Hot,
    /// CitizenWallet 离线扫码签名。
    Cold,
}

/// 单个钱包条目。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct Wallet {
    pub name: String,
    /// 唯一签名路由事实，只允许 Hot/Cold。
    pub sign_mode: SignMode,
    /// 钱包账户 ID，固定为小写 `0x` + 64 位十六进制。
    pub account_id: String,
}

/// 钱包列表和当前账户；`account_id` 是唯一钱包标识，不再另设随机钱包 ID。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct WalletStore {
    pub wallets: Vec<Wallet>,
    pub active_account_id: Option<String>,
}

// 只读取当前严格格式；旧格式直接失败，不写回、不删除用户的原始文件。
fn decode_store(raw: &str) -> Result<WalletStore, String> {
    let store: WalletStore =
        serde_json::from_str(raw).map_err(|error| format!("解析钱包文件失败: {error}"))?;
    crate::transfer::validate_persisted_store(&store)?;
    Ok(store)
}

fn store_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(nodedesktop::security::app_data_dir(app)?.join("cold-wallets.json"))
}

pub(crate) fn load(app: &AppHandle) -> Result<WalletStore, String> {
    let path = store_path(app)?;
    let raw = match fs::read_to_string(&path) {
        Ok(v) => v,
        Err(e) if e.kind() == ErrorKind::NotFound => {
            return Ok(crate::wallet::WalletStore::default())
        }
        Err(e) => return Err(format!("读取钱包文件失败: {e}")),
    };
    decode_store(&raw)
}

pub(crate) fn save(app: &AppHandle, store: &WalletStore) -> Result<(), String> {
    let raw =
        serde_json::to_string_pretty(store).map_err(|e| format!("序列化钱包数据失败: {e}"))?;
    nodedesktop::security::write_text_atomic(&store_path(app)?, &format!("{raw}\n"))
        .map_err(|e| format!("写入钱包文件失败: {e}"))
}
#[cfg(test)]
#[path = "../tests/wallet.rs"]
mod tests;
