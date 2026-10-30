use crate::types::ClearingBankNodeOnChainInfo;
use crate::types::ConnectivityTestReport;
use crate::types::DecryptAdminRequestResult;
use crate::types::DecryptedAdminInfo;
use crate::unlock::VerifyDecryptAdminInput;
use nodedesktop::constants::RPC_RESPONSE_LIMIT_SMALL;
use serde_json::Value;
use std::time::Duration;
use tauri::AppHandle;

const RPC_REQUEST_TIMEOUT: Duration = Duration::from_secs(3);

fn rpc_post(method: &str, params: Value) -> Result<Value, String> {
    nodedesktop::rpc::rpc_post(
        method,
        params,
        RPC_REQUEST_TIMEOUT,
        RPC_RESPONSE_LIMIT_SMALL,
    )
}

/// 链上查询某机构的清算行节点声明信息。返回 None = 该机构未声明节点。
#[tauri::command]
pub async fn query_clearing_bank_node_info(
    app: AppHandle,
    cid_number: String,
) -> Result<Option<ClearingBankNodeOnChainInfo>, String> {
    let status = nodecontrol::identity::current_status(&app)?;
    if !status.running {
        return Err("节点未运行,无法查询链上数据".to_string());
    }
    tauri::async_runtime::spawn_blocking(move || {
        crate::endpoint::fetch_clearing_bank_node(&cid_number)
    })
    .await
    .map_err(|e| format!("query_clearing_bank_node_info task failed:{e}"))?
}

/// 通过 RPC `system_localPeerId` 拿本机 libp2p PeerId。
#[tauri::command]
pub async fn query_local_peer_id(app: AppHandle) -> Result<String, String> {
    let status = nodecontrol::identity::current_status(&app)?;
    if !status.running {
        return Err("节点未运行,无法查询 PeerId".to_string());
    }
    tauri::async_runtime::spawn_blocking(|| {
        let v = rpc_post("system_localPeerId", Value::Array(vec![]))?;
        v.as_str()
            .map(str::to_string)
            .ok_or_else(|| "system_localPeerId 返回格式无效".to_string())
    })
    .await
    .map_err(|e| format!("query_local_peer_id task failed:{e}"))?
}

/// 用户填的对外 RPC 域名+端口连通性自测,提交注册前强制 all_ok 才允许签名。
#[tauri::command]
pub async fn test_clearing_bank_endpoint_connectivity(
    domain: String,
    port: u16,
    expected_peer_id: String,
) -> Result<ConnectivityTestReport, String> {
    tauri::async_runtime::spawn_blocking(move || {
        Ok::<ConnectivityTestReport, String>(crate::health::run_endpoint_connectivity_test(
            &domain,
            port,
            &expected_peer_id,
        ))
    })
    .await
    .map_err(|e| format!("connectivity test task failed:{e}"))?
}

#[tauri::command]
pub async fn build_register_clearing_bank_request(
    app: AppHandle,
    signer_public_key: String,
    actor_cid_number: String,
    peer_id: String,
    rpc_domain: String,
    rpc_port: u16,
) -> Result<nodeclient::signing::VoteSignRequestResult, String> {
    let status = nodecontrol::identity::current_status(&app)?;
    if !status.running {
        return Err("节点未运行,无法构建签名请求".to_string());
    }
    tauri::async_runtime::spawn_blocking(move || {
        crate::signing::build_register_sign_request(
            &signer_public_key,
            &actor_cid_number,
            &peer_id,
            &rpc_domain,
            rpc_port,
        )
    })
    .await
    .map_err(|e| format!("build_register_clearing_bank task failed:{e}"))?
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn submit_register_clearing_bank(
    app: AppHandle,
    request_id: String,
    expected_public_key: String,
    expected_payload_hash: String,
    actor_cid_number: String,
    peer_id: String,
    rpc_domain: String,
    rpc_port: u16,
    sign_nonce: u32,
    sign_block_number: u64,
    response_json: String,
) -> Result<nodeclient::signing::VoteSubmitResult, String> {
    let status = nodecontrol::identity::current_status(&app)?;
    if !status.running {
        return Err("节点未运行,无法提交交易".to_string());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let call_data = crate::signing::build_register_call_data(
            &actor_cid_number,
            &peer_id,
            &rpc_domain,
            rpc_port,
        )?;
        nodeclient::signing::verify_and_submit(
            &request_id,
            &expected_public_key,
            &expected_payload_hash,
            &call_data,
            sign_nonce,
            sign_block_number,
            &response_json,
        )
    })
    .await
    .map_err(|e| format!("submit_register_clearing_bank task failed:{e}"))?
}

#[tauri::command]
pub async fn build_update_clearing_bank_endpoint_request(
    app: AppHandle,
    signer_public_key: String,
    actor_cid_number: String,
    new_domain: String,
    new_port: u16,
) -> Result<nodeclient::signing::VoteSignRequestResult, String> {
    let status = nodecontrol::identity::current_status(&app)?;
    if !status.running {
        return Err("节点未运行".to_string());
    }
    tauri::async_runtime::spawn_blocking(move || {
        crate::signing::build_update_endpoint_sign_request(
            &signer_public_key,
            &actor_cid_number,
            &new_domain,
            new_port,
        )
    })
    .await
    .map_err(|e| format!("build_update_clearing_bank_endpoint task failed:{e}"))?
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn submit_update_clearing_bank_endpoint(
    app: AppHandle,
    request_id: String,
    expected_public_key: String,
    expected_payload_hash: String,
    actor_cid_number: String,
    new_domain: String,
    new_port: u16,
    sign_nonce: u32,
    sign_block_number: u64,
    response_json: String,
) -> Result<nodeclient::signing::VoteSubmitResult, String> {
    let status = nodecontrol::identity::current_status(&app)?;
    if !status.running {
        return Err("节点未运行".to_string());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let call_data = crate::signing::build_update_endpoint_call_data(
            &actor_cid_number,
            &new_domain,
            new_port,
        )?;
        nodeclient::signing::verify_and_submit(
            &request_id,
            &expected_public_key,
            &expected_payload_hash,
            &call_data,
            sign_nonce,
            sign_block_number,
            &response_json,
        )
    })
    .await
    .map_err(|e| format!("submit_update_clearing_bank_endpoint task failed:{e}"))?
}

#[tauri::command]
pub async fn build_unregister_clearing_bank_request(
    app: AppHandle,
    signer_public_key: String,
    actor_cid_number: String,
) -> Result<nodeclient::signing::VoteSignRequestResult, String> {
    let status = nodecontrol::identity::current_status(&app)?;
    if !status.running {
        return Err("节点未运行".to_string());
    }
    tauri::async_runtime::spawn_blocking(move || {
        crate::signing::build_unregister_sign_request(&signer_public_key, &actor_cid_number)
    })
    .await
    .map_err(|e| format!("build_unregister_clearing_bank task failed:{e}"))?
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn submit_unregister_clearing_bank(
    app: AppHandle,
    request_id: String,
    expected_public_key: String,
    expected_payload_hash: String,
    actor_cid_number: String,
    sign_nonce: u32,
    sign_block_number: u64,
    response_json: String,
) -> Result<nodeclient::signing::VoteSubmitResult, String> {
    let status = nodecontrol::identity::current_status(&app)?;
    if !status.running {
        return Err("节点未运行".to_string());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let call_data = crate::signing::build_unregister_call_data(&actor_cid_number)?;
        nodeclient::signing::verify_and_submit(
            &request_id,
            &expected_public_key,
            &expected_payload_hash,
            &call_data,
            sign_nonce,
            sign_block_number,
            &response_json,
        )
    })
    .await
    .map_err(|e| format!("submit_unregister_clearing_bank task failed:{e}"))?
}

#[tauri::command]
pub async fn build_decrypt_admin_request(
    app: AppHandle,
    signer_public_key: String,
    cid_number: String,
) -> Result<DecryptAdminRequestResult, String> {
    let status = nodecontrol::identity::current_status(&app)?;
    if !status.running {
        return Err("节点未运行".to_string());
    }
    tauri::async_runtime::spawn_blocking(move || {
        crate::unlock::build_decrypt_admin_request(&signer_public_key, &cid_number)
    })
    .await
    .map_err(|e| format!("build_decrypt_admin_request task failed:{e}"))?
}

#[tauri::command]
pub async fn verify_and_decrypt_admin(
    request_id: String,
    signer_public_key: String,
    expected_payload_hash: String,
    response_json: String,
) -> Result<DecryptedAdminInfo, String> {
    tauri::async_runtime::spawn_blocking(move || {
        crate::unlock::verify_and_decrypt_admin(VerifyDecryptAdminInput {
            request_id,
            signer_public_key,
            expected_payload_hash,
            response_json,
        })
    })
    .await
    .map_err(|e| format!("verify_and_decrypt_admin task failed:{e}"))?
}

#[tauri::command]
pub async fn list_decrypted_admins(cid_number: String) -> Result<Vec<DecryptedAdminInfo>, String> {
    Ok(crate::unlock::list_decrypted_admins(&cid_number))
}

#[tauri::command]
pub fn lock_decrypted_admin(signer_public_key: String) -> Result<(), String> {
    crate::unlock::lock_decrypted_admin(&signer_public_key)
}
