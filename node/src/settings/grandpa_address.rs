use crate::{
    home,
    settings::{address_utils::decode_hex_32_strict, device_password},
    shared::{keystore, rpc, security, validation::normalize_grandpa_key},
};
use libp2p_identity::PeerId;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::HashSet,
    fs,
    io::ErrorKind,
    path::PathBuf,
    str::FromStr,
    sync::OnceLock,
    thread,
    time::{Duration, Instant},
};
use tauri::AppHandle;
use zeroize::Zeroizing;
const GRANDPA_KEY_TYPE_HEX_PREFIX: &str = "6772616e";
const INSTITUTION_CATALOG_SRC: &str = include_str!("institution-catalog.json");
use crate::shared::constants::RPC_RESPONSE_LIMIT_LARGE;
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
struct GrandpaPersistedStateBackup {
    meta: Option<StoredGrandpaMeta>,
    keystore_files: Vec<GrandpaKeystoreBackupEntry>,
}

#[derive(Debug, Clone)]
/// 节点运行目录条目；机构简称只允许由 `CHINA_CB.cid_short_name` 派生，
/// `institution-catalog.json` 不得再保存第二份名称。
pub(crate) struct InstitutionCatalogEntry {
    pub cid_short_name: String,
    pub role: String,
    pub peer_id: String,
    pub grandpa_public_key: String,
    pub domain: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StoredInstitutionCatalogEntry {
    /// 权威节点角色：`nrc` | `prc` | `prb`
    #[serde(default)]
    pub role: String,
    pub peer_id: String,
    #[serde(rename = "grandpa_public_key")]
    pub grandpa_public_key: String,
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

// 权威节点清单既被 bootnode 模块用于 PeerId 映射，也被 GRANDPA 模块用于公钥匹配，
// 加载时统一做 trim / 去重 / 格式校验，避免配置里的空格或脏数据影响运行时判断。
fn parse_institution_catalog() -> Result<Vec<InstitutionCatalogEntry>, String> {
    let entries: Vec<StoredInstitutionCatalogEntry> = serde_json::from_str(INSTITUTION_CATALOG_SRC)
        .map_err(|e| format!("parse institution-catalog.json failed: {e}"))?;
    if entries.is_empty() {
        return Err("institution-catalog.json 为空".to_string());
    }

    let mut seen_cid_short_names = HashSet::new();
    let mut seen_peer_ids = HashSet::new();
    let mut seen_grandpa = HashSet::new();
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

        let grandpa_public_key = crate::shared::validation::normalize_public_key(
            &entry.grandpa_public_key,
        )
        .map_err(|_| format!("institution-catalog.json 第 {line} 项 grandpa_public_key 无效"))?;
        if !seen_grandpa.insert(grandpa_public_key.clone()) {
            return Err(format!(
                "institution-catalog.json GRANDPA 公钥重复: {}",
                entry.grandpa_public_key
            ));
        }

        // 以创世 GRANDPA 公钥连接节点运行参数与内置机构；简称只取正式的
        // `cid_short_name`，目录顺序变化也不会造成机构名称错配。
        let institution = primitives::cid::china::china_cb::CHINA_CB
            .iter()
            .find(|institution| {
                format!("0x{}", hex::encode(institution.grandpa_key)) == grandpa_public_key
            })
            .ok_or_else(|| {
                format!("institution-catalog.json 第 {line} 项 GRANDPA 公钥未匹配 CHINA_CB")
            })?;
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
            grandpa_public_key,
            domain: entry.domain.trim().to_string(),
        });
    }

    Ok(normalized_entries)
}

fn grandpa_meta_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(security::app_data_dir(app)?.join("grandpa-meta.json"))
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

fn save_grandpa_meta(
    app: &AppHandle,
    cid_short_name: Option<String>,
    public_key: Option<String>,
) -> Result<(), String> {
    let raw = serde_json::to_string_pretty(&StoredGrandpaMeta {
        cid_short_name,
        public_key,
    })
    .map_err(|e| format!("encode grandpa meta failed: {e}"))?;
    security::write_text_atomic(&grandpa_meta_path(app)?, &format!("{raw}\n"))
        .map_err(|e| format!("write grandpa meta failed: {e}"))
}

fn clear_grandpa_meta(app: &AppHandle) -> Result<(), String> {
    match fs::remove_file(grandpa_meta_path(app)?) {
        Ok(_) => Ok(()),
        Err(err) if err.kind() == ErrorKind::NotFound => Ok(()),
        Err(err) => Err(format!("remove grandpa meta failed: {err}")),
    }
}

fn snapshot_grandpa_persisted_state(
    app: &AppHandle,
) -> Result<GrandpaPersistedStateBackup, String> {
    let meta = load_grandpa_meta(app)?;
    let dirs = keystore::keystore_dirs(app)?;
    let mut keystore_files = Vec::new();
    for path in keystore::scan_keystore_files(&dirs, GRANDPA_KEY_TYPE_HEX_PREFIX)? {
        let content = fs::read_to_string(&path).map_err(|e| {
            format!(
                "read grandpa keystore backup failed ({}): {e}",
                security::sanitize_path(&path)
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
    let dirs = keystore::keystore_dirs(app)?;
    for path in keystore::scan_keystore_files(&dirs, GRANDPA_KEY_TYPE_HEX_PREFIX)? {
        match fs::remove_file(&path) {
            Ok(_) => {}
            Err(err) if err.kind() == ErrorKind::NotFound => {}
            Err(err) => {
                return Err(format!(
                    "remove grandpa keystore file failed ({}): {err}",
                    security::sanitize_path(&path)
                ));
            }
        }
    }
    Ok(())
}

fn restore_grandpa_persisted_state(
    app: &AppHandle,
    backup: &GrandpaPersistedStateBackup,
) -> Result<(), String> {
    match &backup.meta {
        Some(meta) => save_grandpa_meta(app, meta.cid_short_name.clone(), meta.public_key.clone())?,
        None => clear_grandpa_meta(app)?,
    }

    remove_all_grandpa_keystore_files(app)?;
    for entry in &backup.keystore_files {
        security::write_secret_text_atomic(&entry.path, &entry.content).map_err(|e| {
            format!(
                "restore grandpa keystore file failed ({}): {e}",
                security::sanitize_path(&entry.path)
            )
        })?;
    }
    Ok(())
}

fn write_grandpa_key_to_keystore(
    app: &AppHandle,
    private_hex: &str,
    public_key: &str,
) -> Result<(), String> {
    let secret = Zeroizing::new(format!("0x{private_hex}"));
    let encoded = serde_json::to_string(&*secret)
        .map_err(|e| format!("encode grandpa keystore secret failed: {e}"))?;
    let content = Zeroizing::new(format!("{encoded}\n"));
    let dirs = keystore::keystore_dirs(app)?;
    // 初次配置只保留一把 gran 密钥；治理换钥必须改走 rotation 专用的双钥保留函数。
    keystore::write_key_to_keystore(
        &dirs,
        GRANDPA_KEY_TYPE_HEX_PREFIX,
        public_key.trim_start_matches("0x"),
        &content,
    )
}

/// 导入治理换钥候选私钥，同时保留旧 GRANDPA 私钥。
pub(crate) fn import_rotation_candidate(
    app: &AppHandle,
    private_key: &[u8; 32],
    public_key: &[u8; 32],
) -> Result<(), String> {
    let secret = Zeroizing::new(format!("0x{}", hex::encode(private_key)));
    let encoded = serde_json::to_string(&*secret)
        .map_err(|e| format!("encode grandpa rotation secret failed: {e}"))?;
    let content = Zeroizing::new(format!("{encoded}\n"));
    let dirs = keystore::keystore_dirs(app)?;
    keystore::write_key_to_keystore_preserving_others(
        &dirs,
        GRANDPA_KEY_TYPE_HEX_PREFIX,
        &hex::encode(public_key),
        &content,
    )
}

/// 使用本机准确公钥对应的 GRANDPA 私钥签名证明摘要。
pub(crate) fn sign_rotation_proof(
    app: &AppHandle,
    public_key: &[u8; 32],
    digest: &[u8; 32],
) -> Result<[u8; 64], String> {
    use ed25519_dalek::Signer;

    let dirs = keystore::keystore_dirs(app)?;
    let raw = keystore::read_key_from_keystore(
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
pub(crate) fn finalize_rotation_key(
    app: &AppHandle,
    cid_short_name: &str,
    old_public_key: &[u8; 32],
    new_public_key: &[u8; 32],
) -> Result<(), String> {
    let dirs = keystore::keystore_dirs(app)?;
    let new_public_key_hex = hex::encode(new_public_key);
    if !keystore::has_key_in_keystore(&dirs, GRANDPA_KEY_TYPE_HEX_PREFIX, &new_public_key_hex) {
        return Err(format!(
            "新 GRANDPA authority 已生效，但本机缺少新私钥（public_key=0x{new_public_key_hex}）"
        ));
    }
    keystore::remove_key_from_keystore(
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
pub(crate) fn discard_rotation_candidate(
    app: &AppHandle,
    new_public_key: &[u8; 32],
) -> Result<(), String> {
    let dirs = keystore::keystore_dirs(app)?;
    keystore::remove_key_from_keystore(
        &dirs,
        GRANDPA_KEY_TYPE_HEX_PREFIX,
        &hex::encode(new_public_key),
    )
}

/// 由机构 CID 定位正式机构简称，不依赖会被治理更换的 GRANDPA 公钥。
pub(crate) fn cid_short_name_for_cid(actor_cid_number: &str) -> Result<String, String> {
    let initial_public_key = primitives::cid::china::china_cb::CHINA_CB
        .iter()
        .find(|entry| entry.cid_number == actor_cid_number)
        .map(|entry| format!("0x{}", hex::encode(entry.grandpa_key)))
        .ok_or_else(|| "目标 CID 不是 NRC/PRC GRANDPA 权威机构".to_string())?;
    load_institution_catalog()?
        .into_iter()
        .find(|entry| {
            matches!(entry.role.as_str(), "nrc" | "prc")
                && entry
                    .grandpa_public_key
                    .eq_ignore_ascii_case(&initial_public_key)
        })
        .map(|entry| entry.cid_short_name)
        .ok_or_else(|| "institution-catalog 缺少该 CID 对应的 NRC/PRC 权威节点".to_string())
}

fn has_grandpa_key_in_keystore(app: &AppHandle, public_key: &str) -> Result<bool, String> {
    let dirs = keystore::keystore_dirs(app)?;
    Ok(keystore::has_key_in_keystore(
        &dirs,
        GRANDPA_KEY_TYPE_HEX_PREFIX,
        public_key.trim_start_matches("0x"),
    ))
}

fn grandpa_institution_options() -> Result<Vec<(String, String)>, String> {
    let out = load_institution_catalog()?
        .into_iter()
        .map(|entry| {
            (
                entry.cid_short_name,
                entry.grandpa_public_key.to_ascii_lowercase(),
            )
        })
        .collect::<Vec<(String, String)>>();
    if out.is_empty() {
        return Err("未配置 GRANDPA 权威公钥".to_string());
    }
    Ok(out)
}

fn cid_short_name_by_grandpa_public_key(public_key: &str) -> Result<Option<String>, String> {
    Ok(grandpa_institution_options()?
        .into_iter()
        .find(|(_, key)| key.eq_ignore_ascii_case(public_key))
        .map(|(cid_short_name, _)| cid_short_name))
}

fn grandpa_public_key_from_private_hex(key_hex: &str) -> Result<String, String> {
    let secret = decode_hex_32_strict(key_hex)
        .map_err(|_| "GRANDPA 私钥格式无效，应为 64 位十六进制".to_string())?;
    let signing = ed25519_dalek::SigningKey::from_bytes(&secret);
    let verify = signing.verifying_key();
    Ok(format!("0x{}", hex::encode(verify.to_bytes())))
}

fn rpc_post(method: &str, params: Value) -> Result<Value, String> {
    rpc::rpc_post(
        method,
        params,
        rpc::RPC_REQUEST_TIMEOUT,
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

pub(crate) fn prepare_grandpa_for_start(app: &AppHandle) -> Result<bool, String> {
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

pub(crate) fn verify_grandpa_after_start(app: &AppHandle) -> Result<(), String> {
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

#[tauri::command]
pub fn set_grandpa_key(
    app: AppHandle,
    key: String,
    unlock_password: String,
) -> Result<GrandpaKey, String> {
    if let Err(e) = security::append_audit_log(&app, "set_grandpa_key", "attempt") {
        eprintln!("[审计] set_grandpa_key attempt 日志写入失败: {e}");
    }
    let unlock = security::ensure_unlock_password(&unlock_password)?;
    device_password::verify_device_login_password(&app, unlock)?;
    let was_running = home::current_status(&app)?.running;
    let backup = snapshot_grandpa_persisted_state(&app)?;
    let normalized = normalize_grandpa_key(&key)?;
    let public_key = grandpa_public_key_from_private_hex(&normalized)?;
    let cid_short_name = cid_short_name_by_grandpa_public_key(&public_key)?.ok_or_else(|| {
        format!("私钥与任何权威节点 GRANDPA 公钥不匹配（推导公钥: {public_key}）")
    })?;

    let normalized = Zeroizing::new(normalized);
    let mut node_stopped_for_restart = false;
    let mut new_node_started = false;
    let apply_result = (|| -> Result<(), String> {
        save_grandpa_meta(&app, Some(cid_short_name.clone()), Some(public_key.clone()))?;
        write_grandpa_key_to_keystore(&app, &normalized, &public_key)?;

        // 若节点当前在运行，保存后立即重启以 authority 模式加载并参与投票。
        if was_running {
            let _ = home::stop_node_blocking(app.clone())?;
            node_stopped_for_restart = true;
            let _ = home::start_node_blocking(app.clone())?;
            new_node_started = true;
            node_stopped_for_restart = false;
            verify_grandpa_after_start(&app)?;
        }
        Ok(())
    })();
    if let Err(err) = apply_result {
        let process_was_interrupted = node_stopped_for_restart || new_node_started;
        if process_was_interrupted {
            let _ = home::stop_node_blocking(app.clone());
        }
        let restore_err = restore_grandpa_persisted_state(&app, &backup).err();
        let restart_restore_err = if was_running && process_was_interrupted && restore_err.is_none()
        {
            home::start_node_blocking(app.clone()).map(|_| ()).err()
        } else {
            None
        };
        if let Err(e) = security::append_audit_log(
            &app,
            "set_grandpa_key",
            if restore_err.is_some() || restart_restore_err.is_some() {
                "rollback_failed"
            } else {
                "rolled_back"
            },
        ) {
            eprintln!("[审计] set_grandpa_key rollback 日志写入失败: {e}");
        }

        let mut detail = format!("保存 GRANDPA 私钥后重启或校验失败：{err}");
        if let Some(restore_err) = restore_err {
            detail.push_str(&format!("；回滚旧配置失败：{restore_err}"));
        } else {
            detail.push_str("；已回滚到旧的元数据和 keystore");
        }
        if let Some(restart_restore_err) = restart_restore_err {
            detail.push_str(&format!(
                "；恢复旧配置后重新启动节点失败：{restart_restore_err}"
            ));
        }
        return Err(detail);
    }
    if let Err(e) = security::append_audit_log(&app, "set_grandpa_key", "success") {
        eprintln!("[审计] set_grandpa_key success 日志写入失败: {e}");
    }

    Ok(GrandpaKey {
        key: None,
        cid_short_name: Some(cid_short_name),
    })
}

#[cfg(test)]
mod tests {
    use super::{
        cid_short_name_for_cid, load_institution_catalog, StoredGrandpaMeta,
        INSTITUTION_CATALOG_SRC,
    };
    use primitives::cid::china::china_cb::CHINA_CB;
    use std::collections::HashSet;

    #[test]
    fn every_grandpa_cid_uses_runtime_cid_short_name_without_duplicate_catalog_name() {
        assert!(!INSTITUTION_CATALOG_SRC.contains(concat!("authority", "NodeLabel")));
        assert!(!INSTITUTION_CATALOG_SRC.contains("cidShortName"));
        let cid_short_names = CHINA_CB
            .iter()
            .map(|entry| {
                let actual = cid_short_name_for_cid(entry.cid_number)
                    .expect("每个 NRC/PRC CID 都必须匹配一个权威节点");
                assert_eq!(actual, entry.cid_short_name);
                actual
            })
            .collect::<HashSet<_>>();
        assert_eq!(cid_short_names.len(), CHINA_CB.len());
        assert_eq!(load_institution_catalog().unwrap().len(), CHINA_CB.len());
    }

    #[test]
    fn old_duplicate_name_field_is_rejected_instead_of_silently_accepted() {
        let old_field = concat!("authority", "NodeLabel");
        let raw = format!(r#"{{"{old_field}":"旧名称","public_key":null}}"#);
        assert!(serde_json::from_str::<StoredGrandpaMeta>(&raw).is_err());
    }
}
