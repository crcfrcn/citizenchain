//! 原所属模块回归，断言与输入保持。
use super::*;

#[test]
fn national_routing_two_houses_with_legislature() {
    let non_edu = routing_for(1, false).expect("national non-edu");
    assert_eq!(non_edu.houses, vec![*b"NRP\0", *b"NSN\0"]);
    assert_eq!(non_edu.executive, *b"PRS\0");
    assert_eq!(non_edu.legislature, Some(*b"NLG\0"));

    // 国家教育案:发起院换教委会,终审仍参议会。
    let edu = routing_for(1, true).expect("national edu");
    assert_eq!(edu.houses, vec![*b"NED\0", *b"NSN\0"]);
}

#[test]
fn constitution_routes_like_national_non_education() {
    let constitution = routing_for(0, false).expect("constitution amend");
    assert_eq!(constitution.houses, vec![*b"NRP\0", *b"NSN\0"]);
    assert_eq!(constitution.legislature, Some(*b"NLG\0"));
}

#[test]
fn provincial_two_houses_municipal_single_house() {
    let province = routing_for(2, false).expect("provincial");
    assert_eq!(province.houses, vec![*b"PRP\0", *b"PSN\0"]);
    assert_eq!(province.executive, *b"PGV\0");

    // 市级单院,无会签。
    for is_edu in [false, true] {
        let city = routing_for(3, is_edu).expect("municipal");
        assert_eq!(city.houses, vec![*b"CLEG"]);
        assert_eq!(city.executive, *b"CGOV");
        assert_eq!(city.legislature, None);
    }
}

#[test]
fn provincial_education_and_illegal_combos_have_no_routing() {
    // 省无教委会 → 省教育案不存在。
    assert!(routing_for(2, true).is_none());
    // 宪法教育案非法。
    assert!(routing_for(0, true).is_none());
    // 未知层级。
    assert!(routing_for(9, false).is_none());
}

#[test]
fn vote_type_education_flags_match_chain() {
    assert!(!vote_type_is_education(0)); // 常规
    assert!(vote_type_is_education(1)); // 常规教育
    assert!(!vote_type_is_education(2)); // 重要
    assert!(vote_type_is_education(3)); // 重要教育
    assert!(!vote_type_is_education(4)); // 特别
}
