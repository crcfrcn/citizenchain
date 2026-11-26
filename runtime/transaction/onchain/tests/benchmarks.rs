//! 基准测量真实带备注转账，覆盖最大和空备注。

use crate::pallet::{BalanceOf, Call, Config, Event, Pallet, TransferRemarkOf};
use frame_benchmarking::v2::*;
use frame_support::traits::{Currency, Get};
use frame_system::RawOrigin;
use sp_runtime::traits::{CheckedAdd, One, Zero, SaturatedConversion};
use sp_std::vec;

struct Transfer<T: Config> {
    caller: T::AccountId,
    beneficiary: T::AccountId,
    amount: BalanceOf<T>,
    balance: BalanceOf<T>,
}

fn prepare_transfer<T: Config>() -> Result<Transfer<T>, BenchmarkError> {
    frame_system::Pallet::<T>::set_block_number(1u32.into());
    let caller = whitelisted_caller();
    let beneficiary = account("beneficiary", 0, 0);
    if caller == beneficiary {
        return Err(BenchmarkError::Stop("转账基准双方账户必须不同"));
    }
    let minimum = T::Currency::minimum_balance();
    let amount = minimum.max(One::one());
    let maximum_fee: BalanceOf<T> = primitives::fee_policy::calculate_transfer_fee(amount.saturated_into(), true).saturated_into();
    let balance = minimum
        .checked_add(&amount)
        .and_then(|value| value.checked_add(&maximum_fee))
        .ok_or(BenchmarkError::Stop("转账基准余额超出类型范围"))?;
    // 收款账户从零开始，真实执行新账户入账；付款后仍保留最低余额。
    drop(T::Currency::make_free_balance_be(
        &beneficiary,
        Zero::zero(),
    ));
    drop(T::Currency::make_free_balance_be(&caller, balance));
    assert_eq!(T::Currency::free_balance(&caller), balance);
    assert!(T::Currency::free_balance(&beneficiary).is_zero());
    Ok(Transfer {
        caller,
        beneficiary,
        amount,
        balance,
    })
}

fn verify_transfer<T: Config>(transfer: Transfer<T>, remark: TransferRemarkOf<T>) {
    assert_eq!(
        T::Currency::free_balance(&transfer.caller),
        transfer.balance - transfer.amount - BalanceOf::<T>::saturated_from(primitives::fee_policy::calculate_transfer_fee(transfer.amount.saturated_into(), !remark.is_empty())),
    );
    assert_eq!(
        T::Currency::free_balance(&transfer.beneficiary),
        transfer.amount
    );
    frame_system::Pallet::<T>::assert_last_event(
        Event::<T>::TransferWithRemark {
            from_account_id: transfer.caller,
            beneficiary_account_id: transfer.beneficiary,
            amount: transfer.amount,
            remark,
        }
        .into(),
    );
}

#[benchmarks]
pub(crate) mod benchmarks {
    use super::*;

    /// 根目录测试只调用FRAME生成的原验证器，不复制或替代基准实现。
    #[cfg(test)]
    pub(crate) fn test_case<T: Config>(name: &[u8]) -> Result<(), BenchmarkError> {
        Pallet::<T>::test_bench_by_name(name)
    }

    #[benchmark]
    fn transfer_with_remark() -> Result<(), BenchmarkError> {
        let transfer = prepare_transfer::<T>()?;
        let remark: TransferRemarkOf<T> = vec![b'a'; T::MaxTransferRemarkLen::get() as usize]
            .try_into()
            .map_err(|_| BenchmarkError::Stop("最大备注必须满足现行长度上限"))?;

        #[extrinsic_call]
        _(
            RawOrigin::Signed(transfer.caller.clone()),
            transfer.beneficiary.clone(),
            transfer.amount,
            remark.clone(),
        );

        verify_transfer::<T>(transfer, remark);
        Ok(())
    }

    #[benchmark(extra)]
    fn transfer_empty_remark() -> Result<(), BenchmarkError> {
        let transfer = prepare_transfer::<T>()?;
        let remark = TransferRemarkOf::<T>::default();

        #[extrinsic_call]
        transfer_with_remark(
            RawOrigin::Signed(transfer.caller.clone()),
            transfer.beneficiary.clone(),
            transfer.amount,
            remark.clone(),
        );

        verify_transfer::<T>(transfer, remark);
        Ok(())
    }
}
