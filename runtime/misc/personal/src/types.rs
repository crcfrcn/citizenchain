//! 定义个人多签账户、成员状态、展示资料及原提案动作数据。

use codec::{Decode, Encode, MaxEncodedLen};
use engingcore::types::InstitutionCode;
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

/// 管理员姓、名各自的最大字节数；姓名只用于展示，不参与权限判断。
pub const ADMIN_PERSON_NAME_MAX_BYTES: u32 = 128;
/// 无法取得真实姓氏时使用的唯一默认姓。
pub const DEFAULT_ADMIN_FAMILY_NAME: &[u8] = "管理".as_bytes();
/// 无法取得真实名字时使用的唯一默认名。
pub const DEFAULT_ADMIN_GIVEN_NAME: &[u8] = "员".as_bytes();
/// 与链上中国公民字段同名的姓。
pub type FamilyName = BoundedVec<u8, ConstU32<ADMIN_PERSON_NAME_MAX_BYTES>>;
/// 与链上中国公民字段同名的名。
pub type GivenName = BoundedVec<u8, ConstU32<ADMIN_PERSON_NAME_MAX_BYTES>>;

/// 个人多签成员只以钱包账户授权；姓、名仅作备注，不参与账户派生。
#[derive(
    Encode, Decode, DecodeWithMemTracking, Clone, Debug, TypeInfo, MaxEncodedLen, PartialEq, Eq,
)]
pub struct Admin<AccountId> {
    pub account_id: AccountId,
    pub family_name: FamilyName,
    pub given_name: GivenName,
}

impl<AccountId> Admin<AccountId> {
    /// 将缺失的姓、名分别规范化为“管理”“员”；账户始终由调用方强制提供。
    pub fn normalize_names(mut self) -> Self {
        if self.family_name.is_empty() {
            self.family_name = FamilyName::truncate_from(DEFAULT_ADMIN_FAMILY_NAME.to_vec());
        }
        if self.given_name.is_empty() {
            self.given_name = GivenName::truncate_from(DEFAULT_ADMIN_GIVEN_NAME.to_vec());
        }
        self
    }
}

/// 个人多签分类保留原SCALE判别值，不把它重排为0。
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
pub enum AdminAccountKind {
    #[codec(index = 2)]
    PersonalMultisig,
}

/// 成员账户生命周期，Pending/Active/Closed的原编码保持不变。
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
pub enum AdminAccountStatus {
    /// 创建提案投票中；投票引擎可读取管理员快照。
    Pending,
    /// 已激活，可发起常规治理、转账或管理员更换。
    Active,
    /// 已关闭，管理员集合不再有效。
    Closed,
}

/// 个人多签成员账户记录，沿用原字段和字段顺序。
#[derive(
    Encode, Decode, DecodeWithMemTracking, Clone, Debug, TypeInfo, MaxEncodedLen, PartialEq, Eq,
)]
#[scale_info(skip_type_params(AdminList))]
pub struct AdminAccount<AdminList, AccountId, BlockNumber> {
    pub institution_code: InstitutionCode,
    pub kind: AdminAccountKind,
    pub admins: AdminList,
    pub creator_account_id: AccountId,
    pub created_at: BlockNumber,
    pub updated_at: BlockNumber,
    pub status: AdminAccountStatus,
}

/// 成员集合变更动作，沿用原裸结构编码。
#[derive(
    Clone, Debug, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, TypeInfo, MaxEncodedLen,
)]
#[scale_info(skip_type_params(AccountId, AdminList))]
pub struct AdminSetChangeAction<AccountId, AdminList> {
    /// 需要变更成员集合的个人多签账户。
    pub personal_account_id: AccountId,
    /// 提案通过后写入的完整管理员集合。
    pub admins: AdminList,
    /// 提案通过后写入投票引擎的个人动态阈值。
    pub new_threshold: u32,
}
