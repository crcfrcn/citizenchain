use crate::proof::build_runtime_call;
use codec::DecodeAll;

#[test]
fn both_call_indices_decode_with_runtime_contract() {
    let common_signature = [7u8; 64];
    let emergency = build_runtime_call(
        "LN001-NRC0G-944805165-2026",
        [3u8; 32],
        0,
        60,
        common_signature,
        None,
    )
    .expect("emergency call");
    assert_eq!(emergency[1], 0);
    assert!(runtime::RuntimeCall::decode_all(&mut emergency.as_slice()).is_ok());

    let routine = build_runtime_call(
        "LN001-NRC0G-944805165-2026",
        [4u8; 32],
        0,
        60,
        common_signature,
        Some([8u8; 64]),
    )
    .expect("routine call");
    assert_eq!(routine[1], 1);
    assert!(runtime::RuntimeCall::decode_all(&mut routine.as_slice()).is_ok());
}
