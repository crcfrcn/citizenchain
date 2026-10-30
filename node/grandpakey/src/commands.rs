use crate::request::build_request_sync;
use crate::request::build_sessions;
use crate::storage::load_pending;
use crate::storage::save_pending;
use crate::types::GrandpaKeyChangeRequest;
use crate::types::GrandpaKeyChangeStatus;

use tauri::AppHandle;

#[tauri::command]
pub async fn build_grandpa_key_change_request(
    app: AppHandle,
    actor_cid_number: String,
    signer_public_key: String,
    emergency_recovery: bool,
    unlock_password: String,
) -> Result<GrandpaKeyChangeRequest, String> {
    let unlock = nodedesktop::security::ensure_unlock_password(&unlock_password)?;
    nodecontrol::password::verify_device_login_password(&app, unlock)?;
    if !nodecontrol::identity::current_status(&app)?.running {
        return Err("节点未运行，无法构建 GRANDPA 换钥请求".to_string());
    }
    tauri::async_runtime::spawn_blocking(move || {
        build_request_sync(
            &app,
            &actor_cid_number,
            &signer_public_key,
            emergency_recovery,
        )
    })
    .await
    .map_err(|error| format!("构建 GRANDPA 换钥请求任务失败: {error}"))?
}

#[tauri::command]
pub async fn submit_grandpa_key_change(
    app: AppHandle,
    request_id: String,
    expected_public_key: String,
    expected_payload_hash: String,
    sign_nonce: u32,
    sign_block_number: u64,
    response_json: String,
) -> Result<nodeclient::signing::VoteSubmitResult, String> {
    if !nodecontrol::identity::current_status(&app)?.running {
        return Err("节点未运行，无法提交 GRANDPA 换钥交易".to_string());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let session = build_sessions()
            .lock()
            .map_err(|_| "GRANDPA 换钥 session 锁损坏".to_string())?
            .get(&request_id)
            .cloned()
            .ok_or_else(|| "GRANDPA 换钥 session 不存在或已过期".to_string())?;
        let result = nodeclient::signing::verify_and_submit(
            &request_id,
            &expected_public_key,
            &expected_payload_hash,
            &session.call_data,
            sign_nonce,
            sign_block_number,
            &response_json,
        )?;
        build_sessions()
            .lock()
            .map_err(|_| "GRANDPA 换钥 session 锁损坏".to_string())?
            .remove(&request_id);
        let mut pending =
            load_pending(&app)?.ok_or_else(|| "本机 GRANDPA 换钥状态缺失".to_string())?;
        pending.tx_hash = Some(result.tx_hash.clone());
        save_pending(&app, &pending)?;
        Ok(result)
    })
    .await
    .map_err(|error| format!("提交 GRANDPA 换钥交易任务失败: {error}"))?
}

#[tauri::command]
pub fn get_grandpa_key_change_status(app: AppHandle) -> Result<GrandpaKeyChangeStatus, String> {
    let Some(pending) = load_pending(&app)? else {
        return Ok(GrandpaKeyChangeStatus {
            pending: false,
            actor_cid_number: None,
            old_public_key: None,
            new_public_key: None,
            change_kind: None,
            tx_hash: None,
        });
    };
    Ok(GrandpaKeyChangeStatus {
        pending: true,
        actor_cid_number: Some(pending.actor_cid_number),
        old_public_key: Some(pending.old_public_key),
        new_public_key: Some(pending.new_public_key),
        change_kind: Some(
            match pending.change_kind {
                crate::types::LocalChangeKind::RoutineRotation => "routine_rotation",
                crate::types::LocalChangeKind::EmergencyRecovery => "emergency_recovery",
            }
            .to_string(),
        ),
        tx_hash: pending.tx_hash,
    })
}
