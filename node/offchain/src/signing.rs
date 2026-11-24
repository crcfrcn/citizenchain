//! 编码清算行登记和端点更新调用并校验签名账户。

use codec::Encode;
use nodeclient::signing::build_sign_request_from_call_data;
use nodeclient::signing::VoteSignRequestResult;

/// 校验 0x 前缀小写 hex 公钥(64 hex 字符)。返回 (clean_hex, raw_bytes)。
fn parse_signer_public_key(signer_public_key: &str) -> Result<(String, Vec<u8>), String> {
    let signer_public_key = nodedesktop::validation::normalize_public_key(signer_public_key)?;
    let bytes = hex::decode(signer_public_key.trim_start_matches("0x"))
        .map_err(|e| format!("公钥解码失败:{e}"))?;
    Ok((signer_public_key, bytes))
}

/// 岗位由调用者明确选择，禁止以空值或固定岗位替代。
fn actor_role(role: &str) -> Result<offchain::ActorRoleCode, String> {
    if role.is_empty() || role.trim() != role {
        return Err("actor_role_code 必须为非空规范岗位码".into());
    }
    role.as_bytes()
        .to_vec()
        .try_into()
        .map_err(|_| "actor_role_code 超出链上岗位码长度".into())
}

/// 构造 register_clearing_bank 的 call_data。
pub(crate) fn build_register_call_data(
    actor_cid_number: &str,
    actor_role_code: &str,
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

    Ok(
        runtime::RuntimeCall::Offchain(offchain::pallet::Call::register_clearing_bank {
            actor_cid_number: actor_cid_number
                .as_bytes()
                .to_vec()
                .try_into()
                .map_err(|_| "机构CID超长")?,
            actor_role_code: actor_role(actor_role_code)?,
            peer_id: peer_id
                .as_bytes()
                .to_vec()
                .try_into()
                .map_err(|_| "PeerId超长")?,
            rpc_domain: rpc_domain
                .as_bytes()
                .to_vec()
                .try_into()
                .map_err(|_| "RPC域名超长")?,
            rpc_port,
        })
        .encode(),
    )
}

/// 构造 update_clearing_bank_endpoint 的 call_data。
pub(crate) fn build_update_endpoint_call_data(
    actor_cid_number: &str,
    actor_role_code: &str,
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
    Ok(
        runtime::RuntimeCall::Offchain(offchain::pallet::Call::update_clearing_bank_endpoint {
            actor_cid_number: actor_cid_number
                .as_bytes()
                .to_vec()
                .try_into()
                .map_err(|_| "机构CID超长")?,
            actor_role_code: actor_role(actor_role_code)?,
            new_domain: new_domain
                .as_bytes()
                .to_vec()
                .try_into()
                .map_err(|_| "RPC域名超长")?,
            new_port,
        })
        .encode(),
    )
}

/// 构造 unregister_clearing_bank 的 call_data。
pub(crate) fn build_unregister_call_data(
    actor_cid_number: &str,
    actor_role_code: &str,
) -> Result<Vec<u8>, String> {
    if actor_cid_number.is_empty()
        || actor_cid_number.len() > primitives::core_const::CID_NUMBER_MAX_BYTES as usize
    {
        return Err("actor_cid_number 长度必须在链上 CID_NUMBER_MAX_BYTES 范围内".to_string());
    }
    Ok(
        runtime::RuntimeCall::Offchain(offchain::pallet::Call::unregister_clearing_bank {
            actor_cid_number: actor_cid_number
                .as_bytes()
                .to_vec()
                .try_into()
                .map_err(|_| "机构CID超长")?,
            actor_role_code: actor_role(actor_role_code)?,
        })
        .encode(),
    )
}

/// register_clearing_bank QR 签名请求。
pub(crate) fn build_register_sign_request(
    signer_public_key: &str,
    actor_cid_number: &str,
    actor_role_code: &str,
    peer_id: &str,
    rpc_domain: &str,
    rpc_port: u16,
) -> Result<VoteSignRequestResult, String> {
    let (clean, bytes) = parse_signer_public_key(signer_public_key)?;
    let call_data = build_register_call_data(
        actor_cid_number,
        actor_role_code,
        peer_id,
        rpc_domain,
        rpc_port,
    )?;
    build_sign_request_from_call_data(&clean, &bytes, &call_data)
}

/// update_clearing_bank_endpoint QR 签名请求。
pub(crate) fn build_update_endpoint_sign_request(
    signer_public_key: &str,
    actor_cid_number: &str,
    actor_role_code: &str,
    new_domain: &str,
    new_port: u16,
) -> Result<VoteSignRequestResult, String> {
    let (clean, bytes) = parse_signer_public_key(signer_public_key)?;
    let call_data =
        build_update_endpoint_call_data(actor_cid_number, actor_role_code, new_domain, new_port)?;
    build_sign_request_from_call_data(&clean, &bytes, &call_data)
}

/// unregister_clearing_bank QR 签名请求。
pub(crate) fn build_unregister_sign_request(
    signer_public_key: &str,
    actor_cid_number: &str,
    actor_role_code: &str,
) -> Result<VoteSignRequestResult, String> {
    let (clean, bytes) = parse_signer_public_key(signer_public_key)?;
    let call_data = build_unregister_call_data(actor_cid_number, actor_role_code)?;
    build_sign_request_from_call_data(&clean, &bytes, &call_data)
}
#[cfg(test)]
#[path = "../tests/signing.rs"]
mod tests;
