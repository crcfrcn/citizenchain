//! 任意合法金额和备注的直接转账守恒；实际签名交易费另由Runtime集成验证。
use super::*;
use proptest::prelude::*;

proptest! {
    #![proptest_config(ProptestConfig {cases: 128, failure_persistence: None, rng_seed: proptest::test_runner::RngSeed::Fixed(20261029), ..ProptestConfig::default()})]
    #[test]
    fn transfer_preserves_total_issuance_and_exact_remark(amount in 1u128..1000, bytes in proptest::collection::vec(any::<u8>(),0..100)) {
        new_test_ext().execute_with(|| {
            let from=account(1);let to=account(2);
            let issuance=Balances::total_issuance();
            let from_before=Balances::free_balance(&from);let to_before=Balances::free_balance(&to);
            let remark=crate::pallet::TransferRemarkOf::<Test>::try_from(bytes.clone()).expect("99字节以内备注");
            assert_ok!(OnchainTransaction::transfer_with_remark(RuntimeOrigin::signed(from.clone()),to.clone(),amount,remark));
            assert_eq!(Balances::free_balance(&from),from_before-amount);
            assert_eq!(Balances::free_balance(&to),to_before+amount);
            assert_eq!(Balances::total_issuance(),issuance);
            assert!(System::events().iter().any(|e| matches!(&e.event,
                RuntimeEvent::OnchainTransaction(pallet::Event::TransferWithRemark {remark,..}) if remark.as_slice()==bytes.as_slice())));
        });
    }
}
