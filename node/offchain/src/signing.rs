use nodeclient::signing::build_sign_request_from_call_data;
use nodeclient::signing::encode_compact_u32_pub;
use nodeclient::signing::VoteSignRequestResult;

/// pallet_index = 19(OffchainTransaction)。
const PALLET_INDEX: u8 = 19;

const CALL_REGISTER: u8 = 50;

const CALL_UPDATE_ENDPOINT: u8 = 51;

const CALL_UNREGISTER: u8 = 52;

/// 校验 0x 前缀小写 hex 公钥(64 hex 字符)。返回 (clean_hex, raw_bytes)。
fn parse_signer_public_key(signer_public_key: &str) -> Result<(String, Vec<u8>), String> {
    let signer_public_key = nodedesktop::validation::normalize_public_key(signer_public_key)?;
    let bytes = hex::decode(signer_public_key.trim_start_matches("0x"))
        .map_err(|e| format!("公钥解码失败:{e}"))?;
    Ok((signer_public_key, bytes))
}

/// 把 cid_number / peer_id / domain 等可变长字段按 SCALE Vec<u8> 编码:
/// `Compact<u32>(len) || raw_bytes`。
fn encode_bytes_with_len(raw: &[u8]) -> Vec<u8> {
    let mut out = encode_compact_u32_pub(raw.len() as u32);
    out.extend_from_slice(raw);
    out
}

/// 构造 register_clearing_bank 的 call_data。
pub(crate) fn build_register_call_data(
    actor_cid_number: &str,
    peer_id: &str,
    rpc_domain: &str,
    rpc_port: u16,
) -> Result<Vec<u8>, String> {
    if actor_cid_number.is_empty()
        || actor_cid_number.len() > primitives::core_const::CID_NUMBER_MAX_BYTES as usize
    {
        return Err("actor_cid_number 长度必须在链上 CID_NUMBER_MAX_BYTES 范围内".to_string());
    }
    if peer_id.is_empty() || peer_id.len() > 64 {
        return Err("peer_id 长度需在 1..=64".to_string());
    }
    if rpc_domain.is_empty() || rpc_domain.len() > 128 {
        return Err("rpc_domain 长度需在 1..=128".to_string());
    }
    if rpc_port < 1024 {
        return Err("rpc_port 必须 >= 1024".to_string());
    }

    let mut call = Vec::with_capacity(
        2 + 1 + actor_cid_number.len() + 1 + peer_id.len() + 1 + rpc_domain.len() + 2,
    );
    call.push(PALLET_INDEX);
    call.push(CALL_REGISTER);
    call.extend_from_slice(&encode_bytes_with_len(actor_cid_number.as_bytes()));
    call.extend_from_slice(&encode_bytes_with_len(peer_id.as_bytes()));
    call.extend_from_slice(&encode_bytes_with_len(rpc_domain.as_bytes()));
    call.extend_from_slice(&rpc_port.to_le_bytes());
    Ok(call)
}

/// 构造 update_clearing_bank_endpoint 的 call_data。
pub(crate) fn build_update_endpoint_call_data(
    actor_cid_number: &str,
    new_domain: &str,
    new_port: u16,
) -> Result<Vec<u8>, String> {
    if actor_cid_number.is_empty()
        || actor_cid_number.len() > primitives::core_const::CID_NUMBER_MAX_BYTES as usize
    {
        return Err("actor_cid_number 长度必须在链上 CID_NUMBER_MAX_BYTES 范围内".to_string());
    }
    if new_domain.is_empty() || new_domain.len() > 128 {
        return Err("rpc_domain 长度需在 1..=128".to_string());
    }
    if new_port < 1024 {
        return Err("rpc_port 必须 >= 1024".to_string());
    }
    let mut call = Vec::with_capacity(2 + 1 + actor_cid_number.len() + 1 + new_domain.len() + 2);
    call.push(PALLET_INDEX);
    call.push(CALL_UPDATE_ENDPOINT);
    call.extend_from_slice(&encode_bytes_with_len(actor_cid_number.as_bytes()));
    call.extend_from_slice(&encode_bytes_with_len(new_domain.as_bytes()));
    call.extend_from_slice(&new_port.to_le_bytes());
    Ok(call)
}

/// 构造 unregister_clearing_bank 的 call_data。
pub(crate) fn build_unregister_call_data(actor_cid_number: &str) -> Result<Vec<u8>, String> {
    if actor_cid_number.is_empty()
        || actor_cid_number.len() > primitives::core_const::CID_NUMBER_MAX_BYTES as usize
    {
        return Err("actor_cid_number 长度必须在链上 CID_NUMBER_MAX_BYTES 范围内".to_string());
    }
    let mut call = Vec::with_capacity(2 + 1 + actor_cid_number.len());
    call.push(PALLET_INDEX);
    call.push(CALL_UNREGISTER);
    call.extend_from_slice(&encode_bytes_with_len(actor_cid_number.as_bytes()));
    Ok(call)
}

/// register_clearing_bank QR 签名请求。
pub(crate) fn build_register_sign_request(
    signer_public_key: &str,
    actor_cid_number: &str,
    peer_id: &str,
    rpc_domain: &str,
    rpc_port: u16,
) -> Result<VoteSignRequestResult, String> {
    let (clean, bytes) = parse_signer_public_key(signer_public_key)?;
    let call_data = build_register_call_data(actor_cid_number, peer_id, rpc_domain, rpc_port)?;
    build_sign_request_from_call_data(&clean, &bytes, &call_data)
}

/// update_clearing_bank_endpoint QR 签名请求。
pub(crate) fn build_update_endpoint_sign_request(
    signer_public_key: &str,
    actor_cid_number: &str,
    new_domain: &str,
    new_port: u16,
) -> Result<VoteSignRequestResult, String> {
    let (clean, bytes) = parse_signer_public_key(signer_public_key)?;
    let call_data = build_update_endpoint_call_data(actor_cid_number, new_domain, new_port)?;
    build_sign_request_from_call_data(&clean, &bytes, &call_data)
}

/// unregister_clearing_bank QR 签名请求。
pub(crate) fn build_unregister_sign_request(
    signer_public_key: &str,
    actor_cid_number: &str,
) -> Result<VoteSignRequestResult, String> {
    let (clean, bytes) = parse_signer_public_key(signer_public_key)?;
    let call_data = build_unregister_call_data(actor_cid_number)?;
    build_sign_request_from_call_data(&clean, &bytes, &call_data)
}
#[cfg(test)]
#[path = "../tests/signing.rs"]
mod tests;
