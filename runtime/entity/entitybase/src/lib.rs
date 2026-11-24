//! 提供实体生命周期、机构资料和账户查询的共用类型及接口。

#![cfg_attr(not(feature = "std"), no_std)]

//! 实体生命周期共用类型与 trait。
//!
//! 本 crate 是 `runtime/entity` 下唯一共享模块，集中机构生命周期共用类型
//! (`RegisteredInstitution`/`InstitutionInfo` 等,
//! 由 publicentity / privateentity re-export)与统一查询 trait,本 crate 不持有 storage。
//! 公权机构、私权机构分别以 CID 和账户记录是否存在表达当前事实；
//! 个人多签继续在自己的 pallet 保存个人账户生命周期状态；
//! 下游模块通过这里的 trait 做统一查询，不直接读取某个实体 pallet 的 storage。

extern crate alloc;

use alloc::vec::Vec;
use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::pallet_prelude::DecodeWithMemTracking;
use primitives::cid::code::InstitutionCode;
use scale_info::TypeInfo;
use sp_runtime::Debug;

// 机构与个人多签共用的账户合法性、保留地址、保护地址检查 trait
// 仍以 primitives::multisig 为唯一真源，entitybase 只做实体侧统一出口。
pub use primitives::multisig::{AccountValidator, ProtectedSourceChecker, ReservedAccountGuard};

pub mod business_action;
pub mod institution_governance;
pub mod institution_role;
pub use business_action::{
    fixed_institution_capability_allows, fixed_role_permission_specs, FixedRolePermissionSpec,
};
pub use institution_governance::{
    InstitutionAssignmentTarget, InstitutionGovernanceAction, InstitutionGovernanceProposal,
    InstitutionGovernanceResult, InstitutionGovernanceResultHandler,
    InstitutionLegalRepresentativeChange, InstitutionRoleAssignmentChange, InstitutionRoleMutation,
    InstitutionRoleRegistration,
    RolePermissionSpec,
};
pub use institution_role::{
    generate_dynamic_role_code, AuthorizationSubject, BusinessActionId, InstitutionAssignment,
    InstitutionAssignmentSource, InstitutionAssignmentStatus, InstitutionCapabilityPolicy,
    InstitutionRole, InstitutionRoleAuthorizationQuery, InstitutionRoleQuery,
    InstitutionRoleStatus, RoleBusinessPermission, RolePermissionOperation, RoleSubject,
    ASSIGNMENT_SOURCE_REF_MAX_BYTES, BUSINESS_MODULE_TAG_MAX_BYTES,
    INSTITUTION_ROLE_CODE_MAX_BYTES, MAX_ROLE_PERMISSIONS_PER_ROLE,
};

// ===== 机构生命周期共用 storage 值类型(唯一真源) =====
// publicentity / privateentity 逐字段一致地复用以下类型;两 pallet 各自
// `pub use entitybase::{...}` 出口,保持既有对外 API(`publicentity::InstitutionInfo` 等)不变。
// 均为 SCALE 存储值类型:字段顺序、derive 集合、枚举判别值必须与既有链上编码一致。

/// CID 机构登记反向索引项：account_id → (cid_number, account_name)。
///
/// 由机构创建或机构新增账户流程与正向账户记录原子写入，用作反向校验。
#[derive(
    Encode, Decode, DecodeWithMemTracking, Clone, Debug, TypeInfo, MaxEncodedLen, PartialEq, Eq,
)]
pub struct RegisteredInstitution<CidNumber, AccountName> {
    pub cid_number: CidNumber,
    pub account_name: AccountName,
}

/// 机构当前法定代表人的公开人员信息。
///
/// 人的姓名在全仓只使用 `family_name`、`given_name`；“法定代表人”语义由外层
/// `legal_representative` 字段表达，不再另造合并姓名或带身份前缀的姓名字段。
#[derive(
    Encode, Decode, DecodeWithMemTracking, Clone, Debug, TypeInfo, MaxEncodedLen, PartialEq, Eq,
)]
pub struct LegalRepresentative<AccountName, CidNumber> {
    pub family_name: AccountName,
    pub given_name: AccountName,
    pub person_cid: CidNumber,
}

/// 机构信息(链上最小集)。
///
/// 链上只保存全国可见的机构身份事实:`cid_number` 作 storage key 已编码省/市/机构码/法人/盈利;
/// 镇归属使用统一字段 `town_code`:镇行政区公权机构由注册局创建时写入,当前私权机构写空值;
/// 主账户/费用账户由 `(cid_number, 保留名)` 派生且常驻 `InstitutionAccounts`,故不在此重复存;
/// 岗位任职和治理阈值由所属实体分别保存。
/// 公权/私权机构名称均以上链字段为准;OnChina 只保留查询缓存。
#[derive(
    Encode, Decode, DecodeWithMemTracking, Clone, Debug, TypeInfo, MaxEncodedLen, PartialEq, Eq,
)]
pub struct InstitutionInfo<BlockNumber, AccountName, CidNumber> {
    /// 机构全称。
    pub cid_full_name: AccountName,
    /// 机构简称。
    pub cid_short_name: AccountName,
    /// 所属镇代码。非镇行政区机构与当前私权机构写空值;镇行政区公权机构由注册局创建时写入。
    pub town_code: AccountName,
    /// 法定代表人公开人员信息。创世没有真实任免资料时为 None。
    pub legal_representative: Option<LegalRepresentative<AccountName, CidNumber>>,
    /// 机构分类和业务路由使用的机构码。
    pub institution_code: InstitutionCode,
    /// 机构注册创建区块号。
    pub created_at: BlockNumber,
}

/// 机构下某个账户名对应的链上账户信息。
#[derive(
    Encode, Decode, DecodeWithMemTracking, Clone, Debug, TypeInfo, MaxEncodedLen, PartialEq, Eq,
)]
pub struct InstitutionAccountInfo<AccountId, Balance, BlockNumber> {
    pub account_id: AccountId,
    /// 创建该逻辑账户时指定的初始余额，只是历史事实，不是当前余额真源。
    pub initial_balance: Balance,
    pub created_at: BlockNumber,
}

/// 关闭机构多签账户提案的业务数据(公权/私权通用,存入投票引擎 ProposalData)。
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub struct CloseInstitutionAction<AccountId, CidNumber> {
    pub actor_cid_number: CidNumber,
    pub institution_account_id: AccountId,
    pub beneficiary_account_id: AccountId,
    pub proposer_account_id: AccountId,
}

/// 创建机构时用户填写的账户初始余额项。
#[derive(
    Encode, Decode, DecodeWithMemTracking, Clone, Debug, TypeInfo, MaxEncodedLen, PartialEq, Eq,
)]
pub struct InstitutionInitialAccount<AccountName, Balance> {
    pub account_name: AccountName,
    pub amount: Balance,
}

/// 机构注册交易的账户项，保存已经派生好的地址，避免重复解释账户名。
#[derive(
    Encode, Decode, DecodeWithMemTracking, Clone, Debug, TypeInfo, MaxEncodedLen, PartialEq, Eq,
)]
pub struct CreateInstitutionAccount<AccountName, AccountId, Balance> {
    pub account_name: AccountName,
    pub account_id: AccountId,
    pub amount: Balance,
}

/// runtime 内实体生命周期分类。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EntityKind {
    /// 公权机构生命周期，由 `publicentity` 承载。
    PublicInstitution,
    /// 私权机构生命周期，由 `privateentity` 承载。
    PrivateInstitution,
    /// 个人多签生命周期，由 `personal` 承载。
    PersonalMultisig,
}

/// 机构多签账户查询 trait，供交易、清算、验签等下游模块使用。
///
/// 输入机构账户地址，查询所属机构 CID、机构码与账户登记状态。
/// 公权/私权 pallet 各自实现本 trait，runtime 再提供聚合查询适配器。
pub trait InstitutionMultisigQuery<AccountId> {
    /// 按机构 CID 与账户名读取唯一账户，并同时核验正向记录和反向索引。
    fn lookup_institution_account(cid_number: &[u8], account_name: &[u8]) -> Option<AccountId> {
        let _ = (cid_number, account_name);
        None
    }

    /// 精确确认账户属于指定机构 CID；实现不得只信任单向索引。
    fn account_belongs_to(cid_number: &[u8], addr: &AccountId) -> bool {
        Self::account_exists(addr) && Self::lookup_cid(addr).as_deref() == Some(cid_number)
    }

    /// 返回机构账户所属唯一 CID。个人多签没有 CID,不得返回伪 CID。
    fn lookup_cid(addr: &AccountId) -> Option<Vec<u8>>;

    /// 返回机构账户所属机构码。
    fn lookup_org(addr: &AccountId) -> Option<InstitutionCode>;

    /// 返回该机构账户是否存在于当前机构账户集合。
    fn account_exists(addr: &AccountId) -> bool;
}

/// 机构 CID 是否已在某个实体生命周期 pallet 中登记。
///
/// 用于 public/private 两个机构生命周期模块互相查询，防止同一 CID
/// 在两个模块中重复登记。本 trait 不持有 storage，不形成第二个共享真源。
pub trait InstitutionCidQuery<CidNumber> {
    /// CID 是否已存在。
    fn cid_exists(cid_number: &CidNumber) -> bool;
}

/// 机构法定代表人查询接口。
///
/// 法定代表人是机构公开信息。人员 CID 以本机构 `InstitutionInfo` 为准，当前钱包由 identity 解析。
pub trait InstitutionLegalRepresentativeQuery<AccountId> {
    /// 按机构唯一 CID 读取当前已任命的法定代表人账户。
    fn legal_representative(cid_number: &[u8]) -> Option<AccountId>;

    /// 按机构 CID 读取法定代表人的人员 CID。公权使用 CTZN，私权可使用 CTZN 或 NATP。
    fn legal_representative_cid(_cid_number: &[u8]) -> Option<Vec<u8>> {
        None
    }
}

impl<CidNumber> InstitutionCidQuery<CidNumber> for () {
    fn cid_exists(_cid_number: &CidNumber) -> bool {
        false
    }
}

impl<AccountId> InstitutionLegalRepresentativeQuery<AccountId> for () {
    fn legal_representative(_cid_number: &[u8]) -> Option<AccountId> {
        None
    }
}

impl<AccountId> InstitutionMultisigQuery<AccountId> for () {
    fn lookup_cid(_addr: &AccountId) -> Option<Vec<u8>> {
        None
    }

    fn lookup_org(_addr: &AccountId) -> Option<InstitutionCode> {
        None
    }

    fn account_exists(_addr: &AccountId) -> bool {
        false
    }
}

/// 注册局登记/维护权限抽象。
///
/// 以签名、注册局机构 CID 和岗位码确定行权人员，再按目标机构的登记权限和辖区校验。
pub trait RegistryAuthority<AccountId> {
    /// 当前 origin 是否可代表 actor CID 登记/维护目标机构。
    fn can_register_institution_origin(
        registrar: &AccountId,
        actor_cid_number: &[u8],
        actor_role_code: &[u8],
        target_cid_number: &[u8],
        target_institution_code: InstitutionCode,
    ) -> bool;
}

impl<AccountId> RegistryAuthority<AccountId> for () {
    fn can_register_institution_origin(
        _registrar: &AccountId,
        _actor_cid_number: &[u8],
        _actor_role_code: &[u8],
        _target_cid_number: &[u8],
        _target_institution_code: InstitutionCode,
    ) -> bool {
        false
    }
}



#[cfg(test)]
#[path = "../tests/mod.rs"]
mod tests;

/// 只读链阶段，用于私权机构保留既有的法定代表人资料校验规则。
pub trait ChainPhaseCheck {
    fn is_operation() -> bool;
}

impl ChainPhaseCheck for () {
    fn is_operation() -> bool {
        false
    }
}
