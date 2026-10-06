// 钱包 JSON 持久化。
//
// 文件只保存用户显式导入的 Cold 钱包公开信息；Hot 钱包由本机 powr 密钥事实动态生成，
// 不进入该文件。缺失或不属于 Hot/Cold 的签名模式由 serde 严格拒绝。

use crate::shared::security;
use serde::{Deserialize, Serialize};
use std::{fs, io::ErrorKind, path::PathBuf};
use tauri::AppHandle;

/// 钱包账户签名模式闭集。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SignMode {
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
    /// 仅用于钱包界面展示的 SS58 地址（prefix 2027）。
    pub ss58_address: String,
    /// 钱包账户 ID，固定为小写 `0x` + 64 位十六进制。
    pub account_id: String,
    pub created_at: u64,
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
    let store: WalletStore = serde_json::from_str(raw)
        .map_err(|error| format!("解析钱包文件失败: {error}"))?;
    super::validate_persisted_store(&store)?;
    Ok(store)
}

fn store_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(security::app_data_dir(app)?.join("cold-wallets.json"))
}

pub fn load(app: &AppHandle) -> Result<WalletStore, String> {
    let path = store_path(app)?;
    let raw = match fs::read_to_string(&path) {
        Ok(v) => v,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(WalletStore::default()),
        Err(e) => return Err(format!("读取钱包文件失败: {e}")),
    };
    decode_store(&raw)
}

pub fn save(app: &AppHandle, store: &WalletStore) -> Result<(), String> {
    let raw =
        serde_json::to_string_pretty(store).map_err(|e| format!("序列化钱包数据失败: {e}"))?;
    security::write_text_atomic(&store_path(app)?, &format!("{raw}\n"))
        .map_err(|e| format!("写入钱包文件失败: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const ACCOUNT_ID: &str = "0x1111111111111111111111111111111111111111111111111111111111111111";

    fn cold_wallet() -> Wallet {
        cold_wallet_for_account(ACCOUNT_ID)
    }

    #[test]
    fn sign_mode_serializes_only_hot_and_cold() {
        assert_eq!(serde_json::to_string(&SignMode::Hot).unwrap(), "\"hot\"");
        assert_eq!(serde_json::to_string(&SignMode::Cold).unwrap(), "\"cold\"");
        assert!(serde_json::from_str::<SignMode>("\"minerHot\"").is_err());
        assert!(serde_json::from_str::<SignMode>("\"external\"").is_err());
        assert!(serde_json::from_str::<SignMode>("\"local\"").is_err());
    }

    #[test]
    fn wallet_store_uses_account_id_and_required_sign_mode() {
        let store = WalletStore {
            wallets: vec![cold_wallet()],
            active_account_id: Some(ACCOUNT_ID.to_string()),
        };
        let value = serde_json::to_value(&store).unwrap();
        assert_eq!(value["activeAccountId"], ACCOUNT_ID);
        assert_eq!(value["wallets"][0]["signMode"], "cold");
        assert!(value["wallets"][0].get("id").is_none());
    }

    #[test]
    fn wallet_store_rejects_old_or_missing_mode_fields() {
        let old_kind = format!(
            r#"{{"wallets":[{{"name":"旧钱包","kind":"cold","ss58Address":"x","accountId":"{ACCOUNT_ID}","createdAt":1}}],"activeAccountId":null}}"#
        );
        let missing_mode = format!(
            r#"{{"wallets":[{{"name":"旧钱包","ss58Address":"x","accountId":"{ACCOUNT_ID}","createdAt":1}}],"activeAccountId":null}}"#
        );
        let random_id = format!(
            r#"{{"wallets":[{{"id":"legacy","name":"旧钱包","signMode":"cold","ss58Address":"x","accountId":"{ACCOUNT_ID}","createdAt":1}}],"activeAccountId":null}}"#
        );

        assert!(serde_json::from_str::<WalletStore>(&old_kind).is_err());
        assert!(serde_json::from_str::<WalletStore>(&missing_mode).is_err());
        assert!(serde_json::from_str::<WalletStore>(&random_id).is_err());
    }

    #[test]
    fn current_store_round_trips_without_rewriting() {
        let store = WalletStore { wallets: vec![cold_wallet()], active_account_id: Some(ACCOUNT_ID.to_string()) };
        assert_eq!(decode_store(&serde_json::to_string(&store).unwrap()).unwrap(), store);
        assert_eq!(decode_store(r#"{"wallets":[],"activeAccountId":null}"#).unwrap(), WalletStore::default());
    }

    #[test]
    fn current_store_rejects_legacy_and_ambiguous_fields() {
        let legacy = format!(r#"{{"wallets":[{{"id":"old","name":"旧钱包","kind":"cold","deletable":true,"ss58_address":"x","account_id":"{ACCOUNT_ID}","createdAt":1}}],"activeId":"old"}}"#);
        assert!(decode_store(&legacy).is_err());
        let store = WalletStore { wallets: vec![cold_wallet()], active_account_id: Some(ACCOUNT_ID.to_string()) };
        let value = serde_json::to_value(store).unwrap();
        for field in ["signMode", "accountId", "ss58Address"] {
            let mut missing = value.clone();
            missing["wallets"][0].as_object_mut().unwrap().remove(field);
            assert!(decode_store(&missing.to_string()).is_err());
        }
        for (field, invalid) in [("signMode", "minerHot"), ("accountId", "0x11"), ("ss58Address", "invalid")] {
            let mut malformed = value.clone();
            malformed["wallets"][0][field] = serde_json::Value::String(invalid.to_string());
            assert!(decode_store(&malformed.to_string()).is_err());
        }
        for field in ["activeId", "id", "extra"] {
            let mut unknown = value.clone();
            unknown.as_object_mut().unwrap().insert(field.to_string(), serde_json::Value::Null);
            assert!(decode_store(&unknown.to_string()).is_err());
        }
    }

    #[test]
    fn current_store_rejects_duplicates_and_inconsistent_accounts() {
        let store = WalletStore { wallets: vec![cold_wallet()], active_account_id: Some(ACCOUNT_ID.to_string()) };
        let mut duplicate = store.clone(); duplicate.wallets.push(cold_wallet());
        assert!(decode_store(&serde_json::to_string(&duplicate).unwrap()).is_err());
        let mut mismatched = store.clone();
        mismatched.wallets[0].ss58_address = cold_wallet_for_account("0x2222222222222222222222222222222222222222222222222222222222222222").ss58_address;
        assert!(decode_store(&serde_json::to_string(&mismatched).unwrap()).is_err());
        let mut dangling = store;
        dangling.active_account_id = Some("0x2222222222222222222222222222222222222222222222222222222222222222".to_string());
        // Cold 文件的活动标识可能指向动态 Hot；读取时只校验规范格式并原样保留。
        assert_eq!(decode_store(&serde_json::to_string(&dangling).unwrap()).unwrap(), dangling);
        for invalid in [
            "0x1234",
            "0xAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
            "not-an-account",
        ] {
            let mut invalid_active = dangling.clone();
            invalid_active.active_account_id = Some(invalid.to_string());
            assert!(decode_store(&serde_json::to_string(&invalid_active).unwrap()).is_err());
        }
    }

    fn cold_wallet_for_account(account_id: &str) -> Wallet {
        let account_id_bytes: [u8; 32] = hex::decode(account_id.trim_start_matches("0x"))
            .unwrap()
            .try_into()
            .unwrap();
        Wallet {
            name: "测试冷钱包".to_string(),
            sign_mode: SignMode::Cold,
            ss58_address: crate::governance::signing::account_id_to_ss58(&account_id_bytes)
                .unwrap(),
            account_id: account_id.to_string(),
            created_at: 1,
        }
    }
}
