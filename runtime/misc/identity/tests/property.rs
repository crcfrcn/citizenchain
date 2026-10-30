//! 日历随机边界必须保持合法性、单调性，并限制在唯一日期空间内。
use super::*;
use proptest::prelude::*;

proptest! {
    #![proptest_config(ProptestConfig {cases: 256, failure_persistence: None, rng_seed: proptest::test_runner::RngSeed::Fixed(20261029), ..ProptestConfig::default()})]
    #[test]
    fn valid_calendar_successor_is_valid_and_increases(year in 1600u32..9999, month in 1u32..13, day in 1u32..32) {
        let current=year*10000+month*100+day;
        prop_assume!(CitizenIdentity::is_plausible_yyyymmdd(current));
        new_test_ext().execute_with(|| {
            let next=CitizenIdentity::next_calendar_date(current).expect("范围内日期有后继");
            assert!(CitizenIdentity::is_plausible_yyyymmdd(next));
            assert!(next>current);
            if day<28 {assert_eq!(next,current+1);}
            if month==12&&day==31 {assert_eq!(next,(year+1)*10000+101);}
        });
    }
}
