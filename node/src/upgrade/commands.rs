// Tauri IPC 参数由既有前端签名协议逐字段传入，合并参数会改变命令契约。
#![allow(clippy::too_many_arguments)]

use super::{call_data, signing as runtime_signing, types::ProposeUpgradeRequestResult};
use crate::{
    governance::signing::{self, VoteSubmitResult},
    home,
};
use codec::Decode;
use serde_json::Value;
use sp_crypto_hashing::twox_128;
use tauri::AppHandle;

#[tauri::command(rename_all = "snake_case")]
pub fn get_pow_difficulty_params() -> Result<pow_difficulty::PowDifficultyParams, String> {
    let key = [
        twox_128(b"PowDifficulty").as_slice(),
        twox_128(b"ActiveParams").as_slice(),
    ]
    .concat();
    let value = signing::rpc_post(
        "state_getStorage",
        Value::Array(vec![Value::String(format!("0x{}", hex::encode(key)))]),
    )?;
    let raw_hex = value
        .as_str()
        .ok_or("链上缺少 PowDifficulty::ActiveParams")?;
    let raw = hex::decode(raw_hex.trim_start_matches("0x"))
        .map_err(|e| format!("PoW 参数十六进制解码失败: {e}"))?;
    let mut input = raw.as_slice();
    let params = pow_difficulty::PowDifficultyParams::decode(&mut input)
        .map_err(|_| "PoW 参数 SCALE 解码失败".to_string())?;
    if !input.is_empty() || params.validate().is_err() {
        return Err("PoW 参数非规范或无效".to_string());
    }
    Ok(params)
}

/// 构建运行期协议升级提案签名请求 QR JSON（需要节点运行）。
#[tauri::command(rename_all = "snake_case")]
pub async fn build_propose_upgrade_request(
    app: AppHandle,
    signer_public_key: String,
    actor_cid_number: String,
    wasm_path: String,
    reason: String,
    pow_params: pow_difficulty::PowDifficultyParams,
) -> Result<ProposeUpgradeRequestResult, String> {
    let status = home::current_status(&app)?;
    if !status.running {
        return Err("节点未运行，无法构建签名请求".to_string());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let sign_result = runtime_signing::build_propose_runtime_upgrade_sign_request(
            &signer_public_key,
            &actor_cid_number,
            &wasm_path,
            &reason,
            pow_params,
        )?;
        Ok(ProposeUpgradeRequestResult {
            request_json: sign_result.request_json,
            request_id: sign_result.request_id,
            expected_payload_hash: sign_result.expected_payload_hash,
            sign_nonce: sign_result.sign_nonce,
            sign_block_number: sign_result.sign_block_number,
        })
    })
    .await
    .map_err(|e| format!("build propose upgrade request task failed: {e}"))?
}

/// 验证签名响应并提交运行期协议升级提案。
#[tauri::command(rename_all = "snake_case")]
pub async fn submit_propose_upgrade(
    app: AppHandle,
    request_id: String,
    expected_signer_public_key: String,
    expected_payload_hash: String,
    actor_cid_number: String,
    wasm_path: String,
    reason: String,
    pow_params: pow_difficulty::PowDifficultyParams,
    sign_nonce: u32,
    sign_block_number: u64,
    response_json: String,
) -> Result<VoteSubmitResult, String> {
    let status = home::current_status(&app)?;
    if !status.running {
        return Err("节点未运行，无法提交提案".to_string());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let call_data = call_data::propose_runtime_upgrade_from_file(
            &actor_cid_number,
            &wasm_path,
            &reason,
            pow_params,
        )?;
        signing::verify_and_submit(
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
    .map_err(|e| format!("submit propose upgrade task failed: {e}"))?
}
