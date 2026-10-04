//! 清算行批次上链前的管理员解锁 Tauri 命令。
//!
//!
//! - `settlement` 目录负责本清算行交易的打包、签名、提交上链。
//! - 管理员解锁只服务清算行批次签名,不等同普通多签账户激活。

use tauri::AppHandle;

use crate::home;
use crate::transaction::offchain::types::{DecryptAdminRequestResult, DecryptedAdminInfo};

use super::admin_unlock::VerifyDecryptAdminInput;

#[tauri::command(rename_all = "snake_case")]
pub async fn build_decrypt_admin_request(
    app: AppHandle,
    signer_public_key: String,
    cid_number: String,
) -> Result<DecryptAdminRequestResult, String> {
    let status = home::current_status(&app)?;
    if !status.running {
        return Err("节点未运行".to_string());
    }
    tauri::async_runtime::spawn_blocking(move || {
        super::admin_unlock::build_decrypt_admin_request(&signer_public_key, &cid_number)
    })
    .await
    .map_err(|e| format!("build_decrypt_admin_request task failed:{e}"))?
}

#[tauri::command(rename_all = "snake_case")]
pub async fn verify_and_decrypt_admin(
    request_id: String,
    signer_public_key: String,
    expected_payload_hash: String,
    response_json: String,
) -> Result<DecryptedAdminInfo, String> {
    tauri::async_runtime::spawn_blocking(move || {
        super::admin_unlock::verify_and_decrypt_admin(VerifyDecryptAdminInput {
            request_id,
            signer_public_key,
            expected_payload_hash,
            response_json,
        })
    })
    .await
    .map_err(|e| format!("verify_and_decrypt_admin task failed:{e}"))?
}

#[tauri::command(rename_all = "snake_case")]
pub async fn list_decrypted_admins(cid_number: String) -> Result<Vec<DecryptedAdminInfo>, String> {
    Ok(super::admin_unlock::list_decrypted_admins(&cid_number))
}

#[tauri::command(rename_all = "snake_case")]
pub fn lock_decrypted_admin(signer_public_key: String) -> Result<(), String> {
    super::admin_unlock::lock_decrypted_admin(&signer_public_key)
}
