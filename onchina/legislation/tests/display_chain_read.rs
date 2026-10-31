//! 原所属模块回归，断言与输入保持。
use super::*;
use codec::Encode;

/// 活跃提案 ID 列表:`BoundedVec<u64>` 与 `Vec<u64>` 同编码,镜像 decode 一致。
#[test]
fn active_proposal_ids_decode_from_vec_u64_golden() {
    let ids: Vec<u64> = vec![7, 12, 99];
    let encoded = ids.encode();
    let decoded = Vec::<u64>::decode(&mut &encoded[..]).expect("decode Vec<u64>");
    assert_eq!(decoded, vec![7, 12, 99]);
}

/// 空列表(点查缺省)解码为空。
#[test]
fn active_proposal_ids_decode_empty() {
    let encoded = Vec::<u64>::new().encode();
    let decoded = Vec::<u64>::decode(&mut &encoded[..]).expect("decode empty");
    assert!(decoded.is_empty());
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
    let mut storage_key = vec![0_u8; 72];
    storage_key.extend(encoded_ticket);
    let (body_index, ticket) =
        decode_representative_ticket_storage_key(&storage_key).expect("decode ticket key");
    assert_eq!(body_index, 2);
    assert_eq!(ticket.role_subject.role_code, b"HOUSE_MEMBER");
    assert_eq!(ticket.voter_account_id, [7_u8; 32]);

    storage_key.push(0);
    assert!(decode_representative_ticket_storage_key(&storage_key).is_err());
}
