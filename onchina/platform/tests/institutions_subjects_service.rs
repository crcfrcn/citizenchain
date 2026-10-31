//! 原所属模块回归，断言与输入保持。
use super::*;

#[test]
fn validate_account_name_basic() {
    assert!(validate_account_name("").is_err());
    assert!(validate_account_name("   ").is_err());
    assert!(validate_account_name("办案账户").is_ok());
    let too_long = "x".repeat(31);
    assert!(validate_account_name(&too_long).is_err());
}
