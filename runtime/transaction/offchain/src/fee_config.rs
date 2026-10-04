//! 清算行费率自治。
//!
//!
//! - 每个清算行主账户对应一个 `L2FeeRateBp` 费率(单位 bp,范围 1~10)。
//! - 清算行有权岗位任职人可提案改费率,**延迟 7 天生效**(防突袭改价,给 L3 换行时间)。
//! - 全局上限 `MaxL2FeeRateBp` 由联合投票调整(沿用 `votingengine`,
//!   这里只定义 extrinsic 入口;具体联合投票执行回调另行对接)。
//! - 在 `on_initialize` 每块扫描一次到期提案并激活(小成本,可优化为 cursor)。

use frame_support::{ensure, pallet_prelude::*};
use frame_system::pallet_prelude::*;
use sp_runtime::traits::{SaturatedConversion, Saturating};
use sp_std::vec::Vec;

use crate::bank_check::{self, CidAccountQuery};
use crate::{Config, Error, Event, L2FeeRateBp, L2FeeRateProposed, MaxL2FeeRateBp, Pallet};

/// Perbill 单位换算:1 bp = 0.01% = `Perbill::from_parts(100_000)`,
/// 因此 `bp = Perbill.deconstruct() / 100_000`。
const PERBILL_PARTS_PER_BP: u32 = 100_000;

/// 费率下限(bp),由 `primitives::fee_policy::OFFCHAIN_FEE_RATE_MIN`(0.01%)推导。
pub const L2_FEE_RATE_BP_MIN: u32 =
    primitives::fee_policy::OFFCHAIN_FEE_RATE_MIN.deconstruct() / PERBILL_PARTS_PER_BP;

/// 费率上限(bp),由 `primitives::fee_policy::OFFCHAIN_FEE_RATE_MAX`(0.1%)推导,对应白皮书 5.4.2。
pub const L2_FEE_RATE_BP_MAX: u32 =
    primitives::fee_policy::OFFCHAIN_FEE_RATE_MAX.deconstruct() / PERBILL_PARTS_PER_BP;

/// 费率变更延迟生效期：按 PoW 固定平均六分钟口径换算为七天。
/// 该值是制度上的固定区块数，不承诺七个自然日内必然产生足够区块。
pub const RATE_CHANGE_DELAY_BLOCKS: u64 = 7 * primitives::pow_const::BLOCKS_PER_DAY;

/// 提案新费率:清算行管理员发起,延迟 `RATE_CHANGE_DELAY_BLOCKS` 后生效。
///
/// 约束:
/// - `who` 必须以清算行 CID + 岗位码匹配有效任职和本动作权限
/// - 目标必须是合法清算行主账户(K1=S/F + Active)
/// - 新费率在 `[L2_FEE_RATE_BP_MIN, MaxL2FeeRateBp]` 区间
/// - 同一清算行不允许并行提案(新提案覆盖旧提案)
pub fn do_propose_l2_fee_rate<T: Config>(
    who: T::AccountId,
    actor_cid_number: &crate::InstitutionCidNumber,
    actor_role_code: &[u8],
    institution_account_id: T::AccountId,
    new_rate_bp: u32,
) -> DispatchResult {
    // 1. CID 与本次操作的清算行主账户必须严格对应。
    bank_check::ensure_institution_account::<T>(
        actor_cid_number.as_slice(),
        &institution_account_id,
        bank_check::ACCOUNT_NAME_MAIN,
    )?;
    bank_check::ensure_can_be_bound::<T>(actor_cid_number.as_slice())?;

    // 2. 授权唯一真源是 CID、岗位码、有效任职账户和业务动作权限。
    ensure!(
        T::CidAccountQuery::is_institution_role_authorized(
            actor_cid_number.as_slice(),
            actor_role_code,
            &who,
            entity_primitives::business_action::ACTION_OFFCHAIN_PROPOSE_FEE_RATE,
        ),
        Error::<T>::UnauthorizedAdmin
    );

    // 3. 费率范围校验
    let max = MaxL2FeeRateBp::<T>::get().max(L2_FEE_RATE_BP_MAX);
    ensure!(
        new_rate_bp >= L2_FEE_RATE_BP_MIN && new_rate_bp <= max,
        Error::<T>::InvalidL2FeeRate
    );

    // 4. 延迟生效高度 = 当前高度 + RATE_CHANGE_DELAY_BLOCKS
    let now = frame_system::Pallet::<T>::block_number();
    let delay: BlockNumberFor<T> = RATE_CHANGE_DELAY_BLOCKS.saturated_into();
    let effective_at = now.saturating_add(delay);

    L2FeeRateProposed::<T>::insert(actor_cid_number, (new_rate_bp, effective_at));
    Pallet::<T>::deposit_event(Event::<T>::L2FeeRateProposed {
        bank_cid: actor_cid_number.clone(),
        new_rate_bp,
        effective_at,
    });
    Ok(())
}

/// `on_initialize` 钩子激活到期提案:把 `L2FeeRateProposed` 里 `effective_at <= now`
/// 的提案搬到 `L2FeeRateBp`,然后删除提案记录。
///
/// 本步采用简单全表扫描(小规模清算行可接受,可优化为 cursor/batched)。
/// 返回消耗的权重,供上层 on_initialize 累加。
/// 单块最多激活的到期费率提案数。参照 votingengine 的 per-block 预算模式:
/// 不做无界全表扫描,收集到上限即停,剩余到期项顺延下一块继续排空(移除即缩表,最终收敛)。
const MAX_L2_FEE_ACTIVATIONS_PER_BLOCK: usize = 32;

pub fn activate_pending_rates<T: Config>(now: BlockNumberFor<T>) -> Weight {
    let db = T::DbWeight::get();
    let mut consumed = Weight::zero();
    let mut to_activate: Vec<(crate::InstitutionCidNumber, u32)> = Vec::new();

    for (bank, (rate, effective_at)) in L2FeeRateProposed::<T>::iter() {
        consumed = consumed.saturating_add(db.reads(1));
        if now >= effective_at {
            to_activate.push((bank, rate));
            // 收满本块预算即停止扫描,其余到期项下一块继续(每块工作量有上界)。
            if to_activate.len() >= MAX_L2_FEE_ACTIVATIONS_PER_BLOCK {
                break;
            }
        }
    }

    for (bank, rate) in to_activate {
        L2FeeRateBp::<T>::insert(&bank, rate);
        L2FeeRateProposed::<T>::remove(&bank);
        Pallet::<T>::deposit_event(Event::<T>::L2FeeRateActivated {
            bank_cid: bank,
            rate_bp: rate,
        });
        consumed = consumed.saturating_add(db.reads_writes(0, 3));
    }

    consumed
}

/// 全局上限治理:由联合投票引擎执行回调。
///
/// 本函数做校验,写入 `MaxL2FeeRateBp`,联合投票回调后改为
/// 只由投票引擎回调可达(把 extrinsic 改为免费的 `execute_*` 类)。
pub fn do_set_max_l2_fee_rate<T: Config>(new_max: u32) -> DispatchResult {
    // 最大费率必须完整落在协议规定的闭区间内。
    ensure!(
        (L2_FEE_RATE_BP_MIN..=L2_FEE_RATE_BP_MAX).contains(&new_max),
        Error::<T>::InvalidL2FeeRate
    );
    MaxL2FeeRateBp::<T>::put(new_max);
    Pallet::<T>::deposit_event(Event::<T>::MaxL2FeeRateUpdated { new_max });
    Ok(())
}

/// 查询清算行当前生效费率。未配置时返回 0(调用方自己决定是否用全局默认)。
pub fn current_rate_bp<T: Config>(bank_cid: &crate::InstitutionCidNumber) -> u32 {
    L2FeeRateBp::<T>::get(bank_cid)
}

#[cfg(test)]
mod delay_tests {
    use super::*;

    #[test]
    fn seven_day_delay_uses_fixed_six_minute_block_calendar() {
        assert_eq!(primitives::pow_const::BLOCKS_PER_DAY, 240);
        assert_eq!(RATE_CHANGE_DELAY_BLOCKS, 1_680);
    }
}
