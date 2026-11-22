//! 验证活跃提案编号、主体键和逐席投票的SCALE解码。

use super::*;
use codec::Encode;

/// 活跃提案 ID 列表:`BoundedVec<u128>` 与 `Vec<u128>` 同编码,镜像 decode 一致。
#[test]
fn active_proposal_ids_decode_from_vec_u128_golden() {
    let ids: Vec<u128> = vec![7, u64::MAX as u128 + 1, u128::MAX];
    let encoded = ids.encode();
    let decoded = decode_active_proposal_ids(&encoded).expect("decode Vec<u128>");
    assert_eq!(decoded, ids);
}

/// 空列表(点查缺省)解码为空。
#[test]
fn active_proposal_ids_decode_empty() {
    let encoded = Vec::<u128>::new().encode();
    let decoded = decode_active_proposal_ids(&encoded).expect("decode empty");
    assert!(decoded.is_empty());
}

#[test]
fn active_proposal_ids_reject_trailing_and_truncated_bytes() {
    let encoded = vec![u128::MAX].encode();
    let mut trailing = encoded.clone();
    trailing.push(0);
    assert!(decode_active_proposal_ids(&trailing).is_err());
    assert!(decode_active_proposal_ids(&encoded[..encoded.len() - 1]).is_err());
    assert!(decode_active_proposal_ids(&[]).is_err());
}

/// 活跃提案键必须是 ProposalSubject::InstitutionCid(CID),不是机构主账户。
#[test]
fn proposal_subject_institution_cid_key_encodes_scale_enum() {
    let key = proposal_subject_institution_cid_key("LN001-NRP0G-000000001-2026")
        .expect("encode subject key");
    let mut expected = vec![0_u8];
    expected.extend("LN001-NRP0G-000000001-2026".as_bytes().to_vec().encode());
    assert_eq!(key, expected);
}

/// 逐席投票值 bool 解码(赞成/反对)。
#[test]
fn ballot_value_decodes_bool() {
    assert!(bool::decode(&mut &true.encode()[..]).expect("decode true"));
    assert!(!bool::decode(&mut &false.encode()[..]).expect("decode false"));
}

#[test]
fn representative_ticket_key_decodes_complete_role_identity() {
    let encoded_ticket = (
        2_u32,
        InstitutionVoteTicketMirror {
            role_subject: RoleSubjectMirror {
                cid_number: b"LN001-NRP0G-000000001-2026".to_vec(),
                role_code: b"HOUSE_MEMBER".to_vec(),
            },
            voter_account_id: [7_u8; 32],
        },
    )
        .encode();
    // 独立金标：真实 pallet/item 前缀、Blake2_128Concat(1u128)、第二键哈希。
    let mut storage_key = hex::decode(concat!(
        "a5cba9989141c780c7f6d6d7614e72a159c12635b4bcb711dcf8e56871ead02f",
        "3ba80a3778f04ebf45e806d19a052025",
        "01000000000000000000000000000000",
        "00bd1a00af752c998a90ace91effb50c",
    ))
    .expect("real u128 ticket key prefix");
    assert_eq!(storage_key.len(), 80);
    storage_key.extend(encoded_ticket);
    for length in [0, 31, 63, 79, 80, storage_key.len() - 1] {
        assert!(decode_representative_ticket_storage_key(&storage_key[..length]).is_err());
    }
    let (body_index, ticket) =
        decode_representative_ticket_storage_key(&storage_key).expect("decode ticket key");
    assert_eq!(body_index, 2);
    assert_eq!(ticket.role_subject.role_code, b"HOUSE_MEMBER");
    assert_eq!(ticket.voter_account_id, [7_u8; 32]);

    storage_key.push(0);
    assert!(decode_representative_ticket_storage_key(&storage_key).is_err());
}
