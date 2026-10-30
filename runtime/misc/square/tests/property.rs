//! 月历续费保持日内时刻，不能退化成固定30天或逐次累积毫秒误差。
use super::*;
use proptest::prelude::*;

proptest! {
    #![proptest_config(ProptestConfig {cases: 128, failure_persistence: None, rng_seed: proptest::test_runner::RngSeed::Fixed(20261029), ..ProptestConfig::default()})]
    #[test]
    fn monthly_period_preserves_clock_and_has_real_calendar_length(now in 946_684_800_000u64..2_524_608_000_000u64) {
        let next=crate::subscription::add_calendar_period(now,BillingPeriod::Monthly).expect("测试日期可加月");
        let day=86_400_000u64;
        prop_assert!(next>now);
        prop_assert!((28*day..=31*day).contains(&(next-now)));
        prop_assert_eq!(next%day,now%day);
    }
}
