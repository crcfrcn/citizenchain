#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(feature = "runtime-benchmarks")]
mod benchmarks;
pub mod weights;

pub use pallet::*;
pub use weights::WeightInfo;

#[frame_support::pallet]
pub mod pallet {
    use crate::weights::WeightInfo;
    use codec::Decode;
    use frame_support::{
        pallet_prelude::*,
        storage::{transactional::with_transaction_opaque_err, TransactionOutcome},
        traits::Currency,
        weights::Weight,
    };
    use frame_system::pallet_prelude::*;
    use sp_runtime::traits::SaturatedConversion;

    use primitives::{
        cid::china::china_ch::CHINA_CH,
        core_const::{
            ENABLE_PROVINCIALBANK_INTEREST_DECAY, PROVINCIALBANK_INITIAL_INTEREST_BP,
            PROVINCIALBANK_INTEREST_DECREASE_BP, PROVINCIALBANK_INTEREST_DURATION_YEARS,
        },
    };

    const BASIS_POINTS_DENOMINATOR: u128 = 10_000;

    // 省储行利息是节点永久规则：逐年递减必须始终开启，runtime 不保留关闭分支。
    const _: () = assert!(
        ENABLE_PROVINCIALBANK_INTEREST_DECAY,
        "ENABLE_PROVINCIALBANK_INTEREST_DECAY must stay true"
    );

    #[pallet::config]
    pub trait Config: frame_system::Config {
        /// 省储行利息只铸造 CitizenChain 原生 u128 余额。
        type Currency: Currency<Self::AccountId, Balance = u128>;

        /// 一个制度年度对应的区块数。
        #[pallet::constant]
        type BlocksPerYear: Get<u64>;

        type WeightInfo: crate::weights::WeightInfo;
    }

    /// 单年度固定发行审计。收款集合与逐户本金来自 `CHINA_CH`，不重复写入可变 storage。
    #[derive(
        Encode, Decode, DecodeWithMemTracking, Clone, PartialEq, Eq, Debug, TypeInfo, MaxEncodedLen,
    )]
    pub struct ProvincialBankInterestAudit {
        pub year: u32,
        pub bank_count: u32,
        pub total_interest: u128,
    }

    /// 已完成结算的最后年度；0 表示尚未结算。
    #[pallet::storage]
    #[pallet::getter(fn last_settled_year)]
    pub type LastSettledYear<T> = StorageValue<_, u32, ValueQuery>;

    /// 省储行年度利息累计发行量。
    #[pallet::storage]
    #[pallet::getter(fn total_provincialbank_interest_issued)]
    pub type TotalProvincialBankInterestIssued<T> = StorageValue<_, u128, ValueQuery>;

    /// 最近一次成功结算的年度审计；创世和首个年度前必须不存在。
    #[pallet::storage]
    #[pallet::getter(fn last_provincialbank_interest_audit)]
    pub type LastProvincialBankInterestAudit<T> =
        StorageValue<_, ProvincialBankInterestAudit, OptionQuery>;

    #[pallet::pallet]
    pub struct Pallet<T>(_);

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        /// 单个固定省储行主账户收到本年度利息。
        ProvincialBankInterestMinted {
            year: u32,
            account_id: T::AccountId,
            amount: u128,
        },
        /// 43 家省储行本年度全部成功入账，审计状态已原子推进。
        ProvincialBankYearSettled {
            year: u32,
            bank_count: u32,
            total_interest: u128,
        },
    }

    #[derive(Debug)]
    enum SettlementError {
        InvalidBlocksPerYear,
        PreviousYearMismatch,
        AccountDecodeFailed,
        InterestOverflow,
        TotalInterestOverflow,
    }

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
        fn on_initialize(n: BlockNumberFor<T>) -> Weight {
            // 实际铸发在 finalize，年度边界块须在 initialize 阶段预留完整 benchmark 权重。
            if Self::settlement_year(n).is_some() {
                T::WeightInfo::on_finalize_settlement()
            } else {
                Weight::zero()
            }
        }

        fn on_finalize(n: BlockNumberFor<T>) {
            let Some(year) = Self::settlement_year(n) else {
                return;
            };

            let result = with_transaction_opaque_err(|| match Self::settle_year(year) {
                Ok(()) => TransactionOutcome::Commit(Ok(())),
                Err(error) => TransactionOutcome::Rollback(Err(error)),
            });
            match result {
                Ok(Ok(())) => {}
                Ok(Err(error)) => {
                    // 固定发行失败时不允许部分入账；NodeGuard 会因缺少精确年度发行而拒绝本块。
                    log::error!(
                        target: "runtime::provincialbank",
                        "省储行固定年度利息结算失败 | year={} | error={:?}",
                        year,
                        error,
                    );
                }
                Err(()) => log::error!(
                    target: "runtime::provincialbank",
                    "省储行固定年度利息结算无法创建存储事务 | year={}",
                    year,
                ),
            }
        }

        #[cfg(feature = "try-runtime")]
        fn try_state(n: BlockNumberFor<T>) -> Result<(), sp_runtime::TryRuntimeError> {
            let block = n.saturated_into::<u64>();
            let per_year = T::BlocksPerYear::get();
            frame_support::ensure!(per_year > 0, "BlocksPerYear 必须大于零");
            let expected_year =
                ((block / per_year) as u32).min(PROVINCIALBANK_INTEREST_DURATION_YEARS);
            let last = LastSettledYear::<T>::get();
            frame_support::ensure!(last == expected_year, "LastSettledYear 与区块高度不一致");
            frame_support::ensure!(
                TotalProvincialBankInterestIssued::<T>::get()
                    == Self::expected_total_through_year(last).ok_or("累计省储行利息计算溢出")?,
                "累计省储行利息不一致"
            );
            if last == 0 {
                frame_support::ensure!(
                    LastProvincialBankInterestAudit::<T>::get().is_none(),
                    "首个年度前不得存在省储行利息审计"
                );
            } else {
                let expected = Self::expected_audit(last).ok_or("省储行利息审计计算溢出")?;
                frame_support::ensure!(
                    LastProvincialBankInterestAudit::<T>::get() == Some(expected),
                    "最近省储行利息审计不一致"
                );
            }
            Ok(())
        }
    }

    impl<T: Config> Pallet<T> {
        /// 只有第 1..=100 个年度边界块需要执行固定利息发行。
        fn settlement_year(n: BlockNumberFor<T>) -> Option<u32> {
            let block = n.saturated_into::<u64>();
            let per_year = T::BlocksPerYear::get();
            if per_year == 0 || block == 0 || block % per_year != 0 {
                return None;
            }
            let year = u32::try_from(block / per_year).ok()?;
            (year <= PROVINCIALBANK_INTEREST_DURATION_YEARS).then_some(year)
        }

        fn interest_bp_for_year(year: u32) -> u32 {
            let decay = year
                .saturating_sub(1)
                .saturating_mul(PROVINCIALBANK_INTEREST_DECREASE_BP);
            PROVINCIALBANK_INITIAL_INTEREST_BP.saturating_sub(decay)
        }

        fn interest_for_principal(principal: u128, year: u32) -> Option<u128> {
            principal
                .checked_mul(u128::from(Self::interest_bp_for_year(year)))
                .map(|gross| gross / BASIS_POINTS_DENOMINATOR)
        }

        fn expected_audit(year: u32) -> Option<ProvincialBankInterestAudit> {
            let total_interest = CHINA_CH.iter().try_fold(0u128, |total, bank| {
                total.checked_add(Self::interest_for_principal(bank.stake_amount, year)?)
            })?;
            Some(ProvincialBankInterestAudit {
                year,
                bank_count: u32::try_from(CHINA_CH.len()).ok()?,
                total_interest,
            })
        }

        #[cfg(feature = "try-runtime")]
        fn expected_total_through_year(last_year: u32) -> Option<u128> {
            (1..=last_year).try_fold(0u128, |total, year| {
                total.checked_add(Self::expected_audit(year)?.total_interest)
            })
        }

        fn settle_year(year: u32) -> Result<(), SettlementError> {
            if T::BlocksPerYear::get() == 0 {
                return Err(SettlementError::InvalidBlocksPerYear);
            }
            if LastSettledYear::<T>::get() != year.saturating_sub(1) {
                return Err(SettlementError::PreviousYearMismatch);
            }

            let expected_audit =
                Self::expected_audit(year).ok_or(SettlementError::TotalInterestOverflow)?;
            let mut issued = 0u128;
            for bank in CHINA_CH.iter() {
                let account = T::AccountId::decode(&mut &bank.main_account[..])
                    .map_err(|_| SettlementError::AccountDecodeFailed)?;
                let interest = Self::interest_for_principal(bank.stake_amount, year)
                    .ok_or(SettlementError::InterestOverflow)?;
                let _imbalance = T::Currency::deposit_creating(&account, interest);
                issued = issued
                    .checked_add(interest)
                    .ok_or(SettlementError::TotalInterestOverflow)?;
                Self::deposit_event(Event::<T>::ProvincialBankInterestMinted {
                    year,
                    account_id: account,
                    amount: interest,
                });
            }
            if issued != expected_audit.total_interest {
                return Err(SettlementError::TotalInterestOverflow);
            }

            let total = TotalProvincialBankInterestIssued::<T>::get()
                .checked_add(issued)
                .ok_or(SettlementError::TotalInterestOverflow)?;
            TotalProvincialBankInterestIssued::<T>::put(total);
            LastSettledYear::<T>::put(year);
            LastProvincialBankInterestAudit::<T>::put(expected_audit.clone());
            Self::deposit_event(Event::<T>::ProvincialBankYearSettled {
                year,
                bank_count: expected_audit.bank_count,
                total_interest: expected_audit.total_interest,
            });
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests;
