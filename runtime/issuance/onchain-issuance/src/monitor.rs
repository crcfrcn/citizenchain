//! NRC 监管动作(freeze / unfreeze / confiscate / forceTransfer / forceClose)— 框架占位。
//!
//! 与 ADR-011 第 5.1 / 5.6 节对齐：
//! - 监管动作走 **JointVote**(NRC admin 多签 + 全民兜底)
//! - propose origin 校验:`actor_cid_number == NRC` 且
//!   `ensure!(proposer_account_id ∈ AdminAccounts[actor_cid_number].admins)`
//! - 强制销毁倒计时 30 天:写入 `ForceCloseSchedule[expire_block].push(asset_id)`,
//!   `on_finalize(n)` 通过 `take(n)` O(1) 处理,不全表扫描 Assets
//!
//! 当前框架阶段只搭函数签名 + doc 占位,实装在后续任务卡 B 完成。
//!
//! **未实装期一律 fail-closed**:本模块所有监管执行入口返回 `Error::NotImplemented`,
//! 不返回 `Ok(())`。唯一例外是 `process_force_close_schedule_on_finalize`
//! (在 `on_finalize` 中执行,不得 panic 也无 Result 可返回),其安全性由
//! `execute_monitor_force_close` 的 fail-closed 上游保证。

use crate::pallet::{BalanceOf, Config, Error};
use crate::proposal::{
    MonitorConfiscateProposal, MonitorForceCloseProposal, MonitorForceTransferProposal,
    MonitorFreezeProposal,
};
use frame_support::pallet_prelude::*;
use frame_system::pallet_prelude::BlockNumberFor;

/// NRC 监管:冻结特定持仓(调 pallet_assets::freeze + emit MonitorFrozen)。
pub fn execute_monitor_freeze<T: Config>(
    _proposal: MonitorFreezeProposal<T::AccountId>,
) -> DispatchResult {
    // 业务未实装(ADR-011 任务卡 B)。返回 Err 而非 Ok:执行入口必须 fail-closed,
    // 否则将来先接回调后写业务时,投票会"通过且无任何链上副作用"形成假成功。
    Err(Error::<T>::NotImplemented.into())
}

/// NRC 监管:解冻持仓(调 pallet_assets::thaw + emit MonitorUnfrozen)。
pub fn execute_monitor_unfreeze<T: Config>(
    _proposal: MonitorFreezeProposal<T::AccountId>,
) -> DispatchResult {
    // 业务未实装(ADR-011 任务卡 B)。返回 Err 而非 Ok:执行入口必须 fail-closed,
    // 否则将来先接回调后写业务时,投票会"通过且无任何链上副作用"形成假成功。
    Err(Error::<T>::NotImplemented.into())
}

/// NRC 监管:强制 burn(扣押,调 pallet_assets::burn_from + emit MonitorConfiscated)。
pub fn execute_monitor_confiscate<T: Config>(
    _proposal: MonitorConfiscateProposal<T::AccountId, BalanceOf<T>>,
) -> DispatchResult {
    // 业务未实装(ADR-011 任务卡 B)。返回 Err 而非 Ok:执行入口必须 fail-closed,
    // 否则将来先接回调后写业务时,投票会"通过且无任何链上副作用"形成假成功。
    Err(Error::<T>::NotImplemented.into())
}

/// NRC 监管:强制划转(追赃,调 pallet_assets::transfer 跳过 from_account_id 同意)。
pub fn execute_monitor_force_transfer<T: Config>(
    _proposal: MonitorForceTransferProposal<T::AccountId, BalanceOf<T>>,
) -> DispatchResult {
    // 业务未实装(ADR-011 任务卡 B)。返回 Err 而非 Ok:执行入口必须 fail-closed,
    // 否则将来先接回调后写业务时,投票会"通过且无任何链上副作用"形成假成功。
    Err(Error::<T>::NotImplemented.into())
}

/// NRC 监管:整币封禁入调度队列(30 天后由 on_finalize 销毁余额)。
///
/// 实装时 `expire_block = current_block + 30 * DAYS`,
/// `ForceCloseSchedule::mutate(expire_block, |list| list.try_push(asset_id))`,
/// `Assets[asset_id].state = ForceClosed { close_block: expire_block }`(同事务)。
/// 30 天后 `on_finalize(expire_block)` 取出 list 逐一执行 `pallet_assets::start_destroy`。
pub fn execute_monitor_force_close<T: Config>(
    _proposal: MonitorForceCloseProposal,
) -> DispatchResult {
    // 业务未实装(ADR-011 任务卡 B)。返回 Err 而非 Ok:执行入口必须 fail-closed,
    // 否则将来先接回调后写业务时,投票会"通过且无任何链上副作用"形成假成功。
    Err(Error::<T>::NotImplemented.into())
}

/// `on_finalize(n)` 处理到期 ForceClose 队列。
///
/// O(1) `take(n)` 取出当前块到期的 asset_id 列表 → 逐一 destroy。
/// 不扫主 Assets 表。
///
/// 本函数在 `on_finalize` 中执行,返回 `()` 且不得 panic(否则整链停止出块),
/// 故不能像其它执行入口那样 fail-closed。安全性由上游保证:
/// `execute_monitor_force_close` 未实装期间一律返回 `NotImplemented`,
/// `ForceCloseSchedule` 永远为空,本函数实际是无副作用空转。
pub fn process_force_close_schedule_on_finalize<T: Config>(_block: BlockNumberFor<T>) {
    // 队列消费未实装(ADR-011 任务卡 B);实装后按下面注释展开。
    // let scheduled = ForceCloseSchedule::<T>::take(_block);
    // for asset_id in scheduled.iter() { pallet_assets::start_destroy(asset_id); ... }
}

/// 监管 callback 入口:VotingEngine JointVote 通过后路由到对应 execute_monitor_*。
///
/// propose origin 校验(proposer_account_id ∈ NRC admins)已在 propose 阶段完成,callback 不再校验。
pub fn dispatch_joint_callback<T: Config>(
    _action: [u8; 4],
    _proposal_data: &[u8],
) -> DispatchResult {
    // ACTION 路由未实装(ADR-011 任务卡 B):目标为 execute_monitor_freeze / unfreeze / ...。
    // 未实装期间一律 fail-closed,禁止静默返回成功。
    Err(Error::<T>::NotImplemented.into())
}
