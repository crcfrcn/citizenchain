//! 保留空业务pallet合同，不能因源码拆分出现可构造的业务调用。
use super::*;
use codec::Decode;

#[test]
fn empty_pallet_has_no_decodable_business_call_for_any_index() {
    for index in 0u8..=255 {
        assert!(crate::Call::<Test>::decode(&mut &[index][..]).is_err());
    }
}
