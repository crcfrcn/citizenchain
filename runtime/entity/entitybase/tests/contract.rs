//! 原机构记录SCALE字段布局金标，预期保持。
use crate::*;
use codec::Encode;

/// 机构记录的声明序就是 SCALE 存储契约，NodeGuard 按同一顺序完整解码。
#[test]
fn institution_info_field_order_matches_node_guard() {
    let value = InstitutionInfo {
        cid_full_name: b"full".to_vec(),
        cid_short_name: b"short".to_vec(),
        town_code: b"001".to_vec(),
        legal_representative: Some(LegalRepresentative {
            family_name: b"family".to_vec(),
            given_name: b"given".to_vec(),
            cid_number: b"citizen-cid".to_vec(),
            account_id: [9u8; 32],
        }),
        institution_code: *b"NRCG",
        created_at: 7u32,
    };
    assert_eq!(
        value.encode(),
        (
            b"full".to_vec(),
            b"short".to_vec(),
            b"001".to_vec(),
            Some((
                b"family".to_vec(),
                b"given".to_vec(),
                b"citizen-cid".to_vec(),
                [9u8; 32],
            )),
            *b"NRCG",
            7u32,
        )
            .encode()
    );
}
