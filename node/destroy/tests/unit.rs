use crate::decode::decode_resolution_destroy_action;
use codec::Encode;

pub(super) fn payload(amount: u128) -> Vec<u8> {
    let mut bytes = b"res-dst".to_vec();
    bytes.extend(b"test-cid".to_vec().encode());
    bytes.extend([34u8; 32]);
    bytes.extend(amount.encode());
    bytes
}

#[test]
fn destroy_decoding_rejects_every_truncation_and_wrong_owner_tag() {
    let bytes = payload(111);
    for n in 0..bytes.len() {
        assert!(
            decode_resolution_destroy_action(7, &bytes[..n]).is_none(),
            "truncation {n}"
        );
    }
    let mut wrong = bytes;
    wrong[0] = 0;
    assert!(decode_resolution_destroy_action(7, &wrong).is_none());
}

#[test]
fn destroy_json_keeps_exact_account_and_string_amount() {
    let detail = decode_resolution_destroy_action(7, &payload(u128::MAX)).unwrap();
    let json = serde_json::to_value(detail).unwrap();
    assert_eq!(json["proposalId"], 7);
    assert_eq!(json["actorCidNumber"], "test-cid");
    assert_eq!(
        json["institutionAccountId"],
        format!("0x{}", "22".repeat(32))
    );
    assert_eq!(json["amountFen"], u128::MAX.to_string());
    assert_eq!(json.as_object().unwrap().len(), 4);
}

#[test]
fn destroy_decoder_rejects_trailing_bytes_for_every_amount_boundary() {
    for amount in [0, 1, 111, u128::MAX] {
        let valid = payload(amount);
        assert_eq!(
            decode_resolution_destroy_action(7, &valid)
                .unwrap()
                .amount_fen,
            amount.to_string()
        );
        for tail in [vec![0], vec![255], vec![0; 16], vec![255; 64]] {
            let mut malformed = valid.clone();
            malformed.extend(tail);
            assert!(decode_resolution_destroy_action(7, &malformed).is_none());
        }
    }
}
