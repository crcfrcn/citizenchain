#![cfg(test)]

use super::*;
use codec::{Decode, Encode};

fn annual_total(year: u32) -> u128 {
    let rate = 100u128 - u128::from(year - 1);
    primitives::cid::china::china_ch::CHINA_CH
        .iter()
        .map(|bank| bank.stake_amount * rate / 10_000)
        .sum()
}

#[test]
fn first_year_mints_all_banks_and_writes_exact_audit() {
    new_test_ext().execute_with(|| {
        run_to_block(10);

        let expected_total = annual_total(1);
        assert_eq!(LastSettledYear::<Test>::get(), 1);
        assert_eq!(
            TotalProvincialBankInterestIssued::<Test>::get(),
            expected_total
        );
        assert_eq!(Balances::total_issuance(), expected_total);
        assert_eq!(
            LastProvincialBankInterestAudit::<Test>::get(),
            Some(ProvincialBankInterestAudit {
                year: 1,
                bank_count: 43,
                total_interest: expected_total,
            })
        );

        for (index, bank) in primitives::cid::china::china_ch::CHINA_CH
            .iter()
            .enumerate()
        {
            assert_eq!(
                Balances::free_balance(provincialbank_account(index)),
                bank.stake_amount * 100 / 10_000
            );
        }
    });
}

#[test]
fn settlement_only_happens_in_finalize() {
    new_test_ext().execute_with(|| {
        System::set_block_number(10);
        let _ = ProvincialBankInterest::on_initialize(10);
        assert_eq!(LastSettledYear::<Test>::get(), 0);
        assert_eq!(Balances::total_issuance(), 0);

        ProvincialBankInterest::on_finalize(10);
        assert_eq!(LastSettledYear::<Test>::get(), 1);
        assert_eq!(Balances::total_issuance(), annual_total(1));
    });
}

#[test]
fn second_year_uses_decayed_rate_and_cumulative_audit() {
    new_test_ext().execute_with(|| {
        run_to_block(20);
        let year_1 = annual_total(1);
        let year_2 = annual_total(2);
        assert_eq!(LastSettledYear::<Test>::get(), 2);
        assert_eq!(
            TotalProvincialBankInterestIssued::<Test>::get(),
            year_1 + year_2
        );
        assert_eq!(Balances::total_issuance(), year_1 + year_2);
        assert_eq!(
            LastProvincialBankInterestAudit::<Test>::get(),
            Some(ProvincialBankInterestAudit {
                year: 2,
                bank_count: 43,
                total_interest: year_2,
            })
        );
    });
}

#[test]
fn missing_previous_year_rolls_back_entire_settlement() {
    new_test_ext().execute_with(|| {
        System::set_block_number(20);
        ProvincialBankInterest::on_finalize(20);

        assert_eq!(LastSettledYear::<Test>::get(), 0);
        assert_eq!(TotalProvincialBankInterestIssued::<Test>::get(), 0);
        assert_eq!(LastProvincialBankInterestAudit::<Test>::get(), None);
        assert_eq!(Balances::total_issuance(), 0);
        for index in 0..primitives::cid::china::china_ch::CHINA_CH.len() {
            assert_eq!(Balances::free_balance(provincialbank_account(index)), 0);
        }
    });
}

#[test]
fn non_boundary_and_zero_period_never_mint() {
    new_test_ext().execute_with(|| {
        System::set_block_number(9);
        ProvincialBankInterest::on_finalize(9);
        assert_eq!(Balances::total_issuance(), 0);

        set_blocks_per_year(0);
        System::set_block_number(10);
        ProvincialBankInterest::on_finalize(10);
        assert_eq!(LastSettledYear::<Test>::get(), 0);
        assert_eq!(Balances::total_issuance(), 0);
    });
}

#[test]
fn interest_goes_to_main_account_not_permanent_stake_account() {
    new_test_ext().execute_with(|| {
        run_to_block(10);
        let bank = &primitives::cid::china::china_ch::CHINA_CH[0];
        let main = provincialbank_account(0);
        let stake = AccountId32::new(bank.stake_account);
        assert_eq!(
            Balances::free_balance(main),
            bank.stake_amount * 100 / 10_000
        );
        assert_eq!(Balances::free_balance(stake), 0);
    });
}

#[test]
fn year_100_is_last_and_uses_one_basis_point() {
    new_test_ext().execute_with(|| {
        // 构造第 99 年已经完整结算的规范审计基准，只定向验证最后一个年度边界。
        let total_99: u128 = (1..=99).map(annual_total).sum();
        LastSettledYear::<Test>::put(99);
        TotalProvincialBankInterestIssued::<Test>::put(total_99);
        LastProvincialBankInterestAudit::<Test>::put(ProvincialBankInterestAudit {
            year: 99,
            bank_count: 43,
            total_interest: annual_total(99),
        });

        System::set_block_number(1_000);
        ProvincialBankInterest::on_finalize(1_000);
        assert_eq!(LastSettledYear::<Test>::get(), 100);
        assert_eq!(
            TotalProvincialBankInterestIssued::<Test>::get(),
            total_99 + annual_total(100)
        );
        assert!(annual_total(100) > 0);

        let total_after_100 = Balances::total_issuance();
        System::set_block_number(1_010);
        ProvincialBankInterest::on_finalize(1_010);
        assert_eq!(LastSettledYear::<Test>::get(), 100);
        assert_eq!(Balances::total_issuance(), total_after_100);
    });
}

#[test]
fn audit_scale_field_order_is_stable_for_node_guard() {
    let audit = ProvincialBankInterestAudit {
        year: 7,
        bank_count: 43,
        total_interest: 123_456,
    };
    let encoded = audit.encode();
    let decoded = <(u32, u32, u128)>::decode(&mut &encoded[..]).expect("字段序必须可解码");
    assert_eq!(decoded, (7, 43, 123_456));
}
