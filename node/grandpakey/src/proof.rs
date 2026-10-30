use crate::request::COMMITTEE_ROLE_CODE;
use codec::Decode;
use codec::Encode;

use serde_json::Value;

use sp_runtime::AccountId32;

pub fn decode_hex_32(value: &str, field: &str) -> Result<[u8; 32], String> {
    let bytes = hex::decode(value.trim_start_matches("0x"))
        .map_err(|error| format!("{field} 十六进制无效: {error}"))?;
    bytes
        .try_into()
        .map_err(|_| format!("{field} 必须为 32 字节"))
}

pub fn finalized_context() -> Result<(String, u64), String> {
    let finalized_hash = nodeclient::query::fetch_finalized_head()?;
    let header = nodeclient::signing::rpc_post(
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

pub fn fetch_current_public_key(
    actor_cid_number: &str,
    finalized_hash: &str,
) -> Result<[u8; 32], String> {
    let key = nodeclient::query::map_key(
        "GrandpaKeyChange",
        "CurrentGrandpaKeys",
        &cid_scale(actor_cid_number)?,
    );
    let raw = nodeclient::query::fetch_storage_at(&key, finalized_hash)?
        .ok_or_else(|| "链上未找到目标机构 GRANDPA 公钥".to_string())?;
    let bytes = hex::decode(raw.trim_start_matches("0x"))
        .map_err(|error| format!("解码当前 GRANDPA 公钥失败: {error}"))?;
    <[u8; 32]>::decode(&mut bytes.as_slice())
        .map_err(|_| "当前 GRANDPA 公钥 SCALE 数据无效".to_string())
}

pub(crate) fn fetch_proof_nonce(
    actor_cid_number: &str,
    finalized_hash: &str,
) -> Result<u64, String> {
    let key = nodeclient::query::map_key(
        "GrandpaKeyChange",
        "NextGrandpaKeyProofNonce",
        &cid_scale(actor_cid_number)?,
    );
    let Some(raw) = nodeclient::query::fetch_storage_at(&key, finalized_hash)? else {
        return Ok(0);
    };
    let bytes = hex::decode(raw.trim_start_matches("0x"))
        .map_err(|error| format!("解码 GRANDPA proof nonce 失败: {error}"))?;
    u64::decode(&mut bytes.as_slice()).map_err(|_| "GRANDPA proof nonce SCALE 数据无效".to_string())
}

pub(crate) fn fetch_current_set_id(finalized_hash: &str) -> Result<u64, String> {
    let key = nodeclient::query::value_key("Grandpa", "CurrentSetId");
    let Some(raw) = nodeclient::query::fetch_storage_at(&key, finalized_hash)? else {
        return Ok(0);
    };
    let bytes = hex::decode(raw.trim_start_matches("0x"))
        .map_err(|error| format!("解码 GRANDPA set id 失败: {error}"))?;
    u64::decode(&mut bytes.as_slice()).map_err(|_| "GRANDPA set id SCALE 数据无效".to_string())
}

pub fn fetch_authorities(
    finalized_hash: &str,
) -> Result<sp_consensus_grandpa::AuthorityList, String> {
    let key = nodeclient::query::value_key("Grandpa", "Authorities");
    let raw = nodeclient::query::fetch_storage_at(&key, finalized_hash)?
        .ok_or_else(|| "链上 GRANDPA authority set 不存在".to_string())?;
    let bytes = hex::decode(raw.trim_start_matches("0x"))
        .map_err(|error| format!("解码 GRANDPA authority set 失败: {error}"))?;
    sp_consensus_grandpa::AuthorityList::decode(&mut bytes.as_slice())
        .map_err(|_| "GRANDPA authority set SCALE 数据无效".to_string())
}

pub(crate) fn signer_account_id(signer_public_key: &str) -> Result<AccountId32, String> {
    let normalized = nodedesktop::validation::normalize_public_key(signer_public_key)?;
    Ok(AccountId32::new(decode_hex_32(
        &normalized,
        "signer_public_key",
    )?))
}

pub(crate) fn build_runtime_call(
    actor_cid_number: &str,
    new_public_key: [u8; 32],
    proof_nonce: u64,
    proof_expires_at: u32,
    new_key_signature: [u8; 64],
    old_key_signature: Option<[u8; 64]>,
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
    let call = match old_key_signature {
        Some(old_key_signature) => runtime::RuntimeCall::GrandpaKeyChange(
            grandpakey::Call::<runtime::Runtime>::schedule_grandpa_key_rotation {
                actor_cid_number,
                actor_role_code,
                new_public_key,
                proof_nonce,
                proof_expires_at,
                old_public_key_signature: old_key_signature,
                new_public_key_signature: new_key_signature,
            },
        ),
        None => runtime::RuntimeCall::GrandpaKeyChange(
            grandpakey::Call::<runtime::Runtime>::propose_emergency_grandpa_key_recovery {
                actor_cid_number,
                actor_role_code,
                new_public_key,
                proof_nonce,
                proof_expires_at,
                new_public_key_signature: new_key_signature,
            },
        ),
    };
    Ok(call.encode())
}

#[cfg(test)]
#[path = "../tests/unit.rs"]
mod unit_tests;
