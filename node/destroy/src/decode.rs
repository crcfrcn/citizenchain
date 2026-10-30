use crate::types::ResolutionDestroyDetail;

use nodeclient::codec::decode_cid_number;

const TAG_RESOLUTION_DESTROY: &[u8] = b"res-dst";

pub fn decode_resolution_destroy_action(
    proposal_id: u64,
    data: &[u8],
) -> Option<ResolutionDestroyDetail> {
    // SCALE 布局：MODULE_TAG + actor_cid_number + institution_account_id + amount。
    let tag = TAG_RESOLUTION_DESTROY;
    if data.len() < tag.len() + 1 + 32 + 16 || &data[..tag.len()] != tag {
        return None;
    }
    let mut offset = tag.len();

    let actor_cid_number = decode_cid_number(data, &mut offset)?;
    // 固定账户和金额必须准确占满余下输入，拒绝合法载荷后的附加字节。
    if offset + 32 + 16 != data.len() {
        return None;
    }
    let institution_account_id = format!("0x{}", hex::encode(&data[offset..offset + 32]));
    offset += 32;

    let amount = u128::from_le_bytes(data[offset..offset + 16].try_into().ok()?);

    Some(ResolutionDestroyDetail {
        proposal_id,
        actor_cid_number,
        institution_account_id,
        amount_fen: amount.to_string(),
    })
}
