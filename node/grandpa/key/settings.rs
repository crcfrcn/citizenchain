//! 为节点启动配置所属GRANDPA权威密钥。

use crate::key::local::cid_short_name_by_grandpa_public_key;
use crate::key::local::grandpa_public_key_from_private_hex;
use crate::key::local::restore_grandpa_persisted_state;
use crate::key::local::save_grandpa_meta;
use crate::key::local::snapshot_grandpa_persisted_state;
use crate::key::local::verify_grandpa_after_start;
use crate::key::local::write_grandpa_key_to_keystore;
use crate::key::local::GrandpaKey;
use nodedesktop::validation::normalize_grandpa_key;

use tauri::AppHandle;
use zeroize::Zeroizing;

#[tauri::command]
/// 保存本机密钥并沿用原重启、校验和失败回滚流程。
pub fn set_grandpa_key(
    app: AppHandle,
    key: String,
    unlock_password: String,
) -> Result<GrandpaKey, String> {
    if let Err(e) = nodedesktop::security::append_audit_log(&app, "set_grandpa_key", "attempt") {
        eprintln!("[审计] set_grandpa_key attempt 日志写入失败: {e}");
    }
    let unlock = nodedesktop::security::ensure_unlock_password(&unlock_password)?;
    let host = crate::key::Host::get(&app)?;
    (host.verify_password)(&app, unlock)?;
    let was_running = (host.is_running)(&app)?;
    let backup = snapshot_grandpa_persisted_state(&app)?;
    let normalized = normalize_grandpa_key(&key)?;
    let public_key = grandpa_public_key_from_private_hex(&normalized)?;
    let cid_short_name = cid_short_name_by_grandpa_public_key(&public_key)?.ok_or_else(|| {
        format!("私钥与任何权威节点 GRANDPA 公钥不匹配（推导公钥: {public_key}）")
    })?;

    let normalized = Zeroizing::new(normalized);
    let result = apply_change(
        was_running,
        || {
            save_grandpa_meta(&app, Some(cid_short_name.clone()), Some(public_key.clone()))?;
            write_grandpa_key_to_keystore(&app, &normalized, &public_key)
        },
        || (host.stop)(app.clone()),
        || (host.start)(app.clone()),
        || verify_grandpa_after_start(&app),
        || restore_grandpa_persisted_state(&app, &backup),
    );
    if let Err(error) = result {
        if let Err(e) = nodedesktop::security::append_audit_log(
            &app,
            "set_grandpa_key",
            if error.rollback_failed {
                "rollback_failed"
            } else {
                "rolled_back"
            },
        ) {
            eprintln!("[审计] set_grandpa_key rollback 日志写入失败: {e}");
        }
        return Err(error.detail);
    }
    if let Err(e) = nodedesktop::security::append_audit_log(&app, "set_grandpa_key", "success") {
        eprintln!("[审计] set_grandpa_key success 日志写入失败: {e}");
    }

    Ok(GrandpaKey {
        key: None,
        cid_short_name: Some(cid_short_name),
    })
}

/// 保留原设置流程的回滚结果，供命令按原审计口径报告。
#[derive(Debug)]
pub(crate) struct ApplyError {
    pub(crate) detail: String,
    pub(crate) rollback_failed: bool,
}

/// 只组织原有保存、启停、校验和恢复调用；不拥有节点生命周期或密钥存储实现。
pub(crate) fn apply_change(
    was_running: bool,
    save: impl FnOnce() -> Result<(), String>,
    mut stop: impl FnMut() -> Result<(), String>,
    mut start: impl FnMut() -> Result<(), String>,
    verify: impl FnOnce() -> Result<(), String>,
    restore: impl FnOnce() -> Result<(), String>,
) -> Result<(), ApplyError> {
    let mut node_stopped_for_restart = false;
    let mut new_node_started = false;
    let apply_result = (|| -> Result<(), String> {
        save()?;
        if was_running {
            stop()?;
            node_stopped_for_restart = true;
            start()?;
            new_node_started = true;
            node_stopped_for_restart = false;
            verify()?;
        }
        Ok(())
    })();
    if let Err(err) = apply_result {
        let process_was_interrupted = node_stopped_for_restart || new_node_started;
        if process_was_interrupted {
            let _ = stop();
        }
        let restore_err = restore().err();
        let restart_restore_err = if was_running && process_was_interrupted && restore_err.is_none()
        {
            start().err()
        } else {
            None
        };
        let rollback_failed = restore_err.is_some() || restart_restore_err.is_some();
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
        return Err(ApplyError {
            detail,
            rollback_failed,
        });
    }
    Ok(())
}
