use crate::types::RuntimeUpgradeDetail;
use codec::Decode;

use nodeclient::codec::decode_cid_number;
use nodeclient::codec::read_compact_u32;

pub const TAG_RUNTIME_UPGRADE: &[u8] = b"rt-upg";

pub fn decode_runtime_upgrade_action(
    proposal_id: u64,
    data: &[u8],
) -> Option<RuntimeUpgradeDetail> {
    // 跳过 MODULE_TAG("rt-upg":6)
    let tag = TAG_RUNTIME_UPGRADE;
    if data.len() < tag.len() || &data[..tag.len()] != tag {
        return None;
    }
    let mut offset = tag.len();

    // actor_cid_number: CidNumber
    let _actor_cid_number = decode_cid_number(data, &mut offset)?;

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

    // code_hash: [u8;32]
    if offset + 32 > data.len() {
        return None;
    }
    let code_hash_hex = hex::encode(&data[offset..offset + 32]);
    offset += 32;

    // expected_pow_params_hash: [u8;32]
    if offset + 32 > data.len() {
        return None;
    }
    let expected_pow_hash = hex::encode(&data[offset..offset + 32]);
    offset += 32;

    // new_pow_params: 固定字段 SCALE 编码。
    let mut params_input = &data[offset..];
    let pow_params = difficulty::PowDifficultyParams::decode(&mut params_input).ok()?;
    let consumed = data[offset..].len().saturating_sub(params_input.len());
    offset += consumed;

    // 协议升级摘要不保存业务状态，真实状态只读 VotingEngine::Proposals.status。
    if offset != data.len() {
        return None;
    }

    Some(RuntimeUpgradeDetail {
        proposal_id,
        proposer_account_id,
        reason,
        code_hash_hex,
        expected_pow_hash,
        pow_params,
    })
}
