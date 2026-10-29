//! ACTION 常量 + 提案体定义。
//!
//! 与 ADR-011 第十节绑定。所有业务和监管动作走 VotingEngine ProposalData，
//! 业务标签前缀 `MODULE_TAG = b"onc-iss"`(见 `lib.rs`),后接 4B ACTION。
//!
//! ## propose origin 校验铁律（ADR-011 第 5.4 / 5.6 节）
//!
//! - **业务 5 ACTION**(OAIS/OAMT/OABN/OACL/OATR):propose 入口校验
//!   `actor_cid_number + actor_role_code + proposer_account_id` 的完整岗位权限
//! - **监管 5 ACTION**(OMFZ/OMUF/OMCF/OMFT/OMFC):propose 入口固定校验
//!   `NRC + COMMITTEE_MEMBER + proposer_account_id` 的完整岗位权限
//!
//! VotingEngine 自身在 cast 阶段校验冻结岗位主体，propose 阶段仍须前置校验完整岗位权限，
//! 防止任意账户消耗 storage 提案位或占用投票引擎额度。
//!
//! ## metadata 永久不可改铁律（ADR-011 第 5.7 节）
//!
//! 第一期不提供 set_metadata ACTION。发行后 name / symbol / description 永久锁定,
//! 如需改名只能 close 重发。

use crate::types::AssetClass;
use codec::{Decode, Encode};
use frame_support::pallet_prelude::*;
use scale_info::TypeInfo;
use sp_runtime::Debug;
use sp_std::vec::Vec;
// 业务 ACTION(走 InternalVote，机构岗位或个人多签主体内部执行)
pub const ACTION_ONCHAIN_ASSET_ISSUE: [u8; 4] = *b"OAIS";
pub const ACTION_ONCHAIN_ASSET_MINT: [u8; 4] = *b"OAMT";
pub const ACTION_ONCHAIN_ASSET_BURN: [u8; 4] = *b"OABN";
pub const ACTION_ONCHAIN_ASSET_CLOSE: [u8; 4] = *b"OACL";
pub const ACTION_ONCHAIN_ASSET_TRANSFER: [u8; 4] = *b"OATR";
// 监管 ACTION(走 JointVote,NRC 治理账户 + 全民兜底)
pub const ACTION_ONCHAIN_ASSET_MONITOR_FREEZE: [u8; 4] = *b"OMFZ";
pub const ACTION_ONCHAIN_ASSET_MONITOR_UNFREEZE: [u8; 4] = *b"OMUF";
pub const ACTION_ONCHAIN_ASSET_MONITOR_CONFISCATE: [u8; 4] = *b"OMCF";
pub const ACTION_ONCHAIN_ASSET_MONITOR_FORCE_TRANSFER: [u8; 4] = *b"OMFT";
pub const ACTION_ONCHAIN_ASSET_MONITOR_FORCE_CLOSE: [u8; 4] = *b"OMFC";
// 提案体(SCALE 编解码,VotingEngine 透明承载)
/// 创建资产提案体。
///
#[derive(Encode, Decode, DecodeWithMemTracking, Clone, Debug, TypeInfo)]
pub struct IssueProposal<AccountId, Balance> {
    /// 发行机构唯一身份。
    pub actor_cid_number: Vec<u8>,
    /// 资产执行账户；只承载资产，不作为机构身份或管理员根。
    pub execution_account_id: AccountId,
    /// 资产种类(第一期 Plain only)。
    pub class: AssetClass,
    /// 名称(过黑名单)。bound 由 runtime 配置 MaxAssetNameLen。
    pub name: Vec<u8>,
    /// 符号(过黑名单)。bound 由 runtime 配置 MaxAssetSymbolLen。
    pub symbol: Vec<u8>,
    /// 描述(过黑名单)。bound 由 runtime 配置 MaxAssetDescriptionLen。
    pub description: Vec<u8>,
    /// 小数位(0..=18)。
    pub decimals: u8,
    /// 初始发行量(链上记账整数,即 raw amount 含 decimals)。
    pub initial_supply: Balance,
}

/// 增发提案体。
#[derive(Encode, Decode, DecodeWithMemTracking, Clone, Debug, TypeInfo)]
pub struct MintProposal<AccountId, Balance> {
    pub actor_cid_number: Vec<u8>,
    pub asset_id: u32,
    pub to_account_id: AccountId,
    pub amount: Balance,
}

/// 销毁提案体。
#[derive(Encode, Decode, DecodeWithMemTracking, Clone, Debug, TypeInfo)]
pub struct BurnProposal<AccountId, Balance> {
    pub actor_cid_number: Vec<u8>,
    pub asset_id: u32,
    pub from_account_id: AccountId,
    pub amount: Balance,
}

/// 关闭资产提案体(发行方主动)。
#[derive(Encode, Decode, DecodeWithMemTracking, Clone, Debug, TypeInfo)]
pub struct CloseProposal {
    pub actor_cid_number: Vec<u8>,
    pub asset_id: u32,
}

/// 转账提案体。
#[derive(Encode, Decode, DecodeWithMemTracking, Clone, Debug, TypeInfo)]
pub struct TransferProposal<AccountId, Balance> {
    pub actor_cid_number: Vec<u8>,
    pub asset_id: u32,
    pub from_account_id: AccountId,
    pub to_account_id: AccountId,
    pub amount: Balance,
}
// 监管提案体(NRC 调用,JointVote)
/// 监管:冻结 / 解冻持仓。
#[derive(Encode, Decode, DecodeWithMemTracking, Clone, Debug, TypeInfo)]
pub struct MonitorFreezeProposal<AccountId> {
    pub actor_cid_number: Vec<u8>,
    pub asset_id: u32,
    pub account_id: AccountId,
    pub reason_hash: [u8; 32],
}

/// 监管:强制 burn(扣押)。
#[derive(Encode, Decode, DecodeWithMemTracking, Clone, Debug, TypeInfo)]
pub struct MonitorConfiscateProposal<AccountId, Balance> {
    pub actor_cid_number: Vec<u8>,
    pub asset_id: u32,
    pub account_id: AccountId,
    pub amount: Balance,
    pub reason_hash: [u8; 32],
}

/// 监管:强制划转。
#[derive(Encode, Decode, DecodeWithMemTracking, Clone, Debug, TypeInfo)]
pub struct MonitorForceTransferProposal<AccountId, Balance> {
    pub actor_cid_number: Vec<u8>,
    pub asset_id: u32,
    pub from_account_id: AccountId,
    pub to_account_id: AccountId,
    pub amount: Balance,
    pub reason_hash: [u8; 32],
}

/// 监管:整币封禁(30 天后销毁)。
#[derive(Encode, Decode, DecodeWithMemTracking, Clone, Debug, TypeInfo)]
pub struct MonitorForceCloseProposal {
    pub actor_cid_number: Vec<u8>,
    pub asset_id: u32,
    pub reason_hash: [u8; 32],
}
