//! 原所属模块回归，断言与输入保持。
use super::*;

#[test]
fn platform_price_call_has_exact_indices_and_field_order() {
    let cid = format!(
        "GD001-SFGQ{}-000000001-2026",
        primitives::cid::number::checksum_char_m1("GD001SFGQ0000000012026", true)
    );
    assert!(onchina::codes::validate_cid_number_format(&cid).is_ok());
    let call = build_propose_platform_price_call(
        &cid,
        "GENESIS_PRODUCT_MANAGER",
        PlatformMembershipLevel::Democracy,
        123_456,
    )
    .expect("build call");
    assert_eq!(&call[..2], &[34, 5]);
    assert_eq!(call[2], (cid.len() as u8) << 2);
    assert_eq!(&call[3..3 + cid.len()], cid.as_bytes());
    let role = "GENESIS_PRODUCT_MANAGER";
    let role_offset = 3 + cid.len();
    assert_eq!(call[role_offset], (role.len() as u8) << 2);
    assert_eq!(
        &call[role_offset + 1..role_offset + 1 + role.len()],
        role.as_bytes()
    );
    let level_offset = role_offset + 1 + role.len();
    assert_eq!(call[level_offset], 1);
    assert_eq!(&call[level_offset + 1..], &123_456_u128.to_le_bytes());
}

#[test]
fn zero_price_is_rejected() {
    assert!(build_propose_platform_price_call(
        &format!(
            "GD001-SFGQ{}-000000001-2026",
            primitives::cid::number::checksum_char_m1("GD001SFGQ0000000012026", true)
        ),
        "GENESIS_PRODUCT_MANAGER",
        PlatformMembershipLevel::Freedom,
        0,
    )
    .is_err());
}
