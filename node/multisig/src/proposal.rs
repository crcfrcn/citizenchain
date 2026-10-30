use crate::types::ProposalAction;
use crate::types::SafetyFundProposalDetail;
use crate::types::SweepProposalDetail;
use crate::types::TransferProposalDetail;

/// MODULE_TAG 前缀（必须与 runtime `multisig` pallet 保持一致）。
const TAG_TRANSFER: &[u8] = b"multisig";

/// 从 `VotingEngine::ProposalData` 的业务 payload 解码普通多签转账动作。
pub fn decode_proposal_data_action(proposal_id: u64, data: &[u8]) -> Option<ProposalAction> {
    decode_transfer_action(proposal_id, data)
        .map(|detail| crate::types::ProposalAction::Transfer(Box::new(detail)))
}

/// 从 multisig-transfer 独立 storage 查询安全基金转账或手续费划转动作。
pub fn fetch_stored_action(proposal_id: u64) -> Result<Option<ProposalAction>, String> {
    if let Some(detail) = fetch_safety_fund_proposal_action(proposal_id)? {
        return Ok(Some(crate::types::ProposalAction::SafetyFund(Box::new(
            detail,
        ))));
    }
    if let Some(detail) = fetch_sweep_proposal_action(proposal_id)? {
        return Ok(Some(crate::types::ProposalAction::Sweep(Box::new(detail))));
    }
    Ok(None)
}

/// 生成 multisig-transfer 列表摘要。
pub fn format_summary<F>(action: &ProposalAction, resolve_cid_full_name: F) -> String
where
    F: Fn(&str) -> Option<String>,
{
    match action {
        crate::types::ProposalAction::Transfer(detail) => {
            format_transfer_summary(detail, resolve_cid_full_name)
        }
        crate::types::ProposalAction::SafetyFund(detail) => format_safety_fund_summary(detail),
        crate::types::ProposalAction::Sweep(detail) => {
            format_sweep_summary(detail, resolve_cid_full_name)
        }
    }
}

fn decode_transfer_action(proposal_id: u64, data: &[u8]) -> Option<TransferProposalDetail> {
    // MODULE_TAG("multisig") + actor_cid_number: Option<CidNumber> + funding_account_id
    // + beneficiary + amount: u128(16) + remark: Vec<u8> + proposer: [u8;32]
    let tag = TAG_TRANSFER;
    if data.len() < tag.len() + 1 + 32 + 32 + 16 + 1 + 32 {
        return None;
    }
    if &data[..tag.len()] != tag {
        return None;
    }
    let mut offset = tag.len();

    let actor_cid_number = match *data.get(offset)? {
        0 => {
            offset += 1;
            None
        }
        1 => {
            offset += 1;
            Some(read_cid_number(data, &mut offset)?)
        }
        _ => return None,
    };

    if offset + 32 + 32 + 16 > data.len() {
        return None;
    }
    let funding_account_id = format!("0x{}", hex::encode(&data[offset..offset + 32]));
    offset += 32;

    let beneficiary_account_id = format!("0x{}", hex::encode(&data[offset..offset + 32]));
    offset += 32;

    let amount_bytes: [u8; 16] = data[offset..offset + 16].try_into().ok()?;
    let amount_fen = u128::from_le_bytes(amount_bytes);
    offset += 16;

    let (remark_len, remark_len_size) = read_compact_u32(data, offset).ok()?;
    offset += remark_len_size;
    if offset + remark_len as usize > data.len() {
        return None;
    }
    let remark = String::from_utf8_lossy(&data[offset..offset + remark_len as usize]).to_string();
    offset += remark_len as usize;

    if offset + 32 > data.len() {
        return None;
    }
    let proposer_account_id = format!("0x{}", hex::encode(&data[offset..offset + 32]));
    offset += 32;
    if offset != data.len() {
        return None;
    }

    Some(TransferProposalDetail {
        proposal_id,
        actor_cid_number,
        funding_account_id,
        beneficiary_account_id,
        amount_fen: amount_fen.to_string(),
        remark,
        proposer_account_id,
    })
}

fn fetch_safety_fund_proposal_action(
    proposal_id: u64,
) -> Result<Option<SafetyFundProposalDetail>, String> {
    let key = nodeclient::query::map_key(
        "MultisigTransfer",
        "SafetyFundProposalActions",
        &proposal_id.to_le_bytes(),
    );
    // 多签转账动作的金额展示以 finalized storage 为准(ADR-017 收口)。
    match nodeclient::query::fetch_finalized_storage(&key)? {
        None => Ok(None),
        Some(hex_data) => {
            let data = decode_hex_storage(&hex_data)?;
            // SafetyFundAction: actor CID + institution_account_id + beneficiary + amount + remark + proposer。
            let mut offset = 0usize;
            let Some(actor_cid_number) = read_cid_number(&data, &mut offset) else {
                return Ok(None);
            };
            if offset + 32 + 32 + 16 > data.len() {
                return Ok(None);
            }
            let institution_account_id = format!("0x{}", hex::encode(&data[offset..offset + 32]));
            offset += 32;
            let beneficiary_account_id = format!("0x{}", hex::encode(&data[offset..offset + 32]));
            offset += 32;
            let amount_fen = {
                let mut bytes = [0u8; 16];
                bytes.copy_from_slice(&data[offset..offset + 16]);
                u128::from_le_bytes(bytes)
            };
            offset += 16;

            let (remark_len, compact_size) = read_compact_u32(&data, offset)?;
            let remark_start = offset + compact_size;
            let remark_end = remark_start + remark_len as usize;
            if remark_end + 32 != data.len() {
                return Ok(None);
            }
            let remark = String::from_utf8_lossy(&data[remark_start..remark_end]).to_string();

            Ok(Some(SafetyFundProposalDetail {
                proposal_id,
                actor_cid_number,
                institution_account_id,
                beneficiary_account_id,
                amount_fen: amount_fen.to_string(),
                remark,
            }))
        }
    }
}

fn fetch_sweep_proposal_action(proposal_id: u64) -> Result<Option<SweepProposalDetail>, String> {
    let key = nodeclient::query::map_key(
        "MultisigTransfer",
        "SweepProposalActions",
        &proposal_id.to_le_bytes(),
    );
    // 多签转账动作的金额展示以 finalized storage 为准(ADR-017 收口)。
    match nodeclient::query::fetch_finalized_storage(&key)? {
        None => Ok(None),
        Some(hex_data) => {
            let data = decode_hex_storage(&hex_data)?;
            // SweepAction: actor CID + institution_account_id + amount + proposer。
            let mut offset = 0usize;
            let Some(actor_cid_number) = read_cid_number(&data, &mut offset) else {
                return Ok(None);
            };
            if offset + 32 + 16 + 32 != data.len() {
                return Ok(None);
            }
            let institution_account_id = format!("0x{}", hex::encode(&data[offset..offset + 32]));
            offset += 32;
            let amount_fen = {
                let mut bytes = [0u8; 16];
                bytes.copy_from_slice(&data[offset..offset + 16]);
                u128::from_le_bytes(bytes)
            };
            Ok(Some(SweepProposalDetail {
                proposal_id,
                actor_cid_number,
                institution_account_id,
                amount_fen: amount_fen.to_string(),
            }))
        }
    }
}

fn decode_hex_storage(hex_str: &str) -> Result<Vec<u8>, String> {
    let clean = hex_str.strip_prefix("0x").unwrap_or(hex_str);
    hex::decode(clean).map_err(|e| format!("hex 解码失败: {e}"))
}

fn read_compact_u32(data: &[u8], offset: usize) -> Result<(u32, usize), String> {
    if offset >= data.len() {
        return Err("Compact<u32> 数据不足".to_string());
    }
    let first = data[offset];
    let mode = first & 0x03;
    match mode {
        0 => Ok(((first >> 2) as u32, 1)),
        1 => {
            if offset + 2 > data.len() {
                return Err("Compact<u32> two-byte 数据不足".to_string());
            }
            let value = (((data[offset + 1] as u32) << 8) | first as u32) >> 2;
            Ok((value, 2))
        }
        2 => {
            if offset + 4 > data.len() {
                return Err("Compact<u32> four-byte 数据不足".to_string());
            }
            let value = ((data[offset + 3] as u32) << 24)
                | ((data[offset + 2] as u32) << 16)
                | ((data[offset + 1] as u32) << 8)
                | (data[offset] as u32);
            Ok((value >> 2, 4))
        }
        _ => Err("Compact<u32> big-integer 模式暂不支持".to_string()),
    }
}

fn read_cid_number(data: &[u8], offset: &mut usize) -> Option<String> {
    let (len, compact_size) = read_compact_u32(data, *offset).ok()?;
    *offset = offset.checked_add(compact_size)?;
    let end = offset.checked_add(len as usize)?;
    if end > data.len() {
        return None;
    }
    let cid = core::str::from_utf8(&data[*offset..end]).ok()?.to_string();
    *offset = end;
    Some(cid)
}

fn truncate_chars(s: &str, max_chars: usize) -> String {
    let mut chars = s.chars();
    let truncated: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        format!("{truncated}…")
    } else {
        truncated
    }
}

fn format_amount_fen(amount_fen: &str) -> String {
    let amount: u128 = amount_fen.parse().unwrap_or(0);
    let yuan = amount / 100;
    let cents = amount % 100;
    let mut digits: Vec<char> = yuan.to_string().chars().rev().collect();
    let mut grouped = String::new();
    for (index, ch) in digits.drain(..).enumerate() {
        if index > 0 && index % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(ch);
    }
    let yuan_grouped: String = grouped.chars().rev().collect();
    format!("{yuan_grouped}.{cents:02}")
}

fn format_transfer_summary<F>(detail: &TransferProposalDetail, resolve_cid_full_name: F) -> String
where
    F: Fn(&str) -> Option<String>,
{
    let remark_short = truncate_chars(&detail.remark, 30);
    let actor = detail
        .actor_cid_number
        .as_deref()
        .and_then(resolve_cid_full_name)
        .unwrap_or_else(|| "个人多签".to_string());
    format!(
        "{actor}转账 {} 元：{remark_short}",
        format_amount_fen(&detail.amount_fen)
    )
}

fn format_safety_fund_summary(detail: &SafetyFundProposalDetail) -> String {
    format!("安全基金转账 {} 元", format_amount_fen(&detail.amount_fen))
}

fn format_sweep_summary<F>(detail: &SweepProposalDetail, resolve_cid_full_name: F) -> String
where
    F: Fn(&str) -> Option<String>,
{
    let inst_name = resolve_cid_full_name(&detail.actor_cid_number)
        .unwrap_or_else(|| detail.actor_cid_number.clone());
    format!(
        "手续费划转 {} 元：{inst_name}",
        format_amount_fen(&detail.amount_fen)
    )
}

#[cfg(test)]
#[path = "../tests/unit.rs"]
mod tests;
