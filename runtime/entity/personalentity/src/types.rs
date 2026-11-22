//! 定义个人多签账户状态、基本资料和提案动作数据。

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
