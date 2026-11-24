//! 查询准确块PoW参数快照并提供升级准备和提交命令。

#![allow(clippy::too_many_arguments)]
use crate::types::ProposeUpgradeRequestResult;
use codec::{Decode, DecodeAll};
use nodeclient::signing::VoteSubmitResult;
use sp_crypto_hashing::twox_128;
use tauri::AppHandle;

fn pow_storage_key(name: &[u8]) -> String {
    let key = [
        twox_128(b"Difficulty").as_slice(),
        twox_128(name).as_slice(),
    ]
    .concat();
    format!("0x{}", hex::encode(key))
}

fn decode_pow_storage<T: Decode>(raw: &str, name: &str) -> Result<T, String> {
    let bytes = hex::decode(raw.trim_start_matches("0x"))
        .map_err(|e| format!("{name} 十六进制解码失败: {e}"))?;
    T::decode_all(&mut bytes.as_slice()).map_err(|_| format!("{name} SCALE 编码非规范或无效"))
}

/// 两项状态固定在同一个 finalized 哈希，不混读不同区块或把未知状态当作无待生效参数。
pub(crate) fn read_pow_snapshot_at<F>(
    finalized_hash: &str,
    mut read: F,
) -> Result<
    (
        difficulty::PowDifficultyParams,
        Option<difficulty::PendingPowDifficultyParams>,
    ),
    String,
>
where
    F: FnMut(&str, &str) -> Result<Option<String>, String>,
{
    let raw = read(&pow_storage_key(b"ActiveParams"), finalized_hash)?
        .ok_or("链上缺少 Difficulty::ActiveParams")?;
    let active: difficulty::PowDifficultyParams = decode_pow_storage(&raw, "PoW 生效参数")?;
    active.validate().map_err(str::to_string)?;
    let pending = read(&pow_storage_key(b"PendingParams"), finalized_hash)?
        .map(|raw| decode_pow_storage(&raw, "PoW 待生效参数"))
        .transpose()?;
    Ok((active, pending))
}

fn fetch_pow_snapshot() -> Result<
    (
        difficulty::PowDifficultyParams,
        Option<difficulty::PendingPowDifficultyParams>,
    ),
    String,
> {
    let hash = nodeclient::query::fetch_finalized_head()?;
    read_pow_snapshot_at(&hash, nodeclient::query::fetch_storage_at)
}

fn preflight_pow_params(params: difficulty::PowDifficultyParams) -> Result<(), String> {
    let (active, pending) = fetch_pow_snapshot()?;
    crate::call::validate_pow_params(params, active, pending.is_some())
}

#[tauri::command]
pub async fn get_pow_difficulty_params() -> Result<difficulty::PowDifficultyParams, String> {
    tauri::async_runtime::spawn_blocking(|| fetch_pow_snapshot().map(|(active, _)| active))
        .await
        .map_err(|e| format!("读取 PoW 参数任务失败: {e}"))?
}

/// 构建运行期协议升级提案签名请求 QR JSON（需要节点运行）。
#[tauri::command]
pub async fn build_propose_upgrade_request(
    app: AppHandle,
    signer_public_key: String,
    actor_cid_number: String,
    wasm_path: String,
    reason: String,
    pow_params: difficulty::PowDifficultyParams,
) -> Result<ProposeUpgradeRequestResult, String> {
    let status = nodecontrol::identity::current_status(&app)?;
    if !status.running {
        return Err("节点未运行，无法构建签名请求".to_string());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let reason = crate::call::normalize_reason(&reason)?;
        preflight_pow_params(pow_params)?;
        let sign_result = crate::signing::build_propose_runtime_upgrade_sign_request(
            &signer_public_key,
            &actor_cid_number,
            wasm_path.trim(),
            reason,
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
#[tauri::command]
pub async fn submit_propose_upgrade(
    app: AppHandle,
    request_id: String,
    expected_public_key: String,
    expected_payload_hash: String,
    actor_cid_number: String,
    wasm_path: String,
    reason: String,
    pow_params: difficulty::PowDifficultyParams,
    sign_nonce: u32,
    sign_block_number: u64,
    response_json: String,
) -> Result<VoteSubmitResult, String> {
    let status = nodecontrol::identity::current_status(&app)?;
    if !status.running {
        return Err("节点未运行，无法提交提案".to_string());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let reason = crate::call::normalize_reason(&reason)?;
        preflight_pow_params(pow_params)?;
        let call_data = crate::call::propose_runtime_upgrade_from_file(
            &actor_cid_number,
            wasm_path.trim(),
            reason,
            pow_params,
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
    .map_err(|e| format!("submit propose upgrade task failed: {e}"))?
}
