//! 原所属模块回归，断言与输入保持。
use super::scope_label;

#[test]
fn scope_label_joins_province_and_city() {
    assert_eq!(
        scope_label(Some("广东省"), Some("深圳市")),
        "广东省 · 深圳市"
    );
}

#[test]
fn scope_label_national_when_empty() {
    assert_eq!(scope_label(None, None), "全国");
    assert_eq!(scope_label(Some(" "), Some("")), "全国");
}

#[test]
fn scope_label_province_only() {
    assert_eq!(scope_label(Some("广东省"), None), "广东省");
}
