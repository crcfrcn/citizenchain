//! 原所属模块回归，断言与输入保持。
use super::*;

#[test]
fn association_keeps_instance_p1_while_fixed_types_define_p1() {
    let association = resolve_private_type_rule("ASSOCIATION", None).expect("association");
    assert_eq!(association.institution_code, "SFAS");
    assert_eq!(association.p1, None);

    let company = resolve_private_type_rule("COMPANY", None).expect("company");
    assert_eq!(company.p1, Some("1"));
    let welfare = resolve_private_type_rule("WELFARE", None).expect("welfare");
    assert_eq!(welfare.p1, Some("0"));
}
