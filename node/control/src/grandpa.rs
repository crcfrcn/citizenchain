//! 读取机构目录并管理本机GRANDPA密钥元数据。

use codec::{DecodeAll, Encode};
use libp2p_identity::PeerId;
use nodedesktop::address::decode_hex_32_strict;
use nodedesktop::constants::RPC_RESPONSE_LIMIT_LARGE;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;
use std::collections::HashSet;
use std::fs;
use std::io::ErrorKind;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::OnceLock;
use std::thread;
use std::time::Duration;
use std::time::Instant;
use tauri::AppHandle;
use zeroize::Zeroizing;

const GRANDPA_KEY_TYPE_HEX_PREFIX: &str = "6772616e";

const INSTITUTION_CATALOG_SRC: &str = include_str!("catalog.json");

const AUTHORITY_ROLE_WAIT_TIMEOUT: Duration = Duration::from_secs(20);

const STATUS_POLL_INTERVAL: Duration = Duration::from_millis(250);

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
/// 前端展示的 GRANDPA 私钥绑定状态。
pub struct GrandpaKey {
    pub key: Option<String>,
    pub cid_short_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredGrandpaMeta {
    #[serde(default)]
    cid_short_name: Option<String>,
    #[serde(default)]
    public_key: Option<String>,
}

#[derive(Debug, Clone)]
struct GrandpaKeystoreBackupEntry {
    path: PathBuf,
    content: String,
}

#[derive(Debug, Clone)]
pub struct GrandpaPersistedStateBackup {
    meta: Option<StoredGrandpaMeta>,
    keystore_files: Vec<GrandpaKeystoreBackupEntry>,
}

#[derive(Debug, Clone)]
/// 节点运行目录条目；机构简称只允许由 `CHINA_CB.cid_short_name` 派生，
/// 节点目录不保存第二份名称或链上公钥副本。
pub(crate) struct InstitutionCatalogEntry {
    pub cid_short_name: String,
    pub role: String,
    pub peer_id: String,
    pub cid_number: String,
    pub domain: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StoredInstitutionCatalogEntry {
    /// 权威节点角色：`nrc` | `prc` | `prb`
    #[serde(default)]
    pub role: String,
    pub peer_id: String,
    pub cid_number: String,
    /// 引导节点域名（如 `nrcgch.crcfrcn.com`），用于远程 RPC 查询。
    #[serde(default)]
    pub domain: String,
}

static INSTITUTION_CATALOG: OnceLock<Vec<InstitutionCatalogEntry>> = OnceLock::new();

/// 获取权威节点清单（OnceLock 惰性初始化，编译期内嵌 JSON 只解析一次）。
pub(crate) fn load_institution_catalog() -> Result<Vec<InstitutionCatalogEntry>, String> {
    if let Some(catalog) = INSTITUTION_CATALOG.get() {
        return Ok(catalog.clone());
    }
    let catalog = parse_institution_catalog()?;
    let _ = INSTITUTION_CATALOG.set(catalog);
    INSTITUTION_CATALOG
        .get()
        .cloned()
        .ok_or_else(|| "初始化权威节点清单失败".to_string())
}

// 节点目录只按机构CID关联运行参数，GRANDPA公钥另从链上查询。
// 加载时统一做 trim / 去重 / 格式校验，避免配置里的空格或脏数据影响运行时判断。
fn parse_institution_catalog() -> Result<Vec<InstitutionCatalogEntry>, String> {
    let entries: Vec<StoredInstitutionCatalogEntry> = serde_json::from_str(INSTITUTION_CATALOG_SRC)
        .map_err(|e| format!("parse institution-catalog.json failed: {e}"))?;
    if entries.is_empty() {
        return Err("institution-catalog.json 为空".to_string());
    }

    let mut seen_cid_short_names = HashSet::new();
    let mut seen_peer_ids = HashSet::new();
    let mut seen_cids = HashSet::new();
    let mut normalized_entries = Vec::with_capacity(entries.len());
    for (idx, entry) in entries.iter().enumerate() {
        let line = idx + 1;
        let peer_id = entry.peer_id.trim();
        if peer_id.is_empty() {
            return Err(format!(
                "institution-catalog.json 第 {line} 项 peerId 不能为空"
            ));
        }
        PeerId::from_str(peer_id)
            .map_err(|_| format!("institution-catalog.json 第 {line} 项 peerId 无效"))?;
        if !seen_peer_ids.insert(peer_id.to_string()) {
            return Err(format!("institution-catalog.json peerId 重复: {peer_id}"));
        }

        if !seen_cids.insert(entry.cid_number.clone()) {
            return Err("节点目录机构CID重复".into());
        }
        // 节点运行参数只按稳定机构CID关联名称，不保存或匹配静态公钥副本。
        let institution = primitives::cid::china::china_cb::CHINA_CB
            .iter()
            .find(|i| i.cid_number == entry.cid_number)
            .ok_or_else(|| format!("节点目录第{line}项机构CID未匹配固定储委会"))?;
        let cid_short_name = institution.cid_short_name;
        if !seen_cid_short_names.insert(cid_short_name.to_string()) {
            return Err(format!(
                "institution-catalog.json 机构简称重复: {cid_short_name}"
            ));
        }
        let expected_role = if institution.cid_number.contains("-NRC") {
            "nrc"
        } else {
            "prc"
        };
        if entry.role != expected_role {
            return Err(format!(
                "institution-catalog.json 第 {line} 项角色与 {cid_short_name} 不一致"
            ));
        }

        normalized_entries.push(InstitutionCatalogEntry {
            cid_short_name: cid_short_name.to_string(),
            role: entry.role.clone(),
            peer_id: peer_id.to_string(),
            cid_number: entry.cid_number.clone(),
            domain: entry.domain.trim().to_string(),
        });
    }

    Ok(normalized_entries)
}

fn grandpa_meta_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(nodedesktop::security::app_data_dir(app)?.join("grandpa-meta.json"))
}

fn load_grandpa_meta(app: &AppHandle) -> Result<Option<StoredGrandpaMeta>, String> {
    let path = grandpa_meta_path(app)?;
    let raw = match fs::read_to_string(path) {
        Ok(v) => v,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("read grandpa meta failed: {e}")),
    };
    let record: StoredGrandpaMeta =
        serde_json::from_str(&raw).map_err(|e| format!("parse grandpa meta failed: {e}"))?;
    Ok(Some(record))
}

pub fn save_grandpa_meta(
    app: &AppHandle,
    cid_short_name: Option<String>,
    public_key: Option<String>,
) -> Result<(), String> {
    let raw = serde_json::to_string_pretty(&StoredGrandpaMeta {
        cid_short_name,
        public_key,
    })
    .map_err(|e| format!("encode grandpa meta failed: {e}"))?;
    nodedesktop::security::write_text_atomic(&grandpa_meta_path(app)?, &format!("{raw}\n"))
        .map_err(|e| format!("write grandpa meta failed: {e}"))
}

fn clear_grandpa_meta(app: &AppHandle) -> Result<(), String> {
    match fs::remove_file(grandpa_meta_path(app)?) {
        Ok(_) => Ok(()),
        Err(err) if err.kind() == ErrorKind::NotFound => Ok(()),
        Err(err) => Err(format!("remove grandpa meta failed: {err}")),
    }
}

pub fn snapshot_grandpa_persisted_state(
    app: &AppHandle,
) -> Result<GrandpaPersistedStateBackup, String> {
    let meta = load_grandpa_meta(app)?;
    let dirs = nodedesktop::keystore::keystore_dirs(app)?;
    let mut keystore_files = Vec::new();
    for path in nodedesktop::keystore::scan_keystore_files(&dirs, GRANDPA_KEY_TYPE_HEX_PREFIX)? {
        let content = fs::read_to_string(&path).map_err(|e| {
            format!(
                "read grandpa keystore backup failed ({}): {e}",
                nodedesktop::security::sanitize_path(&path)
            )
        })?;
        keystore_files.push(GrandpaKeystoreBackupEntry { path, content });
    }
    Ok(GrandpaPersistedStateBackup {
        meta,
        keystore_files,
    })
}

fn remove_all_grandpa_keystore_files(app: &AppHandle) -> Result<(), String> {
    let dirs = nodedesktop::keystore::keystore_dirs(app)?;
    for path in nodedesktop::keystore::scan_keystore_files(&dirs, GRANDPA_KEY_TYPE_HEX_PREFIX)? {
        match fs::remove_file(&path) {
            Ok(_) => {}
            Err(err) if err.kind() == ErrorKind::NotFound => {}
            Err(err) => {
                return Err(format!(
                    "remove grandpa keystore file failed ({}): {err}",
                    nodedesktop::security::sanitize_path(&path)
                ));
            }
        }
    }
    Ok(())
}

pub fn restore_grandpa_persisted_state(
    app: &AppHandle,
    backup: &GrandpaPersistedStateBackup,
) -> Result<(), String> {
    match &backup.meta {
        Some(meta) => save_grandpa_meta(app, meta.cid_short_name.clone(), meta.public_key.clone())?,
        None => clear_grandpa_meta(app)?,
    }

    remove_all_grandpa_keystore_files(app)?;
    for entry in &backup.keystore_files {
        nodedesktop::security::write_secret_text_atomic(&entry.path, &entry.content).map_err(
            |e| {
                format!(
                    "restore grandpa keystore file failed ({}): {e}",
                    nodedesktop::security::sanitize_path(&entry.path)
                )
            },
        )?;
    }
    Ok(())
}

pub fn write_grandpa_key_to_keystore(
    app: &AppHandle,
    private_hex: &str,
    public_key: &str,
) -> Result<(), String> {
    let secret = Zeroizing::new(format!("0x{private_hex}"));
    let encoded = serde_json::to_string(&*secret)
        .map_err(|e| format!("encode grandpa keystore secret failed: {e}"))?;
    let content = Zeroizing::new(format!("{encoded}\n"));
    let dirs = nodedesktop::keystore::keystore_dirs(app)?;
    // 初次配置只保留一把 gran 密钥；治理换钥必须改走 rotation 专用的双钥保留函数。
    nodedesktop::keystore::write_key_to_keystore(
        &dirs,
        GRANDPA_KEY_TYPE_HEX_PREFIX,
        public_key.trim_start_matches("0x"),
        &content,
    )
}

/// 导入治理换钥候选私钥，同时保留旧 GRANDPA 私钥。
pub fn import_rotation_candidate(
    app: &AppHandle,
    private_key: &[u8; 32],
    public_key: &[u8; 32],
) -> Result<(), String> {
    let secret = Zeroizing::new(format!("0x{}", hex::encode(private_key)));
    let encoded = serde_json::to_string(&*secret)
        .map_err(|e| format!("encode grandpa rotation secret failed: {e}"))?;
    let content = Zeroizing::new(format!("{encoded}\n"));
    let dirs = nodedesktop::keystore::keystore_dirs(app)?;
    nodedesktop::keystore::write_key_to_keystore_preserving_others(
        &dirs,
        GRANDPA_KEY_TYPE_HEX_PREFIX,
        &hex::encode(public_key),
        &content,
    )
}

/// 使用本机准确公钥对应的 GRANDPA 私钥签名证明摘要。
pub fn sign_rotation_proof(
    app: &AppHandle,
    public_key: &[u8; 32],
    digest: &[u8; 32],
) -> Result<[u8; 64], String> {
    use ed25519_dalek::Signer;

    let dirs = nodedesktop::keystore::keystore_dirs(app)?;
    let raw = nodedesktop::keystore::read_key_from_keystore(
        &dirs,
        GRANDPA_KEY_TYPE_HEX_PREFIX,
        &hex::encode(public_key),
    )?
    .ok_or_else(|| {
        format!(
            "本机缺少当前 GRANDPA 私钥（public_key=0x{}）",
            hex::encode(public_key)
        )
    })?;
    let private_text: Zeroizing<String> = Zeroizing::new(
        serde_json::from_str(raw.trim())
            .map_err(|e| format!("decode grandpa keystore secret failed: {e}"))?,
    );
    let private_key = decode_hex_32_strict(private_text.trim_start_matches("0x"))
        .map_err(|_| "GRANDPA keystore 私钥不是 32 字节十六进制".to_string())?;
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&private_key);
    ensure_public_key_matches(&signing_key, public_key)?;
    Ok(signing_key.sign(digest).to_bytes())
}

fn ensure_public_key_matches(
    signing_key: &ed25519_dalek::SigningKey,
    expected_public_key: &[u8; 32],
) -> Result<(), String> {
    if signing_key.verifying_key().to_bytes() != *expected_public_key {
        return Err("GRANDPA keystore 私钥与文件名公钥不匹配".to_string());
    }
    Ok(())
}

/// 在新 authority 已 finalized 后删除旧私钥，并把节点元数据切到新公钥。
pub fn finalize_rotation_key(
    app: &AppHandle,
    cid_short_name: &str,
    old_public_key: &[u8; 32],
    new_public_key: &[u8; 32],
) -> Result<(), String> {
    let dirs = nodedesktop::keystore::keystore_dirs(app)?;
    let new_public_key_hex = hex::encode(new_public_key);
    if !nodedesktop::keystore::has_key_in_keystore(
        &dirs,
        GRANDPA_KEY_TYPE_HEX_PREFIX,
        &new_public_key_hex,
    ) {
        return Err(format!(
            "新 GRANDPA authority 已生效，但本机缺少新私钥（public_key=0x{new_public_key_hex}）"
        ));
    }
    nodedesktop::keystore::remove_key_from_keystore(
        &dirs,
        GRANDPA_KEY_TYPE_HEX_PREFIX,
        &hex::encode(old_public_key),
    )?;
    save_grandpa_meta(
        app,
        Some(cid_short_name.to_string()),
        Some(format!("0x{new_public_key_hex}")),
    )
}

/// 放弃尚未提交的候选新私钥，不触碰当前旧私钥和节点元数据。
pub fn discard_rotation_candidate(
    app: &AppHandle,
    new_public_key: &[u8; 32],
) -> Result<(), String> {
    let dirs = nodedesktop::keystore::keystore_dirs(app)?;
    nodedesktop::keystore::remove_key_from_keystore(
        &dirs,
        GRANDPA_KEY_TYPE_HEX_PREFIX,
        &hex::encode(new_public_key),
    )
}

/// 由机构 CID 定位正式机构简称，不依赖会被治理更换的 GRANDPA 公钥。
pub fn cid_short_name_for_cid(actor_cid_number: &str) -> Result<String, String> {
    load_institution_catalog()?
        .into_iter()
        .find(|entry| {
            matches!(entry.role.as_str(), "nrc" | "prc") && entry.cid_number == actor_cid_number
        })
        .map(|entry| entry.cid_short_name)
        .ok_or_else(|| "节点目录中没有该储委会机构CID".into())
}

fn has_grandpa_key_in_keystore(app: &AppHandle, public_key: &str) -> Result<bool, String> {
    let dirs = nodedesktop::keystore::keystore_dirs(app)?;
    Ok(nodedesktop::keystore::has_key_in_keystore(
        &dirs,
        GRANDPA_KEY_TYPE_HEX_PREFIX,
        public_key.trim_start_matches("0x"),
    ))
}

/// 公钥唯一归属只从同一最终块的GrandpaKey正反索引闭环读取。
fn registered_owner(
    public_key: &str,
    mut read: impl FnMut(&str) -> Result<Option<String>, String>,
) -> Result<Option<String>, String> {
    let normalized = nodedesktop::validation::normalize_public_key(public_key)?;
    let public: [u8; 32] = hex::decode(normalized.trim_start_matches("0x"))
        .map_err(|_| "GRANDPA公钥hex无效")?
        .try_into()
        .map_err(|_| "GRANDPA公钥长度无效")?;
    let key = nodedesktop::storage::to_hex(&nodedesktop::storage::blake2_map(
        b"GrandpaKey",
        b"GrandpaKeyOwnerByKey",
        &public.encode(),
    ));
    let Some(raw) = read(&key)? else {
        return Ok(None);
    };
    let raw = hex::decode(raw.strip_prefix("0x").ok_or("公钥反向索引hex缺前缀")?)
        .map_err(|e| e.to_string())?;
    let cid =
        Vec::<u8>::decode_all(&mut raw.as_slice()).map_err(|e| format!("公钥反向索引损坏:{e}"))?;
    let cid = String::from_utf8(cid).map_err(|_| "公钥所属CID非UTF-8")?;
    if cid.trim() != cid
        || !primitives::cid::number::parse_cid_number_parts_bytes(cid.as_bytes()).is_ok_and(|p| {
            p.institution == primitives::cid::code::NRC
                || p.institution == primitives::cid::code::PRC
        })
    {
        return Err("公钥所属机构CID无效".into());
    }
    let key = nodedesktop::storage::to_hex(&nodedesktop::storage::blake2_map(
        b"GrandpaKey",
        b"CurrentGrandpaKeys",
        &cid.as_bytes().to_vec().encode(),
    ));
    let raw = read(&key)?.ok_or("登记公钥正向索引缺失")?;
    let raw = hex::decode(raw.strip_prefix("0x").ok_or("登记公钥hex缺前缀")?)
        .map_err(|e| e.to_string())?;
    let current =
        <[u8; 32]>::decode_all(&mut raw.as_slice()).map_err(|e| format!("登记公钥损坏:{e}"))?;
    if current != public {
        return Err("公钥正反索引不一致".into());
    }
    Ok(Some(cid))
}
pub fn cid_short_name_by_grandpa_public_key(public_key: &str) -> Result<Option<String>, String> {
    let at = rpc_post("chain_getFinalizedHead", serde_json::json!([]))?
        .as_str()
        .ok_or("最终块哈希缺失")?
        .to_string();
    let owner = registered_owner(public_key, |key| {
        match rpc_post("state_getStorage", serde_json::json!([key, &at]))? {
            Value::Null => Ok(None),
            Value::String(s) => Ok(Some(s)),
            _ => Err("登记公钥存储响应无效".into()),
        }
    })?;
    owner.map(|cid| cid_short_name_for_cid(&cid)).transpose()
}

pub fn grandpa_public_key_from_private_hex(key_hex: &str) -> Result<String, String> {
    let secret = decode_hex_32_strict(key_hex)
        .map_err(|_| "GRANDPA 私钥格式无效，应为 64 位十六进制".to_string())?;
    let signing = ed25519_dalek::SigningKey::from_bytes(&secret);
    let verify = signing.verifying_key();
    Ok(format!("0x{}", hex::encode(verify.to_bytes())))
}

fn rpc_post(method: &str, params: Value) -> Result<Value, String> {
    nodedesktop::rpc::rpc_post(
        method,
        params,
        nodedesktop::rpc::RPC_REQUEST_TIMEOUT,
        RPC_RESPONSE_LIMIT_LARGE,
    )
}

fn node_roles() -> Result<Vec<String>, String> {
    let value = rpc_post("system_nodeRoles", Value::Array(vec![]))?;
    let Some(list) = value.as_array() else {
        return Ok(Vec::new());
    };
    Ok(list
        .iter()
        .filter_map(|v| v.as_str().map(|s| s.to_string()))
        .collect())
}

fn is_authority_role(roles: &[String]) -> bool {
    roles.iter().any(|role| {
        let lower = role.to_ascii_lowercase();
        lower == "authority" || lower == "validator"
    })
}

fn wait_for_authority_role() -> Result<(), String> {
    let deadline = Instant::now() + AUTHORITY_ROLE_WAIT_TIMEOUT;
    let mut last_roles = Vec::new();
    while Instant::now() < deadline {
        if let Ok(roles) = node_roles() {
            last_roles = roles.clone();
            if is_authority_role(&roles) {
                return Ok(());
            }
        }
        thread::sleep(STATUS_POLL_INTERVAL);
    }
    let observed = if last_roles.is_empty() {
        "<none>".to_string()
    } else {
        last_roles.join(", ")
    };
    Err(format!(
        "等待 {} 秒后节点仍未进入 AUTHORITY/VALIDATOR 角色（last_roles={observed}）",
        AUTHORITY_ROLE_WAIT_TIMEOUT.as_secs()
    ))
}

pub fn prepare_grandpa_for_start(app: &AppHandle) -> Result<bool, String> {
    let Some(meta) = load_grandpa_meta(app)? else {
        return Ok(false);
    };
    if meta.cid_short_name.is_none() {
        return Ok(false);
    }
    let Some(public_key) = meta.public_key.as_deref() else {
        return Ok(false);
    };

    // 治理换钥后的公钥不会继续出现在静态创世清单中；启动资格以本机元数据、keystore
    // 和 service 对当前链上 authority set 的真实匹配为准，不能退回静态公钥白名单。
    // 确认 keystore 文件存在（初次配置或治理换钥时已写入）。
    // 若 keystore 缺失（如链数据被清除），自动清除过期的 meta，以普通节点启动。
    if !has_grandpa_key_in_keystore(app, public_key)? {
        eprintln!("[GRANDPA] keystore 缺失，自动清除 grandpa-meta.json，以普通节点启动");
        clear_grandpa_meta(app)?;
        return Ok(false);
    }
    Ok(true)
}

pub fn verify_grandpa_after_start(app: &AppHandle) -> Result<(), String> {
    let Some(meta) = load_grandpa_meta(app)? else {
        return Ok(());
    };
    let Some(public_key) = meta.public_key.as_deref() else {
        return Ok(());
    };

    wait_for_authority_role()?;
    if !has_grandpa_key_in_keystore(app, public_key)? {
        return Err(format!(
            "未在本地 keystore 检测到 GRANDPA 密钥文件（public_key={public_key}）"
        ));
    }
    Ok(())
}

#[tauri::command]
pub fn get_grandpa_key(app: AppHandle) -> Result<GrandpaKey, String> {
    let meta = load_grandpa_meta(&app)?;
    let cid_short_name = meta.as_ref().and_then(|v| v.cid_short_name.clone());
    if cid_short_name.is_none() {
        return Ok(GrandpaKey {
            key: None,
            cid_short_name: None,
        });
    }
    // 若 meta 记录了机构简称但 keystore 文件已不存在（如链数据被清除），
    // 自动清除过期 meta，返回空状态（等同全新安装）。
    if let Some(public_key) = meta.as_ref().and_then(|v| v.public_key.as_deref()) {
        if !has_grandpa_key_in_keystore(&app, public_key)? {
            eprintln!("[GRANDPA] get_grandpa_key: keystore 缺失，自动清除 grandpa-meta.json");
            clear_grandpa_meta(&app)?;
            return Ok(GrandpaKey {
                key: None,
                cid_short_name: None,
            });
        }
    }
    Ok(GrandpaKey {
        // 私钥不回传给前端，避免二次暴露。
        key: None,
        cid_short_name,
    })
}
#[cfg(test)]
#[path = "../tests/grandpa.rs"]
mod tests;
