use std::fs;
use std::path::PathBuf;

use tauri::AppHandle;
use tauri::Manager;

const GENESIS_STATE_ENV: &str = "CITIZENCHAIN_GENESIS_STATE_DIR";

const GENESIS_STATE_RESOURCE_DIR: &str = "genesis-state";

const DEFAULT_CHAIN_ID: &str = "citizenchain";

fn local_chain_db_dir(base_path: &std::path::Path) -> PathBuf {
    base_path.join("chains").join(DEFAULT_CHAIN_ID).join("db")
}

fn dir_has_entries(path: &std::path::Path) -> Result<bool, String> {
    let mut entries =
        fs::read_dir(path).map_err(|e| format!("读取目录失败({}): {e}", path.display()))?;
    Ok(entries
        .next()
        .transpose()
        .map_err(|e| format!("读取目录项失败({}): {e}", path.display()))?
        .is_some())
}

fn genesis_state_source(app: &AppHandle) -> Option<PathBuf> {
    if let Some(path) = std::env::var_os(GENESIS_STATE_ENV).map(PathBuf::from) {
        if path.is_dir() {
            return Some(path);
        }
    }
    app.path()
        .resource_dir()
        .ok()
        .map(|dir| dir.join(GENESIS_STATE_RESOURCE_DIR))
        .filter(|path| is_genesis_state_package(path) || has_genesis_state_marker(path))
}

fn is_genesis_state_package(path: &std::path::Path) -> bool {
    path.join("manifest.json").is_file()
        && path
            .join("chains")
            .join(DEFAULT_CHAIN_ID)
            .join("db")
            .is_dir()
}

fn has_genesis_state_marker(path: &std::path::Path) -> bool {
    path.join("manifest.json").exists() || path.join("chains").exists()
}

fn validate_genesis_state_manifest(source_root: &std::path::Path) -> Result<(), String> {
    let manifest = source_root.join("manifest.json");
    if !manifest.is_file() {
        return Err(format!(
            "创世链状态包缺少 manifest.json: {}",
            manifest.display()
        ));
    }
    let text = fs::read_to_string(&manifest)
        .map_err(|e| format!("读取创世状态包 manifest 失败({}): {e}", manifest.display()))?;
    let json: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| format!("解析创世状态包 manifest 失败({}): {e}", manifest.display()))?;
    if json.get("package_format").and_then(|v| v.as_str()) != Some("citizenchain-genesis-state") {
        return Err("创世状态包 manifest.package_format 无效".to_string());
    }
    if json.get("chain_id").and_then(|v| v.as_str()) != Some(DEFAULT_CHAIN_ID) {
        return Err("创世状态包 manifest.chain_id 无效".to_string());
    }
    Ok(())
}

fn copy_dir_recursive(source: &std::path::Path, target: &std::path::Path) -> Result<(), String> {
    fs::create_dir_all(target).map_err(|e| format!("创建目录失败({}): {e}", target.display()))?;
    for entry in
        fs::read_dir(source).map_err(|e| format!("读取目录失败({}): {e}", source.display()))?
    {
        let entry = entry.map_err(|e| format!("读取目录项失败({}): {e}", source.display()))?;
        let source_path = entry.path();
        let target_path = target.join(entry.file_name());
        let metadata = entry
            .metadata()
            .map_err(|e| format!("读取文件元数据失败({}): {e}", source_path.display()))?;
        if metadata.is_dir() {
            copy_dir_recursive(&source_path, &target_path)?;
        } else if metadata.is_file() {
            fs::copy(&source_path, &target_path).map_err(|e| {
                format!(
                    "复制创世状态文件失败({} -> {}): {e}",
                    source_path.display(),
                    target_path.display()
                )
            })?;
        }
    }
    Ok(())
}

pub(crate) fn install_genesis_state_if_available(
    app: &AppHandle,
    base_path: &std::path::Path,
) -> Result<(), String> {
    let target_db = local_chain_db_dir(base_path);
    if target_db.exists() {
        if dir_has_entries(&target_db)? {
            return Ok(());
        }
        fs::remove_dir_all(&target_db)
            .map_err(|e| format!("清理空创世数据库目录失败({}): {e}", target_db.display()))?;
    }
    let Some(source_root) = genesis_state_source(app) else {
        return Ok(());
    };
    let source_db = source_root.join("chains").join(DEFAULT_CHAIN_ID).join("db");
    if !source_db.is_dir() {
        return Err(format!(
            "创世链状态包缺少数据库目录: {}",
            source_db.display()
        ));
    }
    validate_genesis_state_manifest(&source_root)?;
    let target_chain_dir = base_path.join("chains").join(DEFAULT_CHAIN_ID);
    fs::create_dir_all(&target_chain_dir)
        .map_err(|e| format!("创建本地链目录失败({}): {e}", target_chain_dir.display()))?;

    // 只复制链数据库,不复制 keystore / network key。节点身份仍由本机生成和管理。
    let tmp_db = target_chain_dir.join("db.genesis-copying");
    if tmp_db.exists() {
        fs::remove_dir_all(&tmp_db)
            .map_err(|e| format!("清理临时创世数据库目录失败({}): {e}", tmp_db.display()))?;
    }
    copy_dir_recursive(&source_db, &tmp_db)?;
    fs::rename(&tmp_db, &target_db).map_err(|e| {
        let _ = fs::remove_dir_all(&tmp_db);
        format!(
            "安装创世状态数据库失败({} -> {}): {e}",
            tmp_db.display(),
            target_db.display()
        )
    })?;

    let source_manifest = source_root.join("manifest.json");
    if source_manifest.is_file() {
        let target_manifest = target_chain_dir.join("genesis-state-manifest.json");
        fs::copy(&source_manifest, &target_manifest).map_err(|e| {
            format!(
                "复制创世状态包 manifest 失败({} -> {}): {e}",
                source_manifest.display(),
                target_manifest.display()
            )
        })?;
    }
    eprintln!(
        "[节点] 已从创世链状态包初始化本地链数据库: {}",
        source_root.display()
    );
    Ok(())
}

pub(crate) fn needs_local_chain_initialization(
    base_path: &std::path::Path,
) -> Result<bool, String> {
    let target_db = local_chain_db_dir(base_path);
    if !target_db.exists() {
        return Ok(true);
    }
    dir_has_entries(&target_db).map(|has_entries| !has_entries)
}
