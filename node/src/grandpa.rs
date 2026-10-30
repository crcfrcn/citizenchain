use nodecontrol::grandpa::cid_short_name_by_grandpa_public_key;
use nodecontrol::grandpa::grandpa_public_key_from_private_hex;
use nodecontrol::grandpa::restore_grandpa_persisted_state;
use nodecontrol::grandpa::save_grandpa_meta;
use nodecontrol::grandpa::snapshot_grandpa_persisted_state;
use nodecontrol::grandpa::verify_grandpa_after_start;
use nodecontrol::grandpa::write_grandpa_key_to_keystore;
use nodecontrol::grandpa::GrandpaKey;
use nodedesktop::validation::normalize_grandpa_key;

use tauri::AppHandle;
use zeroize::Zeroizing;

#[tauri::command]
pub(crate) fn set_grandpa_key(
    app: AppHandle,
    key: String,
    unlock_password: String,
) -> Result<GrandpaKey, String> {
    if let Err(e) = nodedesktop::security::append_audit_log(&app, "set_grandpa_key", "attempt") {
        eprintln!("[审计] set_grandpa_key attempt 日志写入失败: {e}");
    }
    let unlock = nodedesktop::security::ensure_unlock_password(&unlock_password)?;
    nodecontrol::password::verify_device_login_password(&app, unlock)?;
    let was_running = nodecontrol::identity::current_status(&app)?.running;
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
            let _ = crate::lifecycle::stop_node_blocking(app.clone())?;
            node_stopped_for_restart = true;
            let _ = crate::lifecycle::start_node_blocking(app.clone())?;
            new_node_started = true;
            node_stopped_for_restart = false;
            verify_grandpa_after_start(&app)?;
        }
        Ok(())
    })();
    if let Err(err) = apply_result {
        let process_was_interrupted = node_stopped_for_restart || new_node_started;
        if process_was_interrupted {
            let _ = crate::lifecycle::stop_node_blocking(app.clone());
        }
        let restore_err = restore_grandpa_persisted_state(&app, &backup).err();
        let restart_restore_err = if was_running && process_was_interrupted && restore_err.is_none()
        {
            crate::lifecycle::start_node_blocking(app.clone())
                .map(|_| ())
                .err()
        } else {
            None
        };
        if let Err(e) = nodedesktop::security::append_audit_log(
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
    if let Err(e) = nodedesktop::security::append_audit_log(&app, "set_grandpa_key", "success") {
        eprintln!("[审计] set_grandpa_key success 日志写入失败: {e}");
    }

    Ok(GrandpaKey {
        key: None,
        cid_short_name: Some(cid_short_name),
    })
}
