//! 业务路径执行入口(issue/mint/burn/close/transfer)— 框架占位。
//!
//! 业务审批走多签内部执行:
//! 本 pallet **不暴露 wrapper extrinsic**,业务由 VotingEngine InternalVote 通过后,
//! 通过 callback 回调本模块入口函数,内部以 root 调用 `pallet_assets`。
//!
//! 规则要点:
//! - propose origin 校验：proposer_account_id ∈ admins(actor_cid_number)（防 spam）
//! - 费用必须进入全链五类协议，并由机构 CID 的费用账户承担
//! - `asset_id` 只表示资产编号，治理身份只来自 `actor_cid_number`
//! - OnchainAssetMeta 同时记录机构 CID 与资产执行账户，二者职责分离
//!
//! 当前框架阶段只搭函数签名 + doc 占位,实装在后续任务卡 A 完成。
//!
//! **未实装期一律 fail-closed**:本模块所有执行入口返回 `Error::NotImplemented`,
//! 不返回 `Ok(())`。外层虽已有 `RuntimeCallFilter` reject 与 `FeeRoute::Reject` 双闸,
//! 但那是外层防护;内层自己也必须拒绝,才能保证将来"先接回调、后写业务"时
//! 投票会明确失败,而不是静默通过。

use crate::pallet::{Config, Error, OnchainAssetId};
use crate::proposal::{BurnProposal, CloseProposal, IssueProposal, MintProposal, TransferProposal};
use frame_support::pallet_prelude::*;

use crate::pallet::BalanceOf;

/// 创建用户代币（写入 storage + 调 pallet_assets::create）。
///
/// 框架阶段占位,业务实装时步骤(callback 通过分支):
/// 1. `validation::ensure_institution_context` / `ensure_decimals_in_range` / `ensure_class_supported`
/// 2. 字段过黑名单(`validation::contains_blacklisted_word`)
/// 3. 按全链机构费用协议从 actor CID 费用账户执行收费，不得使用管理员账户押金
/// 4. 分配 AssetId(NextAssetId)
/// 5. 调 `T::Assets::create(asset_id, owner, ...)` + `mint_into` 注入 initial_supply
/// 6. 写 Assets storage,emit AssetIssued 事件
///
pub fn execute_issue<T: Config>(
    _proposal: IssueProposal<T::AccountId, BalanceOf<T>>,
) -> DispatchResult
where
    BalanceOf<T>: From<u128>,
{
    // 业务未实装(ADR-011 任务卡 A)。返回 Err 而非 Ok:执行入口必须 fail-closed,
    // 否则将来先接回调后写业务时,投票会"通过且无任何链上副作用"形成假成功。
    Err(Error::<T>::NotImplemented.into())
}

/// 增发(调 pallet_assets::mint_into + emit Minted)。
pub fn execute_mint<T: Config>(
    _proposal: MintProposal<T::AccountId, BalanceOf<T>>,
) -> DispatchResult {
    // 业务未实装(ADR-011 任务卡 A)。返回 Err 而非 Ok:执行入口必须 fail-closed,
    // 否则将来先接回调后写业务时,投票会"通过且无任何链上副作用"形成假成功。
    Err(Error::<T>::NotImplemented.into())
}

/// 销毁(调 pallet_assets::burn_from + emit Burned)。
pub fn execute_burn<T: Config>(
    _proposal: BurnProposal<T::AccountId, BalanceOf<T>>,
) -> DispatchResult {
    // 业务未实装(ADR-011 任务卡 A)。返回 Err 而非 Ok:执行入口必须 fail-closed,
    // 否则将来先接回调后写业务时,投票会"通过且无任何链上副作用"形成假成功。
    Err(Error::<T>::NotImplemented.into())
}

/// 转账(调 pallet_assets::transfer + emit Transferred)。
pub fn execute_transfer<T: Config>(
    _proposal: TransferProposal<T::AccountId, BalanceOf<T>>,
) -> DispatchResult {
    // 业务未实装(ADR-011 任务卡 A)。返回 Err 而非 Ok:执行入口必须 fail-closed,
    // 否则将来先接回调后写业务时,投票会"通过且无任何链上副作用"形成假成功。
    Err(Error::<T>::NotImplemented.into())
}

/// 关闭资产(调 pallet_assets::start_destroy + 销毁余额 + emit AssetClosed)。
///
/// ADR-011 第 8.1 节：必须由 with_transaction 包裹，保证 OnchainIssuance::Assets.state 与
/// pallet_assets::Asset.status 原子同步。
pub fn execute_close<T: Config>(_proposal: CloseProposal) -> DispatchResult {
    // 业务未实装(ADR-011 任务卡 A)。返回 Err 而非 Ok:执行入口必须 fail-closed,
    // 否则将来先接回调后写业务时,投票会"通过且无任何链上副作用"形成假成功。
    Err(Error::<T>::NotImplemented.into())
}

/// 业务 callback 入口:VotingEngine InternalVote 通过后路由到对应 execute_*。
///
/// 实装时按 proposal_data[0..7] = MODULE_TAG, [7..11] = ACTION 解码。
/// propose origin 校验在 propose 阶段(`validate_proposer_origin`)已完成,callback 此处不再校验。
pub fn dispatch_internal_callback<T: Config>(
    _action: [u8; 4],
    _proposal_data: &[u8],
) -> DispatchResult {
    // ACTION 路由未实装(ADR-011 任务卡 A):目标为 execute_issue / execute_mint / ...。
    // 未实装期间一律 fail-closed,禁止静默返回成功。
    Err(Error::<T>::NotImplemented.into())
}

/// AssetId 自增辅助(NextAssetId 单调递增,从 1 开始)。
pub fn allocate_asset_id<T: Config>() -> Result<OnchainAssetId, crate::pallet::Error<T>> {
    let next = crate::pallet::NextAssetId::<T>::get();
    let allocated = next;
    let new_next = next
        .checked_add(1)
        .ok_or(crate::pallet::Error::<T>::AssetIdOverflow)?;
    crate::pallet::NextAssetId::<T>::put(new_next);
    Ok(allocated)
}
