use crate::proof::build_runtime_call;
use crate::proof::decode_hex_32;
use crate::proof::fetch_current_public_key;
use crate::proof::fetch_current_set_id;
use crate::proof::fetch_proof_nonce;
use crate::proof::finalized_context;
use crate::proof::signer_account_id;
use crate::storage::clear_pending;
use crate::storage::load_pending;
use crate::storage::save_pending;
use crate::types::GrandpaKeyChangeRequest;
use crate::types::PendingKeyChange;

use ed25519_dalek::Signer;

use grandpakey::GrandpaKeyProofPayload;

use sp_core::H256;

use std::collections::HashMap;

use std::sync::Mutex;
use std::sync::OnceLock;

use tauri::AppHandle;

pub(crate) const COMMITTEE_ROLE_CODE: &[u8] =
    primitives::governance_skeleton::ROLE_CODE_COMMITTEE_MEMBER;

const PROOF_LIFETIME_BLOCKS: u64 = 60;

#[derive(Debug, Clone)]
pub(crate) struct BuildSession {
    pub(crate) call_data: Vec<u8>,
}

static BUILD_SESSIONS: OnceLock<Mutex<HashMap<String, BuildSession>>> = OnceLock::new();

pub(crate) fn build_sessions() -> &'static Mutex<HashMap<String, BuildSession>> {
    BUILD_SESSIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

pub(crate) fn build_request_sync(
    app: &AppHandle,
    actor_cid_number: &str,
    signer_public_key: &str,
    emergency_recovery: bool,
) -> Result<GrandpaKeyChangeRequest, String> {
    if load_pending(app)?.is_some() {
        return Err("本机已有尚未完成的 GRANDPA 换钥流程".to_string());
    }
    let cid_short_name = nodecontrol::grandpa::cid_short_name_for_cid(actor_cid_number)?;
    let (finalized_hash, finalized_number) = finalized_context()?;
    let genesis_hash = H256::from(nodeclient::signing::fetch_genesis_hash()?);
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
        crate::types::LocalChangeKind::EmergencyRecovery
    } else {
        crate::types::LocalChangeKind::RoutineRotation
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
    let digest = grandpakey::proof_signing_digest(&proof_payload);
    let new_key_signature = new_signing_key.sign(&digest).to_bytes();
    let old_key_signature = if emergency_recovery {
        None
    } else {
        Some(nodecontrol::grandpa::sign_rotation_proof(
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
        new_key_signature,
        old_key_signature,
    )?;
    let normalized_signer = nodedesktop::validation::normalize_public_key(signer_public_key)?;
    let signer_bytes = decode_hex_32(&normalized_signer, "signer_public_key")?;
    let sign_request = nodeclient::signing::build_sign_request_from_call_data(
        &normalized_signer,
        &signer_bytes,
        &call_data,
    )?;

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
        nodecontrol::grandpa::import_rotation_candidate(app, &new_private_key, &new_public_key)
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
            crate::types::LocalChangeKind::RoutineRotation => "routine_rotation",
            crate::types::LocalChangeKind::EmergencyRecovery => "emergency_recovery",
        }
        .to_string(),
    })
}
#[cfg(test)]
#[path = "../tests/integration.rs"]
mod tests;
