//! 随所属功能迁移的原有回归，输入与断言保持不变。
use super::{
    build_passport_no, passport_city_namespace, passport_valid_until, passport_validity_years,
    PASSPORT_CITY_NAMESPACE_COUNT, PASSPORT_LOCAL_CAPACITY,
};
use chrono::{TimeZone, Utc};

#[test]
fn passport_city_namespace_is_unique_for_city_codes_in_scope() {
    let mut seen = std::collections::BTreeSet::new();
    for city in 1..=392 {
        let code = format!("{city:03}");
        let namespace = passport_city_namespace("GD", &code).expect("namespace");
        assert!(namespace < PASSPORT_CITY_NAMESPACE_COUNT);
        assert!(seen.insert(namespace));
    }
}

#[test]
fn passport_no_uses_province_body_and_check_char() {
    let no = build_passport_no("gd", "001", 1).expect("passport no");
    assert_eq!(no.len(), 11);
    assert!(no.starts_with("GD"));
    assert!(no
        .chars()
        .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()));
}

#[test]
fn passport_no_supports_last_local_sequence() {
    let no =
        build_passport_no("GD", "392", PASSPORT_LOCAL_CAPACITY as i64).expect("passport no");
    assert_eq!(no.len(), 11);
}

#[test]
fn passport_validity_switches_at_sixteen() {
    let created = Utc.with_ymd_and_hms(2026, 6, 30, 12, 0, 0).unwrap();
    assert_eq!(
        passport_validity_years(
            created,
            chrono::NaiveDate::from_ymd_opt(2010, 6, 30).unwrap()
        ),
        10
    );
    assert_eq!(
        passport_validity_years(
            created,
            chrono::NaiveDate::from_ymd_opt(2010, 7, 1).unwrap()
        ),
        5
    );
    assert_eq!(passport_valid_until(created, 10), "2036-06-29");
}
