//! 多签转账签名请求构造。
//!
//! 本文件只负责 `MultisigTransfer::propose_transfer` 的 call_data；
//! 通用 QR 协议、nonce、payload 和提交校验复用治理签名基础设施。

use crate::governance;

/// 构建 propose_transfer 签名请求（创建转账提案：pallet=17, call=0）。
pub fn build_propose_transfer_sign_request(
    signer_public_key: &str,
    actor_cid_number: &str,
    proposer_role_code: &str,
    institution_account_id: &str,
    beneficiary_ss58_address: &str,
    amount_yuan: f64,
    remark: &str,
) -> Result<governance::signing::VoteSignRequestResult, String> {
    let signer_public_key = crate::shared::validation::normalize_public_key(signer_public_key)?;
    let signer_public_key_bytes = hex::decode(signer_public_key.trim_start_matches("0x"))
        .map_err(|e| format!("公钥解码失败: {e}"))?;

    if amount_yuan < 1.11 {
        return Err("转账金额不能低于 1.11 元".to_string());
    }
    let amount_fen = (amount_yuan * 100.0).round() as u128;

    let remark_bytes = remark.as_bytes();
    if remark_bytes.len() > 256 {
        return Err(format!(
            "备注长度不能超过 256 字节，当前 {} 字节",
            remark_bytes.len()
        ));
    }

    let beneficiary_bytes =
        governance::signing::account_id_from_ss58_address(beneficiary_ss58_address)?;

    let institution_account_id =
        super::account_id::institution_account_from_id(institution_account_id)?;
    if beneficiary_bytes == institution_account_id {
        return Err("收款地址不能等于转出机构账户".to_string());
    }

    let call_data = build_transfer_call_data(
        actor_cid_number,
        proposer_role_code,
        &institution_account_id,
        &beneficiary_bytes,
        amount_fen,
        remark_bytes,
    )?;

    governance::signing::build_sign_request_from_call_data(
        &signer_public_key,
        &signer_public_key_bytes,
        &call_data,
    )
}

/// 构建安全基金转账提案签名请求（pallet=17, call=1）。
pub fn build_propose_safety_fund_sign_request(
    signer_public_key: &str,
    actor_cid_number: &str,
    proposer_role_code: &str,
    institution_account_id: &str,
    beneficiary_ss58_address: &str,
    amount_yuan: f64,
    remark: &str,
) -> Result<governance::signing::VoteSignRequestResult, String> {
    let signer_public_key_clean = normalize_signer_public_key(signer_public_key)?;
    let signer_public_key_bytes = hex::decode(signer_public_key_clean.trim_start_matches("0x"))
        .map_err(|e| format!("公钥解码失败: {e}"))?;
    let call_data = build_safety_fund_call_data(
        actor_cid_number,
        proposer_role_code,
        institution_account_id,
        beneficiary_ss58_address,
        amount_yuan,
        remark,
    )?;

    governance::signing::build_sign_request_from_call_data(
        &signer_public_key_clean,
        &signer_public_key_bytes,
        &call_data,
    )
}

/// 构建手续费划转提案签名请求（pallet=17, call=2）。
pub fn build_propose_sweep_sign_request(
    signer_public_key: &str,
    actor_cid_number: &str,
    proposer_role_code: &str,
    institution_account_id: &str,
    amount_yuan: f64,
) -> Result<governance::signing::VoteSignRequestResult, String> {
    let signer_public_key_clean = normalize_signer_public_key(signer_public_key)?;
    let signer_public_key_bytes = hex::decode(signer_public_key_clean.trim_start_matches("0x"))
        .map_err(|e| format!("公钥解码失败: {e}"))?;
    let call_data = build_sweep_call_data(
        actor_cid_number,
        proposer_role_code,
        institution_account_id,
        amount_yuan,
    )?;

    governance::signing::build_sign_request_from_call_data(
        &signer_public_key_clean,
        &signer_public_key_bytes,
        &call_data,
    )
}

/// 安全基金转账 call_data，供签名构造和签名响应提交复用。
pub(crate) fn build_safety_fund_call_data(
    actor_cid_number: &str,
    proposer_role_code: &str,
    institution_account_id: &str,
    beneficiary_ss58_address: &str,
    amount_yuan: f64,
    remark: &str,
) -> Result<Vec<u8>, String> {
    if amount_yuan < 1.11 {
        return Err("转账金额不能低于 1.11 元".to_string());
    }
    let amount_fen = (amount_yuan * 100.0).round() as u128;
    let remark_bytes = remark.as_bytes();
    if remark_bytes.len() > 256 {
        return Err(format!(
            "备注长度不能超过 256 字节，当前 {} 字节",
            remark_bytes.len()
        ));
    }
    let beneficiary_bytes =
        governance::signing::account_id_from_ss58_address(beneficiary_ss58_address)?;
    let institution_account_id =
        super::account_id::institution_account_from_id(institution_account_id)?;
    let actor_cid = encode_actor_cid(actor_cid_number)?;
    let proposer_role = encode_proposer_role_code(proposer_role_code)?;
    let remark_compact = governance::signing::encode_compact_u32_pub(remark_bytes.len() as u32);

    let mut call_data = Vec::with_capacity(
        2 + actor_cid.len()
            + proposer_role.len()
            + 32
            + 32
            + 16
            + remark_compact.len()
            + remark_bytes.len(),
    );
    call_data.push(17u8);
    call_data.push(1u8);
    call_data.extend_from_slice(&actor_cid);
    call_data.extend_from_slice(&proposer_role);
    call_data.extend_from_slice(&institution_account_id);
    call_data.extend_from_slice(&beneficiary_bytes);
    call_data.extend_from_slice(&amount_fen.to_le_bytes());
    call_data.extend_from_slice(&remark_compact);
    call_data.extend_from_slice(remark_bytes);
    Ok(call_data)
}

/// 普通机构多签转账 call_data，供签名构造和签名响应提交复用。
pub(crate) fn build_transfer_call_data(
    actor_cid_number: &str,
    proposer_role_code: &str,
    institution_account_id: &[u8; 32],
    beneficiary_bytes: &[u8; 32],
    amount_fen: u128,
    remark_bytes: &[u8],
) -> Result<Vec<u8>, String> {
    if remark_bytes.len() > 256 {
        return Err(format!(
            "备注长度不能超过 256 字节，当前 {} 字节",
            remark_bytes.len()
        ));
    }
    let actor_cid = encode_actor_cid(actor_cid_number)?;
    let proposer_role = encode_proposer_role_code(proposer_role_code)?;
    let remark_compact = governance::signing::encode_compact_u32_pub(remark_bytes.len() as u32);
    let mut call_data = Vec::with_capacity(
        2 + 1
            + actor_cid.len()
            + 1
            + proposer_role.len()
            + 32
            + 32
            + 16
            + remark_compact.len()
            + remark_bytes.len(),
    );
    call_data.push(17u8);
    call_data.push(0u8);
    // Option<CidNumber>::Some 判别值 + CID，再跟明确的资金账户。
    call_data.push(1u8);
    call_data.extend_from_slice(&actor_cid);
    // 机构转账必须同时携带 Option<RoleCode>::Some。
    call_data.push(1u8);
    call_data.extend_from_slice(&proposer_role);
    call_data.extend_from_slice(institution_account_id);
    call_data.extend_from_slice(beneficiary_bytes);
    call_data.extend_from_slice(&amount_fen.to_le_bytes());
    call_data.extend_from_slice(&remark_compact);
    call_data.extend_from_slice(remark_bytes);
    Ok(call_data)
}

/// 手续费划转 call_data，供签名构造和签名响应提交复用。
pub(crate) fn build_sweep_call_data(
    actor_cid_number: &str,
    proposer_role_code: &str,
    institution_account_id: &str,
    amount_yuan: f64,
) -> Result<Vec<u8>, String> {
    if amount_yuan <= 0.0 {
        return Err("划转金额必须大于 0".to_string());
    }
    let amount_fen = (amount_yuan * 100.0).round() as u128;
    let institution_account_id =
        super::account_id::institution_account_from_id(institution_account_id)?;
    let actor_cid = encode_actor_cid(actor_cid_number)?;
    let proposer_role = encode_proposer_role_code(proposer_role_code)?;

    let mut call_data = Vec::with_capacity(2 + actor_cid.len() + proposer_role.len() + 32 + 16);
    call_data.push(17u8);
    call_data.push(2u8);
    call_data.extend_from_slice(&actor_cid);
    call_data.extend_from_slice(&proposer_role);
    call_data.extend_from_slice(&institution_account_id);
    call_data.extend_from_slice(&amount_fen.to_le_bytes());
    Ok(call_data)
}

/// 按链上 `CID_NUMBER_MAX_BYTES` 约束 SCALE 编码机构交易唯一主键。
fn encode_actor_cid(actor_cid_number: &str) -> Result<Vec<u8>, String> {
    if actor_cid_number.is_empty()
        || actor_cid_number.len() > primitives::core_const::CID_NUMBER_MAX_BYTES as usize
    {
        return Err("actor_cid_number 长度必须在链上 CID_NUMBER_MAX_BYTES 范围内".to_string());
    }
    let mut encoded = governance::signing::encode_compact_u32_pub(actor_cid_number.len() as u32);
    encoded.extend_from_slice(actor_cid_number.as_bytes());
    Ok(encoded)
}

/// 按链上 `RoleCode` 上限编码岗位码；管理员账户本身不代表业务权限。
fn encode_proposer_role_code(proposer_role_code: &str) -> Result<Vec<u8>, String> {
    let proposer_role_code = proposer_role_code.trim();
    if proposer_role_code.is_empty() || proposer_role_code.len() > 64 {
        return Err("proposer_role_code 长度必须为 1 到 64 字节".to_string());
    }
    let mut encoded = governance::signing::encode_compact_u32_pub(proposer_role_code.len() as u32);
    encoded.extend_from_slice(proposer_role_code.as_bytes());
    Ok(encoded)
}

fn normalize_signer_public_key(signer_public_key: &str) -> Result<String, String> {
    crate::shared::validation::normalize_public_key(signer_public_key)
}
