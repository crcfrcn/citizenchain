//! 原所属模块回归，断言与输入保持。
use super::*;
#[test]
fn existing_private_classes_keep_their_original_codes() {
    for (institution, expected) in [
        ("SFGT", "SOLE"),
        ("SFGP", "PARTNERSHIP"),
        ("SFLP", "PARTNERSHIP"),
        ("SFGQ", "COMPANY"),
        ("SFGF", "CORPORATION"),
        ("SFGY", "WELFARE"),
        ("SFAS", "ASSOCIATION"),
    ] {
        assert_eq!(
            private_type_code_from_institution_code(institution),
            Some(expected)
        );
    }
    assert_eq!(private_type_code_from_institution_code("NLG"), None);
}
