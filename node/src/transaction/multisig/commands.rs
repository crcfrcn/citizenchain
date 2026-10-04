//! 多签转账 Tauri 命令。

// Tauri IPC 参数对应扫码多签协议字段，必须保持既有逐字段调用契约。
#![allow(clippy::too_many_arguments)]

use crate::{governance, home};
use tauri::AppHandle;

/// 构建多签转账提案签名请求 QR JSON（需要节点运行）。
#[tauri::command(rename_all = "snake_case")]
pub async fn build_multisig_transfer_request(
    app: AppHandle,
    signer_public_key: String,
    actor_cid_number: String,
    proposer_role_code: String,
    institution_account_id: String,
    beneficiary_ss58_address: String,
    amount_yuan: f64,
    remark: String,
) -> Result<governance::signing::VoteSignRequestResult, String> {
    let status = home::current_status(&app)?;
    if !status.running {
        return Err("节点未运行，无法构建签名请求".to_string());
    }
    tauri::async_runtime::spawn_blocking(move || {
        super::signing::build_propose_transfer_sign_request(
            &signer_public_key,
            &actor_cid_number,
            &proposer_role_code,
            &institution_account_id,
            &beneficiary_ss58_address,
            amount_yuan,
            &remark,
        )
    })
    .await
    .map_err(|e| format!("build multisig transfer request task failed: {e}"))?
}

/// 验证签名响应并提交多签转账提案。
#[tauri::command(rename_all = "snake_case")]
pub async fn submit_multisig_transfer(
    app: AppHandle,
    request_id: String,
    expected_signer_public_key: String,
    expected_payload_hash: String,
    actor_cid_number: String,
    proposer_role_code: String,
    institution_account_id: String,
    beneficiary_ss58_address: String,
    amount_yuan: f64,
    remark: String,
    sign_nonce: u32,
    sign_block_number: u64,
    response_json: String,
) -> Result<governance::signing::VoteSubmitResult, String> {
    let status = home::current_status(&app)?;
    if !status.running {
        return Err("节点未运行，无法提交提案".to_string());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let amount_fen = (amount_yuan * 100.0).round() as u128;
        let beneficiary_bytes =
            governance::signing::account_id_from_ss58_address(&beneficiary_ss58_address)?;
        let remark_bytes = remark.as_bytes();
        let institution_account_id =
            super::account_id::institution_account_from_id(&institution_account_id)?;
        let call_data = super::signing::build_transfer_call_data(
            &actor_cid_number,
            &proposer_role_code,
            &institution_account_id,
            &beneficiary_bytes,
            amount_fen,
            remark_bytes,
        )?;

        governance::signing::verify_and_submit(
            &request_id,
            &expected_signer_public_key,
            &expected_payload_hash,
            &call_data,
            sign_nonce,
            sign_block_number,
            &response_json,
        )
    })
    .await
    .map_err(|e| format!("submit multisig transfer task failed: {e}"))?
}

/// 构建安全基金转账提案签名请求 QR JSON（需要节点运行）。
#[tauri::command(rename_all = "snake_case")]
pub async fn build_multisig_safety_fund_request(
    app: AppHandle,
    signer_public_key: String,
    actor_cid_number: String,
    proposer_role_code: String,
    institution_account_id: String,
    beneficiary_ss58_address: String,
    amount_yuan: f64,
    remark: String,
) -> Result<governance::signing::VoteSignRequestResult, String> {
    let status = home::current_status(&app)?;
    if !status.running {
        return Err("节点未运行，无法构建签名请求".to_string());
    }
    tauri::async_runtime::spawn_blocking(move || {
        super::signing::build_propose_safety_fund_sign_request(
            &signer_public_key,
            &actor_cid_number,
            &proposer_role_code,
            &institution_account_id,
            &beneficiary_ss58_address,
            amount_yuan,
            &remark,
        )
    })
    .await
    .map_err(|e| format!("build multisig safety fund request task failed: {e}"))?
}

/// 验证签名响应并提交安全基金转账提案。
#[tauri::command(rename_all = "snake_case")]
pub async fn submit_multisig_safety_fund(
    app: AppHandle,
    request_id: String,
    expected_signer_public_key: String,
    expected_payload_hash: String,
    actor_cid_number: String,
    proposer_role_code: String,
    institution_account_id: String,
    beneficiary_ss58_address: String,
    amount_yuan: f64,
    remark: String,
    sign_nonce: u32,
    sign_block_number: u64,
    response_json: String,
) -> Result<governance::signing::VoteSubmitResult, String> {
    let status = home::current_status(&app)?;
    if !status.running {
        return Err("节点未运行，无法提交提案".to_string());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let call_data = super::signing::build_safety_fund_call_data(
            &actor_cid_number,
            &proposer_role_code,
            &institution_account_id,
            &beneficiary_ss58_address,
            amount_yuan,
            &remark,
        )?;
        governance::signing::verify_and_submit(
            &request_id,
            &expected_signer_public_key,
            &expected_payload_hash,
            &call_data,
            sign_nonce,
            sign_block_number,
            &response_json,
        )
    })
    .await
    .map_err(|e| format!("submit multisig safety fund task failed: {e}"))?
}

/// 构建手续费划转提案签名请求。
#[tauri::command(rename_all = "snake_case")]
pub async fn build_multisig_sweep_request(
    app: AppHandle,
    signer_public_key: String,
    actor_cid_number: String,
    proposer_role_code: String,
    institution_account_id: String,
    amount_yuan: f64,
) -> Result<governance::signing::VoteSignRequestResult, String> {
    let status = home::current_status(&app)?;
    if !status.running {
        return Err("节点未运行".to_string());
    }
    tauri::async_runtime::spawn_blocking(move || {
        super::signing::build_propose_sweep_sign_request(
            &signer_public_key,
            &actor_cid_number,
            &proposer_role_code,
            &institution_account_id,
            amount_yuan,
        )
    })
    .await
    .map_err(|e| format!("build multisig sweep failed: {e}"))?
}

/// 验证签名并提交手续费划转提案。
#[tauri::command(rename_all = "snake_case")]
pub async fn submit_multisig_sweep(
    app: AppHandle,
    request_id: String,
    expected_signer_public_key: String,
    expected_payload_hash: String,
    actor_cid_number: String,
    proposer_role_code: String,
    institution_account_id: String,
    amount_yuan: f64,
    sign_nonce: u32,
    sign_block_number: u64,
    response_json: String,
) -> Result<governance::signing::VoteSubmitResult, String> {
    let status = home::current_status(&app)?;
    if !status.running {
        return Err("节点未运行".to_string());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let call_data = super::signing::build_sweep_call_data(
            &actor_cid_number,
            &proposer_role_code,
            &institution_account_id,
            amount_yuan,
        )?;
        governance::signing::verify_and_submit(
            &request_id,
            &expected_signer_public_key,
            &expected_payload_hash,
            &call_data,
            sign_nonce,
            sign_block_number,
            &response_json,
        )
    })
    .await
    .map_err(|e| format!("submit multisig sweep failed: {e}"))?
}
