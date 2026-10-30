//! 宽整数参考值与随机边界补充固定费用、地域金标。
use crate::{fee_policy::calculate_onchain_fee, genesis::LawRegion};
use codec::{DecodeAll, Encode};
use proptest::prelude::*;
use sp_core::U256;

proptest! {
    #![proptest_config(ProptestConfig {cases: 256, failure_persistence: None, rng_seed: proptest::test_runner::RngSeed::Fixed(20261029), ..ProptestConfig::default()})]
    #[test]
    fn fees_match_independent_wide_integer_reference(amount in any::<u128>(), other in any::<u128>()) {
        let expected = ((U256::from(amount) * U256::from(1_000_000u64)
            + U256::from(500_000_000u64)) / U256::from(1_000_000_000u64)).low_u128().max(10);
        prop_assert_eq!(calculate_onchain_fee(amount), expected);
        prop_assert!(calculate_onchain_fee(amount.min(other)) <= calculate_onchain_fee(amount.max(other)));
    }
    #[test]
    fn law_region_is_four_bytes_roundtrips_and_rejects_tail(province in any::<[u8;2]>(), city in any::<u16>()) {
        let region=LawRegion {province_code:province,city_number:city};
        let encoded=region.encode();
        prop_assert_eq!(&encoded[..2], &province);
        prop_assert_eq!(&encoded[2..], &city.to_le_bytes());
        prop_assert_eq!(LawRegion::decode_all(&mut &encoded[..]).unwrap(),region);
        let mut damaged=encoded;damaged.push(0);
        prop_assert!(LawRegion::decode_all(&mut &damaged[..]).is_err());
    }
}
