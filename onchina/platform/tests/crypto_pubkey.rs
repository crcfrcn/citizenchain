//! 原所属模块回归，断言与输入保持。
use super::*;

const ACCOUNT_ID: &str = "0x1111111111111111111111111111111111111111111111111111111111111111";

#[test]
fn account_id_accepts_only_canonical_text() {
    assert_eq!(
        normalize_account_id(ACCOUNT_ID).as_deref(),
        Some(ACCOUNT_ID)
    );
    assert!(normalize_account_id(&ACCOUNT_ID[2..]).is_none());
    assert!(normalize_account_id(&ACCOUNT_ID.to_uppercase()).is_none());
    assert!(normalize_account_id("5GrwvaEF5zXb26Fz9rcQpDWSQ57Cr4NMh6t7vY6zT").is_none());
}

#[test]
fn account_comparison_has_no_case_or_prefix_fallback() {
    assert!(same_account_id(ACCOUNT_ID, ACCOUNT_ID));
    assert!(!same_account_id(ACCOUNT_ID, &ACCOUNT_ID.to_uppercase()));
    assert!(!same_account_id(ACCOUNT_ID, &ACCOUNT_ID[2..]));
}

#[test]
fn ss58_is_derived_without_changing_account_id() {
    let ss58_address = account_id_to_ss58(ACCOUNT_ID).expect("规范账户应能派生展示地址");
    assert!(!ss58_address.starts_with("0x"));
    assert_eq!(
        normalize_account_id(ACCOUNT_ID).as_deref(),
        Some(ACCOUNT_ID)
    );
}
