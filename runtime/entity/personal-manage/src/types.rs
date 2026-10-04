//! 个人多签类型定义。
//!
//! 第一类是"账户基本信息"——`PersonalStatus` / `PersonalAccount`,
//! 由 `PersonalAccounts` storage map 引用,只描述个人多签的创建者、
//! 账户名、创建区块和账户状态。管理员列表的唯一真源是 `personal-admins`，
//! 普通动态阈值的唯一真源是 `internal-vote`。
//!
//! 第二类是"提案业务数据"——`PersonalCreateAction` / `PersonalCloseAction`,
//! 在投票引擎 `ProposalData` 里 SCALE 编码存放,投票通过后由
//! `InternalVoteExecutor` 解码后执行业务。

use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::pallet_prelude::*;
use scale_info::TypeInfo;
use sp_runtime::Debug;

/// 多签账户状态
#[derive(
    Encode,
    Decode,
    DecodeWithMemTracking,
    Clone,
    Copy,
    Debug,
    TypeInfo,
    MaxEncodedLen,
    PartialEq,
    Eq,
)]
pub enum PersonalStatus {
    /// 提案投票中,尚未激活
    Pending,
    /// 已激活(投票通过并入金完成)
    Active,
}

#[derive(
    Encode, Decode, DecodeWithMemTracking, Clone, Debug, TypeInfo, MaxEncodedLen, PartialEq, Eq,
)]
pub struct PersonalAccount<AccountId, AccountName, BlockNumber> {
    pub creator_account_id: AccountId,
    pub account_name: AccountName,
    pub created_at: BlockNumber,
    pub status: PersonalStatus,
}

/// 创建多签账户提案的业务数据(存入投票引擎 ProposalData)。
///
/// `fee` 是提案创建当下的手续费快照,执行或清理时必须读取该字段,
/// 不能用当前 runtime 的 fee 公式重新计算。
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub struct PersonalCreateAction<AccountId, Balance> {
    pub account_id: AccountId,
    pub proposer_account_id: AccountId,
    pub amount: Balance,
    pub fee: Balance,
}

/// 关闭多签账户提案的业务数据
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub struct PersonalCloseAction<AccountId> {
    pub account_id: AccountId,
    pub beneficiary_account_id: AccountId,
    pub proposer_account_id: AccountId,
}
