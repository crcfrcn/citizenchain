use crate::constants::EXPECTED_SS58_PREFIX;
use crate::constants::SS58_PREFIX;

#[test]
fn ss58_prefix_values_consistent() {
    assert_eq!(SS58_PREFIX as u64, EXPECTED_SS58_PREFIX);
}

#[test]
fn ss58_prefix_is_2027() {
    assert_eq!(SS58_PREFIX, 2027);
}
