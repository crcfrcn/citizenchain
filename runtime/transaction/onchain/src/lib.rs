#![cfg_attr(not(feature = "std"), no_std)]

use frame_support::traits::{
    fungible::Inspect,
    tokens::{
        fungible::{Balanced, Credit},
        Fortitude, Imbalance, Precision, Preservation,
    },
    Currency, FindAuthor, OnUnbalanced,
};
use frame_support::unsigned::TransactionValidityError;
use pallet_transaction_payment::{Config as TxPaymentConfig, OnChargeTransaction, TxCreditHold};
use sp_runtime::{
    traits::{DispatchInfoOf, PostDispatchInfoOf, SaturatedConversion, Zero},
    transaction_validity::InvalidTransaction,
};
use sp_std::{marker::PhantomData, prelude::*};

/// 链上资金交易 pallet：承载普通转账备注调用和统一手续费审计事件。
#[frame_support::pallet]
pub mod pallet {
    use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
    use frame_support::{
        pallet_prelude::*,
        traits::{Currency, ExistenceRequirement, Get},
    };
    use frame_system::pallet_prelude::*;
    use scale_info::TypeInfo;
    use sp_runtime::{traits::Zero, Debug, Perbill};

    #[pallet::pallet]
    pub struct Pallet<T>(_);

    #[pallet::config]
    pub trait Config: frame_system::Config<RuntimeEvent: From<Event<Self>>> {
        /// 普通链上转账使用的余额系统。
        type Currency: Currency<Self::AccountId>;

        /// 普通转账备注最大 UTF-8 字节数。
        #[pallet::constant]
        type MaxTransferRemarkLen: Get<u32>;

        // ── 以下三项把链上收费参数下发到 metadata,供客户端预估费用 ──
        //
        // 费率真源恒为 `primitives::fee_policy`,runtime 绑定时**只做转发**,本 pallet
        // 与任何客户端都不得另立交易费常量(全仓交易费常量只有区块链常量库一处)。
        // 客户端据此自行计算 `max(amount × OnchainFeeRate, OnchainMinFee)`,不再复刻数字。
        /// 链上交易单笔最低费(分)。
        #[pallet::constant]
        type OnchainMinFee: Get<u128>;

        /// 链上交易费率,按交易金额计费。
        #[pallet::constant]
        type OnchainFeeRate: Get<Perbill>;

        /// 实际投票统一费(分/票)。
        #[pallet::constant]
        type VoteFlatFee: Get<u128>;
    }

    /// 普通转账金额类型。
    pub type BalanceOf<T> =
        <<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;

    /// 普通转账备注，编码为 SCALE BoundedVec<u8>。
    pub type TransferRemarkOf<T> = BoundedVec<u8, <T as Config>::MaxTransferRemarkLen>;

    /// 手续费份额销毁原因，供链上事件审计和运维聚合。
    #[derive(
        Clone,
        Copy,
        Encode,
        Decode,
        DecodeWithMemTracking,
        Eq,
        PartialEq,
        Debug,
        TypeInfo,
        MaxEncodedLen,
    )]
    pub enum BurnReason {
        /// 当前区块作者无法从共识 digest 中识别。
        AuthorMissing,
        /// 区块作者尚未绑定全节点手续费奖励账户。
        RewardAccountUnbound,
        /// 全节点奖励账户入账失败，剩余 credit 被销毁。
        FullnodeResolveFailed,
        /// 国家储委会手续费账户未配置。
        NrcMissing,
        /// 国家储委会手续费账户入账失败，剩余 credit 被销毁。
        NrcResolveFailed,
        /// 安全基金账户入账失败，剩余 credit 被销毁。
        SafetyFundResolveFailed,
    }

    #[pallet::event]
    #[pallet::generate_deposit(pub(crate) fn deposit_event)]
    pub enum Event<T: Config> {
        /// 交易手续费已收取。
        /// `tip` 在协议层永久为零，因此 `fee` 就是本笔完整链上交易费或投票费。
        FeePaid { account_id: T::AccountId, fee: u128 },
        /// 手续费分账份额因无法安全入账而被销毁。
        FeeShareBurnt { reason: BurnReason, amount: u128 },
        /// 普通链上转账已执行，备注随交易事件绑定。
        TransferWithRemark {
            from_account_id: T::AccountId,
            beneficiary_account_id: T::AccountId,
            amount: BalanceOf<T>,
            remark: TransferRemarkOf<T>,
        },
    }

    #[pallet::error]
    pub enum Error<T> {
        /// 转账金额不能为零。
        ZeroAmount,
        /// 不能向自己转账。
        SelfTransferNotAllowed,
        /// 余额模块拒绝转账。
        TransferFailed,
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        /// 普通转账并把备注绑定到同一笔链上交易事件。
        #[pallet::call_index(0)]
        #[pallet::weight(T::DbWeight::get().reads_writes(1, 1))]
        pub fn transfer_with_remark(
            origin: OriginFor<T>,
            beneficiary_account_id: T::AccountId,
            amount: BalanceOf<T>,
            remark: TransferRemarkOf<T>,
        ) -> DispatchResult {
            let from = ensure_signed(origin)?;
            ensure!(!amount.is_zero(), Error::<T>::ZeroAmount);
            ensure!(
                from != beneficiary_account_id,
                Error::<T>::SelfTransferNotAllowed
            );

            T::Currency::transfer(
                &from,
                &beneficiary_account_id,
                amount,
                ExistenceRequirement::KeepAlive,
            )
            .map_err(|_| Error::<T>::TransferFailed)?;

            Self::deposit_event(Event::TransferWithRemark {
                from_account_id: from,
                beneficiary_account_id,
                amount,
                remark,
            });
            Ok(())
        }
    }
}

const EXPECTED_FEE_PERCENT_TOTAL: u32 = 100;

const _: () = {
    let total_percent = primitives::fee_policy::ONCHAIN_FEE_FULLNODE_PERCENT
        .saturating_add(primitives::fee_policy::ONCHAIN_FEE_NRC_PERCENT)
        .saturating_add(primitives::fee_policy::ONCHAIN_FEE_SAFETY_FUND_PERCENT);
    assert!(
        total_percent == EXPECTED_FEE_PERCENT_TOTAL,
        "fee distribution percents must sum to 100"
    );
    assert!(
        primitives::fee_policy::ONCHAIN_MIN_FEE > 0,
        "ONCHAIN_MIN_FEE must be positive"
    );
    assert!(
        primitives::fee_policy::ONCHAIN_FEE_RATE.deconstruct() > 0,
        "ONCHAIN_FEE_RATE must be non-zero"
    );
};

/// 链上交易手续费分配器（统一入口）：
/// - 全节点（绑定账户）分成：`ONCHAIN_FEE_FULLNODE_PERCENT`（80%）
/// - 国家储委会手续费账户分成：`ONCHAIN_FEE_NRC_PERCENT`（10%）
/// - 安全基金账户分成：`ONCHAIN_FEE_SAFETY_FUND_PERCENT`（10%）
pub struct OnchainFeeRouter<T, Currency, AuthorFinder, NrcProvider, SafetyFundProvider>(
    PhantomData<(T, Currency, AuthorFinder, NrcProvider, SafetyFundProvider)>,
);

/// Runtime 唯一路由入口。
///
/// 五类协议及确切付款账户只使用 `primitives::fee_policy::FeeRoute`；本 trait
/// 只让具体 runtime 把 `RuntimeCall` 映射到该唯一类型，不再拆分分类器和付款提取器。
pub trait CallFeeRoute<AccountId, Call, Balance> {
    fn fee_route(
        who: &AccountId,
        call: &Call,
    ) -> primitives::fee_policy::FeeRoute<AccountId, Balance>;
}

/// 统一抽象：由 Runtime 注入国家储委会收款账户来源。
pub trait NrcAccountProvider<AccountId> {
    /// 提供国家储委会收款账户。
    /// 返回 None 时，NRC 份额按安全退化策略直接销毁。
    fn nrc_account() -> Option<AccountId>;
}

impl<AccountId> NrcAccountProvider<AccountId> for () {
    fn nrc_account() -> Option<AccountId> {
        None
    }
}

/// 统一抽象：由 Runtime 注入安全基金收款账户来源。
pub trait SafetyFundAccountProvider<AccountId> {
    /// 提供安全基金账户。
    /// 安全基金账户属于制度常量，使用 provider 可避免在分账热路径反复 decode。
    fn safety_fund_account() -> AccountId;
}

/// 统一手续费收取适配器：
/// - 只有实际投票固定收取 `VOTE_FLAT_FEE`
/// - 机构操作按 actor CID 的唯一费用账户收取最低链上交易费
/// - 链上资金交易按 `ONCHAIN_FEE_RATE` 和 `ONCHAIN_MIN_FEE` 计算
/// - 链下清算手续费由清算模块执行，不重复进入链上手续费分账
/// - 免费调用不扣基础费
pub struct OnchainChargeAdapter<Currency, Router, FeeRouteProvider>(
    PhantomData<(Currency, Router, FeeRouteProvider)>,
);

/// 业务回调执行期链上交易费收取器。
///
/// 外层 extrinsic 继续由 `OnchainChargeAdapter` 消费 `FeeRoute`；投票通过后的
/// 资金执行没有新的外层签名交易，因此由业务模块把已经核验的确切付款账户和
/// 金额交给本执行器。两条路径共用同一公式、同一分账和同一 `FeePaid` 事件。
pub struct OnchainExecutionFeeCharger<T, C, Router>(PhantomData<(T, C, Router)>);

impl<T, C, Router>
    primitives::fee_policy::OnchainFeeCharger<T::AccountId, <C as Currency<T::AccountId>>::Balance>
    for OnchainExecutionFeeCharger<T, C, Router>
where
    T: pallet::Config,
    C: Currency<T::AccountId>,
    Router: OnUnbalanced<<C as Currency<T::AccountId>>::NegativeImbalance>,
    <C as Currency<T::AccountId>>::Balance: SaturatedConversion + Copy + Zero,
{
    fn charge(
        payer_account_id: &T::AccountId,
        transaction_amount: <C as Currency<T::AccountId>>::Balance,
    ) -> Result<<C as Currency<T::AccountId>>::Balance, sp_runtime::DispatchError> {
        let fee_u128 = primitives::fee_policy::calculate_onchain_fee(
            transaction_amount.saturated_into::<u128>(),
        );
        let fee = fee_u128.saturated_into();
        let imbalance = C::withdraw(
            payer_account_id,
            fee,
            frame_support::traits::WithdrawReasons::FEE,
            frame_support::traits::ExistenceRequirement::KeepAlive,
        )?;
        Router::on_unbalanced(imbalance);
        pallet::Pallet::<T>::deposit_event(pallet::Event::FeePaid {
            account_id: payer_account_id.clone(),
            fee: fee_u128,
        });
        Ok(fee)
    }
}

impl<T, Currency, Router, FeeRouteProvider> OnChargeTransaction<T>
    for OnchainChargeAdapter<Currency, Router, FeeRouteProvider>
where
    T: TxPaymentConfig + fullnode_issuance::Config + pallet::Config,
    Currency: Balanced<T::AccountId> + 'static,
    Router: OnUnbalanced<Credit<T::AccountId, Currency>>,
    FeeRouteProvider:
        CallFeeRoute<T::AccountId, T::RuntimeCall, <Currency as Inspect<T::AccountId>>::Balance>,
    T::AccountId: Clone,
{
    type LiquidityInfo = Option<Credit<T::AccountId, Currency>>;
    type Balance = <Currency as Inspect<T::AccountId>>::Balance;

    fn withdraw_fee(
        who: &T::AccountId,
        call: &T::RuntimeCall,
        dispatch_info: &DispatchInfoOf<T::RuntimeCall>,
        _fee_with_tip: Self::Balance,
        tip: Self::Balance,
    ) -> Result<Self::LiquidityInfo, TransactionValidityError> {
        // 框架金额不参与制度收费，实际金额与付款账户只能来自唯一 FeeRoute。
        let Some((payer_account_id, fee)) =
            charge_details::<T, Currency, FeeRouteProvider>(who, call, dispatch_info, tip)?
        else {
            return Ok(None);
        };

        // Exact + Preserve：要么从明确账户完整扣款并保留 ED，要么整笔交易失败。
        let credit = Currency::withdraw(
            &payer_account_id,
            fee,
            Precision::Exact,
            Preservation::Preserve,
            Fortitude::Polite,
        )
        .map_err(|_| InvalidTransaction::Payment)?;

        // 发出链上手续费事件，供手机端 / 浏览器 / node 读取真实手续费。
        let paid_fee: u128 = fee.saturated_into();
        pallet::Pallet::<T>::deposit_event(pallet::Event::FeePaid {
            account_id: payer_account_id.clone(),
            fee: paid_fee,
        });

        Ok(Some(credit))
    }

    fn can_withdraw_fee(
        who: &T::AccountId,
        call: &T::RuntimeCall,
        dispatch_info: &DispatchInfoOf<T::RuntimeCall>,
        _fee_with_tip: Self::Balance,
        tip: Self::Balance,
    ) -> Result<(), TransactionValidityError> {
        let Some((payer_account_id, fee)) =
            charge_details::<T, Currency, FeeRouteProvider>(who, call, dispatch_info, tip)?
        else {
            return Ok(());
        };
        match Currency::can_withdraw(&payer_account_id, fee) {
            frame_support::traits::tokens::WithdrawConsequence::Success => Ok(()),
            _ => Err(InvalidTransaction::Payment.into()),
        }
    }

    fn correct_and_deposit_fee(
        _who: &T::AccountId,
        _dispatch_info: &DispatchInfoOf<T::RuntimeCall>,
        _post_info: &PostDispatchInfoOf<T::RuntimeCall>,
        _corrected_fee_with_tip: Self::Balance,
        _tip: Self::Balance,
        liquidity_info: Self::LiquidityInfo,
    ) -> Result<(), TransactionValidityError> {
        // PROTOCOL: no post-dispatch refund.
        // 本制度按五类费用模型固定收费，协议上明确"不做执行后退款"。
        // `_corrected_fee_with_tip` 和 `_tip` 仅为框架接口参数；tip 已在预检阶段禁用。
        if let Some(fee_credit) = liquidity_info {
            Router::on_unbalanced(fee_credit);
        }
        Ok(())
    }

    #[cfg(feature = "runtime-benchmarks")]
    fn endow_account(who: &T::AccountId, amount: Self::Balance) {
        let _ = Currency::deposit(who, amount, Precision::BestEffort);
    }

    #[cfg(feature = "runtime-benchmarks")]
    fn minimum_balance() -> Self::Balance {
        Currency::minimum_balance()
    }
}

impl<T, Currency, Router, FeeRouteProvider> TxCreditHold<T>
    for OnchainChargeAdapter<Currency, Router, FeeRouteProvider>
where
    T: TxPaymentConfig,
    Currency: Balanced<T::AccountId> + 'static,
{
    type Credit = ();
}

impl<T, Currency, AuthorFinder, NrcProvider, SafetyFundProvider>
    OnUnbalanced<Credit<T::AccountId, Currency>>
    for OnchainFeeRouter<T, Currency, AuthorFinder, NrcProvider, SafetyFundProvider>
where
    T: frame_system::Config + fullnode_issuance::Config + pallet::Config,
    Currency: Balanced<T::AccountId>,
    AuthorFinder: FindAuthor<T::AccountId>,
    NrcProvider: NrcAccountProvider<T::AccountId>,
    SafetyFundProvider: SafetyFundAccountProvider<T::AccountId>,
{
    fn on_nonzero_unbalanced(amount: Credit<T::AccountId, Currency>) {
        let fullnode_percent = primitives::fee_policy::ONCHAIN_FEE_FULLNODE_PERCENT;
        let nrc_percent = primitives::fee_policy::ONCHAIN_FEE_NRC_PERCENT;
        let safety_fund_percent = primitives::fee_policy::ONCHAIN_FEE_SAFETY_FUND_PERCENT;
        let total_percent = fullnode_percent
            .saturating_add(nrc_percent)
            .saturating_add(safety_fund_percent);
        debug_assert_eq!(
            total_percent, EXPECTED_FEE_PERCENT_TOTAL,
            "fee distribution constants must sum to 100"
        );

        // 制度常量异常时，直接全部销毁，避免错误分配。
        if total_percent != EXPECTED_FEE_PERCENT_TOTAL {
            log::error!(
                target: "runtime::onchain",
                "fee distribution percents must sum to {}; got fullnode={}, nrc={}, safety_fund={}, total={}",
                EXPECTED_FEE_PERCENT_TOTAL,
                fullnode_percent,
                nrc_percent,
                safety_fund_percent,
                total_percent
            );
            return;
        }

        // 先切出全节点份额，再把剩余部分在 NRC 和安全基金之间二次切分，
        // 可以避免三项分账时因为整数除法带来更复杂的舍入误差。
        let (fullnode_credit, remainder) = amount.ration(
            fullnode_percent,
            total_percent.saturating_sub(fullnode_percent),
        );
        let (nrc_credit, safety_fund_credit) = remainder.ration(nrc_percent, safety_fund_percent);

        // 手续费全节点分成只发给当前区块作者绑定的奖励接收账户；未绑定则不分配（自动销毁）。
        let digest = <frame_system::Pallet<T>>::digest();
        let pre_runtime_digests = digest.logs().iter().filter_map(|d| d.as_pre_runtime());
        match AuthorFinder::find_author(pre_runtime_digests) {
            Some(miner_account_id) => {
                if let Some(reward_account_id) =
                    fullnode_issuance::RewardAccountIdByMiner::<T>::get(&miner_account_id)
                {
                    if let Err(remaining) = Currency::resolve(&reward_account_id, fullnode_credit) {
                        let burnt_amount = remaining.peek().saturated_into::<u128>();
                        log::warn!(
                            target: "runtime::onchain",
                            "burn fullnode fee share: failed to resolve reward account credit: {:?}",
                            remaining.peek()
                        );
                        emit_fee_share_burn::<T>(
                            pallet::BurnReason::FullnodeResolveFailed,
                            burnt_amount,
                        );
                    }
                } else {
                    let burnt_amount = fullnode_credit.peek().saturated_into::<u128>();
                    log::warn!(
                        target: "runtime::onchain",
                        "burn fullnode fee share: author found but reward account not bound"
                    );
                    emit_fee_share_burn::<T>(
                        pallet::BurnReason::RewardAccountUnbound,
                        burnt_amount,
                    );
                    drop(fullnode_credit);
                }
            }
            None => {
                let burnt_amount = fullnode_credit.peek().saturated_into::<u128>();
                log::warn!(
                    target: "runtime::onchain",
                    "burn fullnode fee share: block author not found"
                );
                emit_fee_share_burn::<T>(pallet::BurnReason::AuthorMissing, burnt_amount);
                drop(fullnode_credit);
            }
        }

        // 国家储委会手续费账户分成。
        if let Some(nrc_fee_account) = NrcProvider::nrc_account() {
            if let Err(remaining) = Currency::resolve(&nrc_fee_account, nrc_credit) {
                let burnt_amount = remaining.peek().saturated_into::<u128>();
                log::warn!(
                    target: "runtime::onchain",
                    "nrc fee share: failed to resolve nrc fee account credit: {:?}",
                    remaining.peek()
                );
                emit_fee_share_burn::<T>(pallet::BurnReason::NrcResolveFailed, burnt_amount);
            }
        } else {
            let burnt_amount = nrc_credit.peek().saturated_into::<u128>();
            log::warn!(
                target: "runtime::onchain",
                "nrc fee share: nrc fee account not configured"
            );
            emit_fee_share_burn::<T>(pallet::BurnReason::NrcMissing, burnt_amount);
            drop(nrc_credit);
        }

        // 安全基金账户由 runtime provider 注入，避免每笔手续费分账重复 decode 常量地址。
        let safety_fund_account = SafetyFundProvider::safety_fund_account();
        if let Err(remaining) = Currency::resolve(&safety_fund_account, safety_fund_credit) {
            let burnt_amount = remaining.peek().saturated_into::<u128>();
            log::warn!(
                target: "runtime::onchain",
                "safety fund fee share: failed to resolve credit: {:?}",
                remaining.peek()
            );
            emit_fee_share_burn::<T>(pallet::BurnReason::SafetyFundResolveFailed, burnt_amount);
        }
    }
}

fn emit_fee_share_burn<T: pallet::Config>(reason: pallet::BurnReason, amount: u128) {
    if amount == 0 {
        return;
    }
    pallet::Pallet::<T>::deposit_event(pallet::Event::FeeShareBurnt { reason, amount });
}

type ChargeDetails<AccountId, Balance> = Option<(AccountId, Balance)>;

// 费用计算返回类型同时绑定 runtime 账户和 Currency 余额，保持两者的编译期一致性。
#[allow(clippy::type_complexity)]
fn charge_details<T, Currency, FeeRouteProvider>(
    who: &T::AccountId,
    call: &T::RuntimeCall,
    _dispatch_info: &DispatchInfoOf<T::RuntimeCall>,
    tip: <Currency as Inspect<T::AccountId>>::Balance,
) -> Result<
    ChargeDetails<T::AccountId, <Currency as Inspect<T::AccountId>>::Balance>,
    TransactionValidityError,
>
where
    T: TxPaymentConfig,
    Currency: Balanced<T::AccountId>,
    FeeRouteProvider:
        CallFeeRoute<T::AccountId, T::RuntimeCall, <Currency as Inspect<T::AccountId>>::Balance>,
{
    // tip 不属于交易费；非零 tip 在入池预检和区块执行中都直接拒绝。
    if !tip.is_zero() {
        return Err(InvalidTransaction::Payment.into());
    }

    use primitives::fee_policy::FeeRoute;
    match FeeRouteProvider::fee_route(who, call) {
        FeeRoute::Onchain {
            transaction_amount,
            payer_account_id,
        } => {
            // 只有链上交易费按金额套 0.1% 费率；机构普通操作传零金额，固定落最低费。
            let amount_u128: u128 = transaction_amount.saturated_into();
            let fee_u128 = primitives::fee_policy::calculate_onchain_fee(amount_u128);
            let fee = fee_u128.saturated_into();
            Ok(Some((payer_account_id, fee)))
        }
        FeeRoute::Vote { payer_account_id } => {
            let fee = primitives::fee_policy::VOTE_FLAT_FEE.saturated_into();
            Ok(Some((payer_account_id, fee)))
        }
        FeeRoute::Offchain { .. } => {
            // 链下费用由 offchain 执行器按同一个路由结果和批次规则收取，
            // 不进入链上 80/10/10 分账，当前适配器不得重复扣款。
            Ok(None)
        }
        FeeRoute::Free => Ok(None),
        FeeRoute::Reject => Err(InvalidTransaction::Call.into()),
    }
}

#[cfg(test)]
mod tests;
