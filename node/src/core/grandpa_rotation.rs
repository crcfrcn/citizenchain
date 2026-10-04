//! GRANDPA 验证密钥正常更换与紧急恢复的节点侧安全编排。
//!
//! 本模块生成新 ed25519 私钥、构造 Runtime 持钥证明与冷签交易，并在本机 keystore
//! 同时保留旧、新私钥。只有 finalized 状态同时确认机构当前公钥和 GRANDPA authority
//! 已切到新公钥后，后台监视器才删除旧私钥并重启节点。

use crate::{
    governance::{chain_query, signing, storage_keys},
    home,
    settings::{device_password, grandpa_address},
    shared::security,
};
use codec::{Decode, Encode};
use ed25519_dalek::Signer;
use grandpakey_change::{GrandpaKeyChangeKind, GrandpaKeyProofPayload};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sp_core::H256;
use sp_runtime::AccountId32;
use std::{
    collections::HashMap,
    fs,
    io::ErrorKind,
    sync::{Mutex, OnceLock},
    thread,
    time::Duration,
};
use tauri::AppHandle;

const COMMITTEE_ROLE_CODE: &[u8] = primitives::governance_skeleton::ROLE_CODE_COMMITTEE_MEMBER;
const PROOF_LIFETIME_BLOCKS: u64 = 60;
const MONITOR_INTERVAL: Duration = Duration::from_secs(5);
const PENDING_STATE_FILE: &str = "grandpa-key-change.json";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum LocalChangeKind {
    RoutineRotation,
    EmergencyRecovery,
}

impl From<LocalChangeKind> for GrandpaKeyChangeKind {
    fn from(value: LocalChangeKind) -> Self {
        match value {
            LocalChangeKind::RoutineRotation => Self::RoutineRotation,
            LocalChangeKind::EmergencyRecovery => Self::EmergencyRecovery,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PendingKeyChange {
    actor_cid_number: String,
    cid_short_name: String,
    old_public_key: String,
    new_public_key: String,
    change_kind: LocalChangeKind,
    proof_nonce: u64,
    proof_expires_at: u32,
    tx_hash: Option<String>,
}

#[derive(Debug, Clone)]
struct BuildSession {
    call_data: Vec<u8>,
}

static BUILD_SESSIONS: OnceLock<Mutex<HashMap<String, BuildSession>>> = OnceLock::new();

fn build_sessions() -> &'static Mutex<HashMap<String, BuildSession>> {
    BUILD_SESSIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GrandpaKeyChangeRequest {
    pub request_json: String,
    pub request_id: String,
    pub expected_payload_hash: String,
    pub sign_nonce: u32,
    pub sign_block_number: u64,
    pub old_public_key: String,
    pub new_public_key: String,
    pub proof_nonce: u64,
    pub proof_expires_at: u32,
    pub change_kind: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GrandpaKeyChangeStatus {
    pub pending: bool,
    pub actor_cid_number: Option<String>,
    pub old_public_key: Option<String>,
    pub new_public_key: Option<String>,
    pub change_kind: Option<String>,
    pub tx_hash: Option<String>,
}

fn pending_state_path(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    Ok(security::app_data_dir(app)?.join(PENDING_STATE_FILE))
}

fn load_pending(app: &AppHandle) -> Result<Option<PendingKeyChange>, String> {
    let path = pending_state_path(app)?;
    let raw = match fs::read_to_string(&path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("读取 GRANDPA 换钥状态失败: {error}")),
    };
    serde_json::from_str(&raw)
        .map(Some)
        .map_err(|error| format!("解析 GRANDPA 换钥状态失败: {error}"))
}

fn save_pending(app: &AppHandle, pending: &PendingKeyChange) -> Result<(), String> {
    let raw = serde_json::to_string_pretty(pending)
        .map_err(|error| format!("编码 GRANDPA 换钥状态失败: {error}"))?;
    security::write_text_atomic(&pending_state_path(app)?, &format!("{raw}\n"))
        .map_err(|error| format!("保存 GRANDPA 换钥状态失败: {error}"))
}

fn clear_pending(app: &AppHandle) -> Result<(), String> {
    match fs::remove_file(pending_state_path(app)?) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("删除 GRANDPA 换钥状态失败: {error}")),
    }
}

fn decode_hex_32(value: &str, field: &str) -> Result<[u8; 32], String> {
    let bytes = hex::decode(value.trim_start_matches("0x"))
        .map_err(|error| format!("{field} 十六进制无效: {error}"))?;
    bytes
        .try_into()
        .map_err(|_| format!("{field} 必须为 32 字节"))
}

fn finalized_context() -> Result<(String, u64), String> {
    let finalized_hash = chain_query::fetch_finalized_head()?;
    let header = signing::rpc_post(
        "chain_getHeader",
        Value::Array(vec![Value::String(finalized_hash.clone())]),
    )?;
    let number_hex = header
        .get("number")
        .and_then(Value::as_str)
        .ok_or_else(|| "chain_getHeader(finalized).number 缺失".to_string())?;
    let finalized_number = u64::from_str_radix(number_hex.trim_start_matches("0x"), 16)
        .map_err(|error| format!("finalized block number 无效: {error}"))?;
    Ok((finalized_hash, finalized_number))
}

fn cid_scale(actor_cid_number: &str) -> Result<Vec<u8>, String> {
    if actor_cid_number.is_empty()
        || actor_cid_number.len() > primitives::core_const::CID_NUMBER_MAX_BYTES as usize
    {
        return Err("actor_cid_number 超出链上 CID 范围".to_string());
    }
    Ok(actor_cid_number.as_bytes().to_vec().encode())
}

fn fetch_current_public_key(
    actor_cid_number: &str,
    finalized_hash: &str,
) -> Result<[u8; 32], String> {
    let key = storage_keys::map_key(
        "GrandpaKeyChange",
        "CurrentGrandpaKeys",
        &cid_scale(actor_cid_number)?,
    );
    let raw = chain_query::fetch_storage_at(&key, finalized_hash)?
        .ok_or_else(|| "链上未找到目标机构 GRANDPA 公钥".to_string())?;
    let bytes = hex::decode(raw.trim_start_matches("0x"))
        .map_err(|error| format!("解码当前 GRANDPA 公钥失败: {error}"))?;
    <[u8; 32]>::decode(&mut bytes.as_slice())
        .map_err(|_| "当前 GRANDPA 公钥 SCALE 数据无效".to_string())
}

fn fetch_proof_nonce(actor_cid_number: &str, finalized_hash: &str) -> Result<u64, String> {
    let key = storage_keys::map_key(
        "GrandpaKeyChange",
        "NextGrandpaKeyProofNonce",
        &cid_scale(actor_cid_number)?,
    );
    let Some(raw) = chain_query::fetch_storage_at(&key, finalized_hash)? else {
        return Ok(0);
    };
    let bytes = hex::decode(raw.trim_start_matches("0x"))
        .map_err(|error| format!("解码 GRANDPA proof nonce 失败: {error}"))?;
    u64::decode(&mut bytes.as_slice()).map_err(|_| "GRANDPA proof nonce SCALE 数据无效".to_string())
}

fn fetch_current_set_id(finalized_hash: &str) -> Result<u64, String> {
    let key = storage_keys::value_key("Grandpa", "CurrentSetId");
    let Some(raw) = chain_query::fetch_storage_at(&key, finalized_hash)? else {
        return Ok(0);
    };
    let bytes = hex::decode(raw.trim_start_matches("0x"))
        .map_err(|error| format!("解码 GRANDPA set id 失败: {error}"))?;
    u64::decode(&mut bytes.as_slice()).map_err(|_| "GRANDPA set id SCALE 数据无效".to_string())
}

fn fetch_authorities(finalized_hash: &str) -> Result<sp_consensus_grandpa::AuthorityList, String> {
    let key = storage_keys::value_key("Grandpa", "Authorities");
    let raw = chain_query::fetch_storage_at(&key, finalized_hash)?
        .ok_or_else(|| "链上 GRANDPA authority set 不存在".to_string())?;
    let bytes = hex::decode(raw.trim_start_matches("0x"))
        .map_err(|error| format!("解码 GRANDPA authority set 失败: {error}"))?;
    sp_consensus_grandpa::AuthorityList::decode(&mut bytes.as_slice())
        .map_err(|_| "GRANDPA authority set SCALE 数据无效".to_string())
}

fn signer_account_id(signer_public_key: &str) -> Result<AccountId32, String> {
    let normalized = crate::shared::validation::normalize_public_key(signer_public_key)?;
    Ok(AccountId32::new(decode_hex_32(
        &normalized,
        "signer_public_key",
    )?))
}

fn build_runtime_call(
    actor_cid_number: &str,
    new_public_key: [u8; 32],
    proof_nonce: u64,
    proof_expires_at: u32,
    new_public_key_signature: [u8; 64],
    old_public_key_signature: Option<[u8; 64]>,
) -> Result<Vec<u8>, String> {
    let actor_cid_number = actor_cid_number
        .as_bytes()
        .to_vec()
        .try_into()
        .map_err(|_| "actor_cid_number 超出链上 CID 范围".to_string())?;
    let actor_role_code = COMMITTEE_ROLE_CODE
        .to_vec()
        .try_into()
        .map_err(|_| "委员岗位码超出链上范围".to_string())?;
    let call = match old_public_key_signature {
        Some(old_public_key_signature) => {
            citizenchain::RuntimeCall::GrandpaKeyChange(grandpakey_change::Call::<
                citizenchain::Runtime,
            >::schedule_grandpa_key_rotation {
                actor_cid_number,
                actor_role_code,
                new_public_key,
                proof_nonce,
                proof_expires_at,
                old_public_key_signature,
                new_public_key_signature,
            })
        }
        None => {
            citizenchain::RuntimeCall::GrandpaKeyChange(grandpakey_change::Call::<
                citizenchain::Runtime,
            >::propose_emergency_grandpa_key_recovery {
                actor_cid_number,
                actor_role_code,
                new_public_key,
                proof_nonce,
                proof_expires_at,
                new_public_key_signature,
            })
        }
    };
    Ok(call.encode())
}

fn build_request_sync(
    app: &AppHandle,
    actor_cid_number: &str,
    signer_public_key: &str,
    emergency_recovery: bool,
) -> Result<GrandpaKeyChangeRequest, String> {
    if load_pending(app)?.is_some() {
        return Err("本机已有尚未完成的 GRANDPA 换钥流程".to_string());
    }
    let cid_short_name = grandpa_address::cid_short_name_for_cid(actor_cid_number)?;
    let (finalized_hash, finalized_number) = finalized_context()?;
    let genesis_hash = H256::from(signing::fetch_genesis_hash()?);
    let old_public_key = fetch_current_public_key(actor_cid_number, &finalized_hash)?;
    let current_set_id = fetch_current_set_id(&finalized_hash)?;
    let proof_nonce = fetch_proof_nonce(actor_cid_number, &finalized_hash)?;
    let proof_expires_at = u32::try_from(finalized_number)
        .map_err(|_| "finalized block number 超出 Runtime u32 范围".to_string())?
        .checked_add(PROOF_LIFETIME_BLOCKS as u32)
        .ok_or_else(|| "GRANDPA proof_expires_at 溢出".to_string())?;
    let initiator_account_id = signer_account_id(signer_public_key)?;
    let actor_cid = actor_cid_number
        .as_bytes()
        .to_vec()
        .try_into()
        .map_err(|_| "actor_cid_number 超出链上 CID 范围".to_string())?;
    let role_code = COMMITTEE_ROLE_CODE
        .to_vec()
        .try_into()
        .map_err(|_| "委员岗位码超出链上范围".to_string())?;

    let new_private_key = rand::random::<[u8; 32]>();
    let new_signing_key = ed25519_dalek::SigningKey::from_bytes(&new_private_key);
    let new_public_key = new_signing_key.verifying_key().to_bytes();
    let local_kind = if emergency_recovery {
        LocalChangeKind::EmergencyRecovery
    } else {
        LocalChangeKind::RoutineRotation
    };
    let proof_payload = GrandpaKeyProofPayload {
        genesis_hash,
        actor_cid_number: actor_cid,
        actor_role_code: role_code,
        initiator_account_id,
        old_public_key,
        new_public_key,
        current_set_id,
        proof_nonce,
        proof_expires_at,
        change_kind: local_kind.into(),
    };
    let digest = grandpakey_change::proof_signing_digest(&proof_payload);
    let new_public_key_signature = new_signing_key.sign(&digest).to_bytes();
    let old_public_key_signature = if emergency_recovery {
        None
    } else {
        Some(grandpa_address::sign_rotation_proof(
            app,
            &old_public_key,
            &digest,
        )?)
    };
    let call_data = build_runtime_call(
        actor_cid_number,
        new_public_key,
        proof_nonce,
        proof_expires_at,
        new_public_key_signature,
        old_public_key_signature,
    )?;
    let normalized_signer = crate::shared::validation::normalize_public_key(signer_public_key)?;
    let signer_bytes = decode_hex_32(&normalized_signer, "signer_public_key")?;
    let sign_request =
        signing::build_sign_request_from_call_data(&normalized_signer, &signer_bytes, &call_data)?;

    let pending = PendingKeyChange {
        actor_cid_number: actor_cid_number.to_string(),
        cid_short_name,
        old_public_key: format!("0x{}", hex::encode(old_public_key)),
        new_public_key: format!("0x{}", hex::encode(new_public_key)),
        change_kind: local_kind,
        proof_nonce,
        proof_expires_at,
        tx_hash: None,
    };
    // 先持久化不含私钥的恢复状态，再写候选私钥；这样即使进程在两步之间退出，
    // 后台监视器仍能在证明过期后收口状态，不会留下无人管理的额外 gran 私钥。
    save_pending(app, &pending)?;
    if let Err(error) =
        grandpa_address::import_rotation_candidate(app, &new_private_key, &new_public_key)
    {
        let _ = clear_pending(app);
        return Err(error);
    }
    build_sessions()
        .lock()
        .map_err(|_| "GRANDPA 换钥 session 锁损坏".to_string())?
        .insert(
            sign_request.request_id.clone(),
            BuildSession {
                call_data: call_data.clone(),
            },
        );

    Ok(GrandpaKeyChangeRequest {
        request_json: sign_request.request_json,
        request_id: sign_request.request_id,
        expected_payload_hash: sign_request.expected_payload_hash,
        sign_nonce: sign_request.sign_nonce,
        sign_block_number: sign_request.sign_block_number,
        old_public_key: pending.old_public_key,
        new_public_key: pending.new_public_key,
        proof_nonce,
        proof_expires_at,
        change_kind: match local_kind {
            LocalChangeKind::RoutineRotation => "routine_rotation",
            LocalChangeKind::EmergencyRecovery => "emergency_recovery",
        }
        .to_string(),
    })
}

#[tauri::command(rename_all = "snake_case")]
pub async fn build_grandpa_key_change_request(
    app: AppHandle,
    actor_cid_number: String,
    signer_public_key: String,
    emergency_recovery: bool,
    unlock_password: String,
) -> Result<GrandpaKeyChangeRequest, String> {
    let unlock = security::ensure_unlock_password(&unlock_password)?;
    device_password::verify_device_login_password(&app, unlock)?;
    if !home::current_status(&app)?.running {
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

#[tauri::command(rename_all = "snake_case")]
pub async fn submit_grandpa_key_change(
    app: AppHandle,
    request_id: String,
    expected_signer_public_key: String,
    expected_payload_hash: String,
    sign_nonce: u32,
    sign_block_number: u64,
    response_json: String,
) -> Result<signing::VoteSubmitResult, String> {
    if !home::current_status(&app)?.running {
        return Err("节点未运行，无法提交 GRANDPA 换钥交易".to_string());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let session = build_sessions()
            .lock()
            .map_err(|_| "GRANDPA 换钥 session 锁损坏".to_string())?
            .get(&request_id)
            .cloned()
            .ok_or_else(|| "GRANDPA 换钥 session 不存在或已过期".to_string())?;
        let result = signing::verify_and_submit(
            &request_id,
            &expected_signer_public_key,
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
                LocalChangeKind::RoutineRotation => "routine_rotation",
                LocalChangeKind::EmergencyRecovery => "emergency_recovery",
            }
            .to_string(),
        ),
        tx_hash: pending.tx_hash,
    })
}

fn monitor_once(app: &AppHandle) -> Result<(), String> {
    let Some(pending) = load_pending(app)? else {
        return Ok(());
    };
    let (finalized_hash, finalized_number) = finalized_context()?;
    let old_public_key = decode_hex_32(&pending.old_public_key, "old_public_key")?;
    let new_public_key = decode_hex_32(&pending.new_public_key, "new_public_key")?;
    let current_public_key = fetch_current_public_key(&pending.actor_cid_number, &finalized_hash)?;
    let authorities = fetch_authorities(&finalized_hash)?;
    let old_authority =
        sp_consensus_grandpa::AuthorityId::from(sp_core::ed25519::Public::from_raw(old_public_key));
    let new_authority =
        sp_consensus_grandpa::AuthorityId::from(sp_core::ed25519::Public::from_raw(new_public_key));
    let old_is_active = authorities
        .iter()
        .any(|(authority, _)| authority == &old_authority);
    let new_is_active = authorities
        .iter()
        .any(|(authority, _)| authority == &new_authority);

    if current_public_key == new_public_key && new_is_active && !old_is_active {
        grandpa_address::finalize_rotation_key(
            app,
            &pending.cid_short_name,
            &old_public_key,
            &new_public_key,
        )?;
        clear_pending(app)?;
        if home::current_status(app)?.running {
            let _ = home::stop_node_blocking(app.clone())?;
            let _ = home::start_node_blocking(app.clone())?;
        }
        eprintln!(
            "[GRANDPA 换钥] finalized 已确认新 authority 0x{}，旧私钥已自动删除",
            hex::encode(new_public_key)
        );
    } else if pending.tx_hash.is_none() && finalized_number > u64::from(pending.proof_expires_at) {
        // 用户未提交且证明窗口已过期时清理候选新私钥；已提交的内部投票可能持续更久，
        // 不能按证明提交期限误删仍待治理通过的新私钥。
        grandpa_address::discard_rotation_candidate(app, &new_public_key)?;
        clear_pending(app)?;
    }
    Ok(())
}

/// 桌面应用启动后持续监视 finalized 状态，确保重启后仍能完成旧私钥清理。
pub(crate) fn start_monitor(app: AppHandle) {
    if let Err(error) = thread::Builder::new()
        .name("grandpa-key-rotation-monitor".into())
        .spawn(move || loop {
            if let Err(error) = monitor_once(&app) {
                eprintln!("[GRANDPA 换钥] finalized 监视暂时失败: {error}");
            }
            thread::sleep(MONITOR_INTERVAL);
        })
    {
        eprintln!("[GRANDPA 换钥] 启动 finalized 监视线程失败: {error}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use codec::DecodeAll;

    #[test]
    fn both_call_indices_decode_with_runtime_contract() {
        let common_signature = [7u8; 64];
        let emergency = build_runtime_call(
            "LN001-NRC0G-944805165-2026",
            [3u8; 32],
            0,
            60,
            common_signature,
            None,
        )
        .expect("emergency call");
        assert_eq!(emergency[1], 0);
        assert!(citizenchain::RuntimeCall::decode_all(&mut emergency.as_slice()).is_ok());

        let routine = build_runtime_call(
            "LN001-NRC0G-944805165-2026",
            [4u8; 32],
            0,
            60,
            common_signature,
            Some([8u8; 64]),
        )
        .expect("routine call");
        assert_eq!(routine[1], 1);
        assert!(citizenchain::RuntimeCall::decode_all(&mut routine.as_slice()).is_ok());
    }
}
