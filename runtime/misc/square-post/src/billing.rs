//! runtime 内部真实公历自动扣款引擎。
//!
//! 用户签名订阅后，链上订阅状态就是持续扣款授权；只有签名取消才撤销。每个区块结束时
//! 使用已经写入的共识时间戳处理到期调度，不需要 CitizenApp、Cloudflare 或外部交易。

use crate::{
    pallet::{
        BalanceOf, CidNumberOf, Config, CreatorPlans, CreatorTierNames, Error, Event, IssuerKeyOf,
        Pallet, RenewalIndex, RenewalSchedule, SubKeyOf, Subscriptions,
    },
    subscription::{
        add_calendar_period, CreatorTier, CreatorTierInput, CreatorTiers, IssuerKey,
        SubscriptionPlan, SubscriptionState, SubscriptionStatus, SuspendReason, TierId, TierName,
    },
};
use frame_support::{
    ensure,
    storage::with_storage_layer,
    traits::{Currency, ExistenceRequirement},
};
use sp_runtime::{traits::SaturatedConversion, DispatchResult};
use sp_std::vec::Vec;

impl<T: Config> Pallet<T> {
    /// 订阅并立即完成首次扣款。已有 Active 同计划幂等；已取消但尚未到期时恢复调度且不重扣。
    pub(crate) fn do_subscribe(
        subscriber_account_id: T::AccountId,
        issuer: IssuerKeyOf<T>,
        plan: SubscriptionPlan,
        expected_price_fen: u128,
    ) -> DispatchResult {
        let subscriber_cid_number = Self::active_cid_number_for_account(&subscriber_account_id)?;
        if let IssuerKey::Creator(creator_cid_number) = &issuer {
            ensure!(
                creator_cid_number != &subscriber_cid_number,
                Error::<T>::CannotSubscribeSelf
            );
        }
        let now = Self::now_ms();
        let key = (subscriber_cid_number.clone(), issuer.clone());
        if let Some(mut state) = Subscriptions::<T>::get(&key) {
            if state.subscription_status == SubscriptionStatus::Active {
                ensure!(state.plan == plan, Error::<T>::TermsLocked);
                // 创作者改价后、到期前再签名：仅更新已授权价、保持 Active、不即时扣款；下期按新价扣。
                if matches!(issuer, IssuerKey::Creator(_)) {
                    let (current_price, _) = Self::current_price_and_payee(&issuer, &plan, now)?;
                    if current_price != state.authorized_price_fen {
                        ensure!(
                            expected_price_fen == current_price,
                            Error::<T>::SignedPriceChanged
                        );
                        state.authorized_price_fen = current_price;
                        Subscriptions::<T>::insert(&key, state);
                        Self::deposit_event(Event::SubscriptionReconsented {
                            subscriber_cid_number,
                            issuer,
                            authorized_price_fen: current_price,
                        });
                        return Ok(());
                    }
                }
                return Ok(());
            }
            if state.subscription_status == SubscriptionStatus::Cancelled && now < state.paid_until
            {
                ensure!(state.plan == plan, Error::<T>::TermsLocked);
                state.subscription_status = SubscriptionStatus::Active;
                state.suspend_reason = None;
                Subscriptions::<T>::insert(&key, state.clone());
                Self::schedule_renewal(&key, state.paid_until);
                Self::deposit_event(Event::SubscriptionResumed {
                    subscriber_cid_number,
                    issuer,
                    paid_until: state.paid_until,
                });
                return Ok(());
            }
        }
        Self::charge_and_schedule(
            subscriber_cid_number,
            subscriber_account_id,
            issuer,
            plan,
            Some(expected_price_fen),
        )
    }

    /// 首次订阅或挂起后再签名恢复的原子扣款路径；新周期从现在起算。
    fn charge_and_schedule(
        subscriber_cid_number: CidNumberOf<T>,
        subscriber_account_id: T::AccountId,
        issuer: IssuerKeyOf<T>,
        plan: SubscriptionPlan,
        expected_price_fen: Option<u128>,
    ) -> DispatchResult {
        with_storage_layer(|| -> DispatchResult {
            let now = Self::now_ms();
            let (price_fen, payee) = Self::current_price_and_payee(&issuer, &plan, now)?;
            if let Some(expected) = expected_price_fen {
                ensure!(expected == price_fen, Error::<T>::SignedPriceChanged);
            }
            let paid_until = add_calendar_period(now, plan.billing_period())
                .ok_or(Error::<T>::CalendarOverflow)?;
            let amount: BalanceOf<T> = price_fen.saturated_into();
            T::Currency::transfer(
                &subscriber_account_id,
                &payee,
                amount,
                ExistenceRequirement::KeepAlive,
            )?;

            let key = (subscriber_cid_number.clone(), issuer.clone());
            Self::unschedule_renewal(&key);
            let started_at = now;
            Subscriptions::<T>::insert(
                &key,
                SubscriptionState {
                    plan: plan.clone(),
                    started_at,
                    last_charged_at: now,
                    last_charged_price_fen: price_fen,
                    paid_until,
                    subscription_status: SubscriptionStatus::Active,
                    authorized_price_fen: price_fen,
                    suspend_reason: None,
                },
            );
            Self::schedule_renewal(&key, paid_until);
            Self::deposit_event(Event::SubscriptionCharged {
                subscriber_cid_number,
                payer_account_id: subscriber_account_id,
                issuer,
                plan,
                price_fen,
                charged_at: now,
                paid_until,
            });
            Ok(())
        })
    }

    /// 按时间戳有序处理到期任务。链曾停止出块时，逾期周期按顺序继续处理直到追上当前时间。
    pub(crate) fn process_due_subscriptions(now: u64, limit: u32) -> u32 {
        let mut processed = 0u32;
        while processed < limit {
            let Some((due_key, subscription_key)) = RenewalSchedule::<T>::iter_keys().next() else {
                break;
            };
            let due_at = u64::from_be_bytes(due_key);
            if due_at > now {
                break;
            }
            RenewalSchedule::<T>::remove(due_key, &subscription_key);
            RenewalIndex::<T>::remove(&subscription_key);
            Self::process_one_due(subscription_key, due_at, now);
            processed = processed.saturating_add(1);
        }
        processed
    }

    fn process_one_due(key: SubKeyOf<T>, due_at: u64, now: u64) {
        let Some(mut state) = Subscriptions::<T>::get(&key) else {
            return;
        };
        // 只有留在调度里的 Active / IssuerPaused 才处理；双向一致性由 try_state 守护。
        if !matches!(
            state.subscription_status,
            SubscriptionStatus::Active | SubscriptionStatus::IssuerPaused
        ) {
            return;
        }
        let (subscriber_cid_number, issuer) = key.clone();
        let plan = state.plan.clone();
        let subscriber_account_id = match Self::current_account_id_for_cid(&subscriber_cid_number) {
            Ok(account_id) => account_id,
            Err(_) => {
                Self::suspend_subscription(
                    &key,
                    state,
                    subscriber_cid_number,
                    issuer,
                    SuspendReason::IdentityBindingUnavailable,
                    now,
                );
                return;
            }
        };
        let (price_fen, payee) = match Self::current_price_and_payee(&issuer, &plan, now) {
            Ok(value) => value,
            // 创作者删了该档/周期 → 挂起待再签名，保留粉丝关系。
            Err(e) if e == Error::<T>::CreatorPlanNotFound.into() => {
                Self::suspend_subscription(
                    &key,
                    state,
                    subscriber_cid_number,
                    issuer,
                    SuspendReason::NeedReconsent,
                    now,
                );
                return;
            }
            // 签发方暂时不具备收款条件 → 暂停扣费，保留调度下周期重试，恢复即自动续。
            Err(e)
                if e == Error::<T>::CreatorNotPlatformMember.into()
                    || e == Error::<T>::PlatformPriceNotSet.into()
                    || e == Error::<T>::PlatformNotBound.into() =>
            {
                state.subscription_status = SubscriptionStatus::IssuerPaused;
                state.suspend_reason = None;
                Subscriptions::<T>::insert(&key, state);
                // 暂停期不推进 paid_until（不给权益）；调度重排到下周期重试。
                if let Some(retry_at) = add_calendar_period(due_at, plan.billing_period()) {
                    Self::schedule_renewal(&key, retry_at);
                }
                Self::deposit_event(Event::SubscriptionIssuerPaused {
                    subscriber_cid_number,
                    issuer,
                    paused_at: now,
                });
                return;
            }
            // 创作者 CID 当前没有完整 active 绑定，禁止向历史账户付款。
            Err(e) if e == Error::<T>::CitizenIdentityUnavailable.into() => {
                Self::suspend_subscription(
                    &key,
                    state,
                    subscriber_cid_number,
                    issuer,
                    SuspendReason::IdentityBindingUnavailable,
                    now,
                );
                return;
            }
            // 定价或计划归属配置有误，需人工修正后由订阅者再签名恢复。
            Err(e)
                if e == Error::<T>::ZeroPrice.into()
                    || e == Error::<T>::PlanIssuerMismatch.into() =>
            {
                Self::suspend_subscription(
                    &key,
                    state,
                    subscriber_cid_number,
                    issuer,
                    SuspendReason::NeedReconsent,
                    now,
                );
                return;
            }
            // 兜底一律挂起，不得终止；只有公历溢出（下方独立分支）才终止。
            Err(_) => {
                Self::suspend_subscription(
                    &key,
                    state,
                    subscriber_cid_number,
                    issuer,
                    SuspendReason::NeedReconsent,
                    now,
                );
                return;
            }
        };
        // 平台价格只由投票引擎通过后的治理结果更新；平台订阅是持续授权，按最新治理价
        // 自动续费。创作者价格属于个人自主决定，不继承平台治理授权，改价后必须挂起并
        // 等订阅者按新价重新签名。两类价格不得共用同一再授权规则。
        if matches!(issuer, IssuerKey::Creator(_)) && price_fen != state.authorized_price_fen {
            Self::suspend_subscription(
                &key,
                state,
                subscriber_cid_number,
                issuer,
                SuspendReason::NeedReconsent,
                now,
            );
            return;
        }
        let Some(paid_until) = add_calendar_period(due_at, plan.billing_period()) else {
            state.subscription_status = SubscriptionStatus::Terminated;
            state.suspend_reason = None;
            Subscriptions::<T>::insert(&key, state);
            Self::deposit_event(Event::SubscriptionRenewalStopped {
                subscriber_cid_number,
                issuer,
                stopped_at: now,
            });
            return;
        };
        let amount: BalanceOf<T> = price_fen.saturated_into();
        if T::Currency::transfer(
            &subscriber_account_id,
            &payee,
            amount,
            ExistenceRequirement::KeepAlive,
        )
        .is_err()
        {
            // 余额不足 → 挂起待充值再签名，不终止、不重排调度。
            Self::suspend_subscription(
                &key,
                state,
                subscriber_cid_number,
                issuer,
                SuspendReason::InsufficientBalance,
                now,
            );
            return;
        }

        state.plan = plan.clone();
        state.last_charged_at = now;
        state.last_charged_price_fen = price_fen;
        state.paid_until = paid_until;
        state.subscription_status = SubscriptionStatus::Active;
        state.authorized_price_fen = price_fen;
        state.suspend_reason = None;
        Subscriptions::<T>::insert(&key, state);
        Self::schedule_renewal(&key, paid_until);
        Self::deposit_event(Event::SubscriptionCharged {
            subscriber_cid_number,
            payer_account_id: subscriber_account_id,
            issuer,
            plan,
            price_fen,
            charged_at: now,
            paid_until,
        });
    }

    pub(crate) fn schedule_renewal(key: &SubKeyOf<T>, due_at: u64) {
        Self::unschedule_renewal(key);
        RenewalSchedule::<T>::insert(due_at.to_be_bytes(), key, ());
        RenewalIndex::<T>::insert(key, due_at);
    }

    pub(crate) fn unschedule_renewal(key: &SubKeyOf<T>) {
        if let Some(previous) = RenewalIndex::<T>::take(key) {
            RenewalSchedule::<T>::remove(previous.to_be_bytes(), key);
        }
    }

    /// 挂起订阅：保留粉丝关系、写挂起原因、退出续费调度（调用方保证已离调度），等用户再签名/充值恢复。
    fn suspend_subscription(
        key: &SubKeyOf<T>,
        mut state: SubscriptionState,
        subscriber_cid_number: CidNumberOf<T>,
        issuer: IssuerKeyOf<T>,
        reason: SuspendReason,
        now: u64,
    ) {
        state.subscription_status = SubscriptionStatus::Suspended;
        state.suspend_reason = Some(reason);
        Subscriptions::<T>::insert(key, state);
        Self::deposit_event(Event::SubscriptionSuspended {
            subscriber_cid_number,
            issuer,
            reason,
            suspended_at: now,
        });
    }

    /// 签名取消是撤销自动扣款授权的唯一方式；当前已付款权益不缩短。
    pub(crate) fn do_cancel(
        subscriber_account_id: T::AccountId,
        issuer: IssuerKeyOf<T>,
    ) -> DispatchResult {
        let subscriber_cid_number = Self::active_cid_number_for_account(&subscriber_account_id)?;
        let key = (subscriber_cid_number.clone(), issuer.clone());
        let paid_until = Subscriptions::<T>::try_mutate(&key, |slot| {
            let state = slot.as_mut().ok_or(Error::<T>::SubscriptionNotFound)?;
            state.subscription_status = SubscriptionStatus::Cancelled;
            state.suspend_reason = None;
            Ok::<_, Error<T>>(state.paid_until)
        })?;
        Self::unschedule_renewal(&key);
        Self::deposit_event(Event::SubscriptionCancelled {
            subscriber_cid_number,
            issuer,
            paid_until,
        });
        Ok(())
    }

    /// 换挡立即生效并折算：升档补扣「新价 − 剩余权益折算」，降档不扣、余额折算成延长时长。
    pub(crate) fn do_change_subscription_plan(
        subscriber_account_id: T::AccountId,
        issuer: IssuerKeyOf<T>,
        new_plan: SubscriptionPlan,
        expected_price_fen: u128,
    ) -> DispatchResult {
        with_storage_layer(|| -> DispatchResult {
            let subscriber_cid_number =
                Self::active_cid_number_for_account(&subscriber_account_id)?;
            let key = (subscriber_cid_number.clone(), issuer.clone());
            let mut state =
                Subscriptions::<T>::get(&key).ok_or(Error::<T>::SubscriptionNotFound)?;
            let now = Self::now_ms();
            let (new_price, payee) = Self::current_price_and_payee(&issuer, &new_plan, now)?;
            ensure!(
                new_price == expected_price_fen,
                Error::<T>::SignedPriceChanged
            );

            // 仅当仍在有效已付周期内（Active/Cancelled 且未到期）才折算剩余权益。
            let credit = if now < state.paid_until
                && matches!(
                    state.subscription_status,
                    SubscriptionStatus::Active | SubscriptionStatus::Cancelled
                ) {
                Self::remaining_credit(
                    state.last_charged_price_fen,
                    state.last_charged_at,
                    state.paid_until,
                    now,
                )
            } else {
                0
            };

            let base_end = add_calendar_period(now, new_plan.billing_period())
                .ok_or(Error::<T>::CalendarOverflow)?;

            let (charged_now, paid_until) = if new_price > credit {
                // 升档：立即补扣差额，新周期从现在起算。
                let charge = new_price.saturating_sub(credit);
                let amount: BalanceOf<T> = charge.saturated_into();
                T::Currency::transfer(
                    &subscriber_account_id,
                    &payee,
                    amount,
                    ExistenceRequirement::KeepAlive,
                )?;
                (charge, base_end)
            } else {
                // 降档：不扣款，剩余信用按新档单价折算成额外时长叠加（new_price > 0 已由定价保证）。
                let period_ms = u128::from(base_end.saturating_sub(now));
                let extra_ms =
                    credit.saturating_sub(new_price).saturating_mul(period_ms) / new_price;
                (
                    0u128,
                    base_end.saturating_add(extra_ms.saturated_into::<u64>()),
                )
            };

            state.plan = new_plan.clone();
            state.last_charged_at = now;
            state.last_charged_price_fen = new_price;
            state.authorized_price_fen = new_price;
            state.paid_until = paid_until;
            state.subscription_status = SubscriptionStatus::Active;
            state.suspend_reason = None;
            Subscriptions::<T>::insert(&key, state);
            Self::schedule_renewal(&key, paid_until);
            Self::deposit_event(Event::SubscriptionPlanChanged {
                subscriber_cid_number,
                payer_account_id: subscriber_account_id,
                issuer,
                new_plan,
                charged_now,
                paid_until,
            });
            Ok(())
        })
    }

    /// 剩余权益折算：本期实收价 × 剩余时长 ÷ 本期总时长（按毫秒，向下取整）。
    ///
    /// 基准价只接受 `last_charged_price_fen`；`authorized_price_fen` 是已同意的下期价，
    /// 只用于判断是否需要再签名，不参与金额计算。
    fn remaining_credit(
        last_charged_price_fen: u128,
        last_charged_at: u64,
        paid_until: u64,
        now: u64,
    ) -> u128 {
        if now >= paid_until || paid_until <= last_charged_at {
            return 0;
        }
        let remaining = u128::from(paid_until - now);
        let total = u128::from(paid_until - last_charged_at);
        last_charged_price_fen.saturating_mul(remaining) / total
    }

    /// 创作者覆盖式写入自己的链上付款套餐与档位名称；两类 storage 同一交易原子收敛。
    pub(crate) fn do_set_creator_plans(
        creator_account_id: T::AccountId,
        tiers: Vec<CreatorTierInput>,
    ) -> DispatchResult {
        let creator_cid_number = Self::active_cid_number_for_account(&creator_account_id)?;
        ensure!(
            Self::has_effective_platform_subscription(&creator_cid_number, Self::now_ms()),
            Error::<T>::CreatorNotPlatformMember
        );
        Self::validate_creator_tiers(&tiers)?;
        let previous = CreatorPlans::<T>::get(&creator_cid_number);
        for tier in previous {
            CreatorTierNames::<T>::remove(&creator_cid_number, tier.tier_id);
        }
        let mut price_tiers = Vec::with_capacity(tiers.len());
        for tier in tiers {
            CreatorTierNames::<T>::insert(&creator_cid_number, &tier.tier_id, tier.tier_name);
            price_tiers.push(CreatorTier {
                tier_id: tier.tier_id,
                prices_fen: tier.prices_fen,
            });
        }
        let bounded =
            CreatorTiers::try_from(price_tiers).map_err(|_| Error::<T>::TooManyCreatorTiers)?;
        CreatorPlans::<T>::insert(&creator_cid_number, bounded.clone());
        Self::deposit_event(Event::CreatorPlansSet {
            creator_cid_number,
            signer_account_id: creator_account_id,
            tier_count: bounded.len() as u32,
        });
        Ok(())
    }

    /// 只修改名称。当前 CID 控制账户即可执行，不要求平台会员仍有效，避免名称修正被权益状态阻断。
    pub(crate) fn do_update_creator_tier_name(
        signer_account_id: T::AccountId,
        tier_id: TierId,
        tier_name: TierName,
    ) -> DispatchResult {
        let creator_cid_number = Self::active_cid_number_for_account(&signer_account_id)?;
        ensure!(
            CreatorPlans::<T>::get(&creator_cid_number)
                .iter()
                .any(|tier| tier.tier_id == tier_id),
            Error::<T>::CreatorPlanNotFound
        );
        Self::validate_tier_name(&tier_name)?;
        CreatorTierNames::<T>::insert(&creator_cid_number, &tier_id, tier_name);
        Self::deposit_event(Event::CreatorTierNameUpdated {
            creator_cid_number,
            signer_account_id,
            tier_id,
        });
        Ok(())
    }
}
