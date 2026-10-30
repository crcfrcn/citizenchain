use crate::types::IssuanceAllocationItem;
use crate::types::ResolutionIssuanceDetail;

use nodeclient::codec::decode_cid_number;
use nodeclient::codec::decode_role_code;
use nodeclient::codec::read_compact_u32;

pub const TAG_RESOLUTION_ISSUANCE: &[u8] = b"res-iss";

pub fn decode_resolution_issuance_action(
    proposal_id: u64,
    data: &[u8],
) -> Option<ResolutionIssuanceDetail> {
    // SCALE 布局：MODULE_TAG("res-iss":7) + actor_cid_number + proposer_role_code
    //            + proposer(32) + reason(Compact+bytes) + total_amount(u128:16)
    //            + allocations(Compact<count> + N × (recipient(32) + amount(u128:16)))
    let tag = TAG_RESOLUTION_ISSUANCE;
    if data.len() < tag.len() || &data[..tag.len()] != tag {
        return None;
    }
    let mut offset = tag.len();

    // actor_cid_number: CidNumber
    let actor_cid_number = decode_cid_number(data, &mut offset)?;

    // proposer_role_code: RoleCode
    let proposer_role_code = decode_role_code(data, &mut offset)?;

    // proposer: [u8;32]
    if offset + 32 > data.len() {
        return None;
    }
    let proposer_account_id = format!("0x{}", hex::encode(&data[offset..offset + 32]));
    offset += 32;

    // reason: Vec<u8>
    let (reason_len, reason_len_size) = read_compact_u32(data, offset).ok()?;
    offset += reason_len_size;
    if offset + reason_len as usize > data.len() {
        return None;
    }
    let reason = String::from_utf8_lossy(&data[offset..offset + reason_len as usize]).to_string();
    offset += reason_len as usize;

    // total_amount: u128 (16 bytes LE)
    if offset + 16 > data.len() {
        return None;
    }
    let total_amount = u128::from_le_bytes(data[offset..offset + 16].try_into().ok()?);
    offset += 16;

    // allocations: Vec<RecipientAmount>
    let (alloc_count, alloc_count_size) = read_compact_u32(data, offset).ok()?;
    offset += alloc_count_size;

    let mut allocations = Vec::new();
    for _ in 0..alloc_count {
        if offset + 32 + 16 > data.len() {
            return None;
        }
        let recipient_account_id = format!("0x{}", hex::encode(&data[offset..offset + 32]));
        offset += 32;
        let amount = u128::from_le_bytes(data[offset..offset + 16].try_into().ok()?);
        offset += 16;
        allocations.push(IssuanceAllocationItem {
            recipient_account_id,
            amount_fen: amount.to_string(),
        });
    }

    if offset != data.len() {
        return None;
    }

    Some(ResolutionIssuanceDetail {
        proposal_id,
        actor_cid_number,
        proposer_role_code,
        proposer_account_id,
        reason,
        total_amount_fen: total_amount.to_string(),
        allocations,
    })
}
