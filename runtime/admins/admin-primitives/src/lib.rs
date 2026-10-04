#![cfg_attr(not(feature = "std"), no_std)]
//! 管理员共用原语。
//!
//! 本 crate 只放管理员共用类型、trait 与分类策略，不放业务 storage，
//! 也不直接创建任何 pallet。`public-admins`、`private-admins` 和
//! `personal-admins` 必须在各自模块内维护自己的管理员状态。

extern crate alloc;

use alloc::vec::Vec;
use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use frame_support::{dispatch::DispatchResult, traits::ConstU32, BoundedVec};
use primitives::cid::code::{
    is_fixed_governance_code, is_private_legal_code, is_public_legal_code, is_unincorporated_code,
    InstitutionCode, PMUL,
};
use primitives::core_const::CID_NUMBER_MAX_BYTES;
use scale_info::TypeInfo;
use sp_runtime::Debug;

/// 固定治理公权机构码,唯一真源在 `primitives::cid::code`。
pub use primitives::cid::code::{FRG, NJD};

/// 管理员集合所属机构 CID 号类型。
pub type AdminCidNumber = BoundedVec<u8, ConstU32<CID_NUMBER_MAX_BYTES>>;

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

/// 机构与个人多签管理员人员记录（公权/私权/个人多签统一复用）。
///
/// `cid_number` 引用 citizen-identity 真源;当前仅承载身份、不参与授权（授权仍按 `account_id`）。
/// 字段完整性按(机构类型, 是否 LR 岗)分层要求，由 `required_admin_elements` 单源 +
/// `ChainPhaseCheck` 期段门控：创世期允许空、运行期(Operation)强制。
/// `family_name`、`given_name` 与链上中国公民姓名字段逐字对齐。机构岗位和任职由 entity 独立保存。
#[derive(
    Encode, Decode, DecodeWithMemTracking, Clone, Debug, TypeInfo, MaxEncodedLen, PartialEq, Eq,
)]
pub struct Admin<AccountId> {
    pub account_id: AccountId,
    /// 管理员本人公民 CID（引用 citizen-identity 真源）。当前不参与授权；
    /// 是否必填按 `required_admin_elements` 分层、`ChainPhaseCheck` 期段门控。个人多签恒空。
    pub cid_number: AdminCidNumber,
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

    /// 按必填要素校验本管理员**原始**字段是否齐全。
    /// 须在 `normalize_names` 之前调用，避免默认“管理/员”掩盖真正的空值。
    pub fn satisfies(&self, req: RequiredAdminElements) -> bool {
        (!req.cid || !self.cid_number.is_empty())
            && (!req.family || !self.family_name.is_empty())
            && (!req.given || !self.given_name.is_empty())
    }
}

/// 管理员集合所属类型。
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
    /// 公权机构管理员；固定治理机构也是公权机构，只是创世写入并采用固定阈值。
    PublicInstitution,
    /// 私权机构管理员。
    ///
    /// 非法人不是私权同义词;上层必须按所属法人归属把非法人路由到
    /// public-admins 或 private-admins。
    PrivateInstitution,
    /// 个人多签管理员。
    PersonalMultisig,
}

/// 某管理员按(机构类型, 是否 LR 岗)必须完整提供哪些要素。account_id 恒必填,不在此表。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RequiredAdminElements {
    pub cid: bool,
    pub family: bool,
    pub given: bool,
}

/// 分层强制单源真值表（= 运行期 Operation 的必填要素；account_id 恒必填不在表内）。
///
/// 门控在调用侧:仅 `ChainPhaseCheck::is_operation() == true` 时按本表强制;
/// Genesis（创世/开发期）一律放行=允许空。一次 runtime 升级把 Phase 翻 Operation
/// 即全域启用强制,无专属迁移。
///   - `PublicInstitution`            => 全 true（公权:所有管理员四要素完整）
///   - `PrivateInstitution` & LR 岗    => 全 true（私权:仅法定代表人岗四要素完整）
///   - `PrivateInstitution` & 非 LR 岗 => 全 false（仅 account_id）
///   - `PersonalMultisig`             => 全 false 【死规则,永不可翻真;个人多签禁强制姓/名/CID】
///
/// LR 岗判定单源:`role_code == primitives::institution_constraints::ROLE_CODE_LEGAL_REPRESENTATIVE`
/// (`b"LR"`),由持岗位任职上下文的 entity 层计算 `is_lr_role` 传入。
pub fn required_admin_elements(kind: AdminAccountKind, is_lr_role: bool) -> RequiredAdminElements {
    match kind {
        // 公权:所有管理员四要素完整。
        AdminAccountKind::PublicInstitution => RequiredAdminElements {
            cid: true,
            family: true,
            given: true,
        },
        // 私权:仅法定代表人(LR)岗四要素完整。
        AdminAccountKind::PrivateInstitution if is_lr_role => RequiredAdminElements {
            cid: true,
            family: true,
            given: true,
        },
        // 私权非 LR 岗:仅 account_id。
        AdminAccountKind::PrivateInstitution => RequiredAdminElements {
            cid: false,
            family: false,
            given: false,
        },
        // 个人多签:仅 account_id;cid 永 false【死规则,节点守卫锁死禁强制】。
        AdminAccountKind::PersonalMultisig => RequiredAdminElements {
            cid: false,
            family: false,
            given: false,
        },
    }
}

/// 运行期强制门控注入。链进入 Operation 期后 `required_admin_elements` 才生效。
///
/// 由 runtime 用 genesis-pallet 的 `Phase` 实现注入(仿 `DeveloperUpgradeCheck` 范式)。
/// 定义在 admin-primitives 而非 genesis-pallet:后者反向依赖各 admin/entity pallet,
/// 定义于此可让 public-admins/private-manage 在 Config 约束引用而不成环。
pub trait ChainPhaseCheck {
    /// 链是否已进入运行期(Operation)。Genesis 返回 false。
    fn is_operation() -> bool;
}

/// 测试 no-op 默认:恒 Genesis(放行),供不关心相位的单测 mock 直接用 `()`。
/// 生产 runtime 必显式注入 `GenesisPallet`,不用本默认。
impl ChainPhaseCheck for () {
    fn is_operation() -> bool {
        false
    }
}

/// 个人多签管理员集合生命周期。
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

/// 机构管理员集合。
///
/// 本结构只保存管理员人员记录及必要路由状态；岗位、任期和任职来源全部归
/// `entity` 的岗位任职存储。机构管理员没有“创建人、创建时间、更新时间”字段。
#[derive(
    Encode, Decode, DecodeWithMemTracking, Clone, Debug, TypeInfo, MaxEncodedLen, PartialEq, Eq,
)]
#[scale_info(skip_type_params(AdminList))]
pub struct InstitutionAdmins<AdminList> {
    /// 机构码，用于把查询路由到对应公权或私权业务。
    pub institution_code: InstitutionCode,
    /// 按账户去重的管理员人员集合。
    pub admins: AdminList,
}

/// 个人多签管理员集合记录。
#[derive(
    Encode, Decode, DecodeWithMemTracking, Clone, Debug, TypeInfo, MaxEncodedLen, PartialEq, Eq,
)]
#[scale_info(skip_type_params(AdminList))]
pub struct AdminAccount<AdminList, AccountId, BlockNumber> {
    /// 个人多签没有机构 CID，固定为空。
    pub cid_number: AdminCidNumber,
    pub institution_code: InstitutionCode,
    pub kind: AdminAccountKind,
    pub admins: AdminList,
    pub creator_account_id: AccountId,
    pub created_at: BlockNumber,
    pub updated_at: BlockNumber,
    pub status: AdminAccountStatus,
}

/// 管理员集合变更提案业务数据。
#[derive(
    Clone, Debug, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, TypeInfo, MaxEncodedLen,
)]
#[scale_info(skip_type_params(AccountId, AdminList))]
pub struct AdminSetChangeAction<AccountId, AdminList> {
    /// 个人多签账户。机构管理员变更统一按 CID，不使用本类型。
    pub personal_account_id: AccountId,
    /// 提案通过后写入的完整管理员集合。
    pub admins: AdminList,
    /// 提案通过后写入投票引擎的动态阈值；固定治理机构必须等于制度固定阈值。
    pub new_threshold: u32,
}

/// 个人多签管理员集合生命周期写入口。
///
/// 个人多签创建、注销等业务 pallet 只能通过此 trait 请求 personal-admins
/// 写入 Pending/Active/Closed，机构管理员不使用本生命周期模型。
pub trait AdminAccountLifecycle<AccountId, AdminItem = AccountId> {
    // 个人多签待激活记录必须逐字段绑定提案、主体和创建人，接口形状属于跨 pallet 契约。
    #[allow(clippy::too_many_arguments)]
    fn create_pending_admin_account_for_proposal(
        proposal_id: u64,
        module_tag: &[u8],
        personal_account_id: AccountId,
        cid_number: Vec<u8>,
        institution_code: InstitutionCode,
        kind: AdminAccountKind,
        admins: Vec<AdminItem>,
        creator_account_id: AccountId,
    ) -> DispatchResult;

    fn activate_admin_account_for_proposal(
        proposal_id: u64,
        module_tag: &[u8],
        personal_account_id: AccountId,
    ) -> DispatchResult;

    fn remove_pending_admin_account_for_proposal(
        proposal_id: u64,
        module_tag: &[u8],
        personal_account_id: AccountId,
    ) -> DispatchResult;

    fn close_admin_account_for_proposal(
        proposal_id: u64,
        module_tag: &[u8],
        personal_account_id: AccountId,
    ) -> DispatchResult;
}

/// 机构管理员集合写入口。
///
/// 机构的来源、岗位和任职全部由 entity 表达，因此该接口不接收 `creator_account_id`，也不承担
/// 个人多签的创建语义。公权与私权 entity 只通过本接口原子写入管理员人员集合。
pub trait InstitutionAdminLifecycle<AccountId, AdminItem = Admin<AccountId>> {
    /// 注册局或机构治理写入有效管理员人员集合；机构阈值由 entity 独立保存。
    fn set_institution_admins(
        module_tag: &[u8],
        cid_number: Vec<u8>,
        institution_code: InstitutionCode,
        kind: AdminAccountKind,
        admins: Vec<AdminItem>,
    ) -> DispatchResult;
}

/// 机构管理员集合统一查询口。
///
/// 机构身份只使用 CID；账户地址不能作为本 trait 的查询 key。
pub trait InstitutionAdminQuery<AccountId> {
    fn institution_admins_exist(institution_code: InstitutionCode, cid_number: &[u8]) -> bool;

    fn is_institution_admin(
        institution_code: InstitutionCode,
        cid_number: &[u8],
        who: &AccountId,
    ) -> bool;

    fn institution_admins(
        institution_code: InstitutionCode,
        cid_number: &[u8],
    ) -> Option<Vec<AccountId>>;

    fn institution_admins_len(institution_code: InstitutionCode, cid_number: &[u8]) -> Option<u32>;

    /// 把调用者钱包解析为名册中的**规范账户**(canonical `account_id`，即快照键)。
    /// - 运行期(Operation)对**有 CID** 的管理员：按 citizen-identity 绑定解析
    ///   (`matches_citizen_account(cid, caller)`)，换绑钱包后新钱包解析到同一 canonical、
    ///   旧钱包不再匹配 →「换绑不掉权」；
    /// - 创世期，或**无 CID** 的管理员(私权非 LR / 个人多签)：按 `account_id` 直配；
    /// - 返回 `None` 表示 caller 不是该机构管理员。
    ///
    /// 授权链路统一先经本方法解析，再用返回的 canonical 匹配名册与任职，`account_id`
    /// 由此降为签名快照、CID 成授权锚点(相位门控与 Phase 2 字段强制同一开关)。
    fn resolve_admin_account(
        institution_code: InstitutionCode,
        cid_number: &[u8],
        caller: &AccountId,
    ) -> Option<AccountId>;
}

impl<AccountId> InstitutionAdminQuery<AccountId> for () {
    fn institution_admins_exist(_institution_code: InstitutionCode, _cid_number: &[u8]) -> bool {
        false
    }

    fn is_institution_admin(
        _institution_code: InstitutionCode,
        _cid_number: &[u8],
        _who: &AccountId,
    ) -> bool {
        false
    }

    fn institution_admins(
        _institution_code: InstitutionCode,
        _cid_number: &[u8],
    ) -> Option<Vec<AccountId>> {
        None
    }

    fn institution_admins_len(
        _institution_code: InstitutionCode,
        _cid_number: &[u8],
    ) -> Option<u32> {
        None
    }

    fn resolve_admin_account(
        _institution_code: InstitutionCode,
        _cid_number: &[u8],
        _caller: &AccountId,
    ) -> Option<AccountId> {
        None
    }
}

/// 公权管理员非空公民 CID 与账户绑定查询。
///
/// 实现只能读取 `citizen-identity` 真源；public-admins 不得自行生成或修正公民 CID。
pub trait CitizenIdentityBindingQuery<AccountId> {
    fn matches_citizen_account(cid_number: &[u8], account: &AccountId) -> bool;
}

impl<AccountId> CitizenIdentityBindingQuery<AccountId> for () {
    fn matches_citizen_account(_cid_number: &[u8], _account: &AccountId) -> bool {
        false
    }
}

/// 个人多签管理员集合查询口。
///
/// 机构管理员已经使用 CID 专用查询；本 trait 只服务个人多签账户。
pub trait AdminAccountQuery<AccountId> {
    fn active_admin_account_exists(
        institution_code: InstitutionCode,
        personal_account_id: AccountId,
    ) -> bool;

    fn is_active_account_admin(
        institution_code: InstitutionCode,
        personal_account_id: AccountId,
        who: &AccountId,
    ) -> bool;

    fn active_account_admins(
        institution_code: InstitutionCode,
        personal_account_id: AccountId,
    ) -> Option<Vec<AccountId>>;

    /// 返回个人多签完整管理员人员记录；授权调用方仍只能比较账户。
    fn active_account_admin_records(
        institution_code: InstitutionCode,
        personal_account_id: AccountId,
    ) -> Option<Vec<Admin<AccountId>>>;

    fn active_account_admins_len(
        institution_code: InstitutionCode,
        personal_account_id: AccountId,
    ) -> Option<u32>;

    fn pending_account_exists_for_snapshot(
        _institution_code: InstitutionCode,
        _personal_account: AccountId,
    ) -> bool {
        false
    }

    fn is_pending_account_admin_for_snapshot(
        _institution_code: InstitutionCode,
        _personal_account: AccountId,
        _who: &AccountId,
    ) -> bool {
        false
    }

    fn pending_account_admins_for_snapshot(
        _institution_code: InstitutionCode,
        _personal_account: AccountId,
    ) -> Option<Vec<AccountId>> {
        None
    }

    fn pending_account_admin_records_for_snapshot(
        _institution_code: InstitutionCode,
        _personal_account: AccountId,
    ) -> Option<Vec<Admin<AccountId>>> {
        None
    }

    fn pending_account_admins_len_for_snapshot(
        _institution_code: InstitutionCode,
        _personal_account: AccountId,
    ) -> Option<u32> {
        None
    }
}

impl<AccountId> AdminAccountQuery<AccountId> for () {
    fn active_admin_account_exists(
        _institution_code: InstitutionCode,
        _personal_account: AccountId,
    ) -> bool {
        false
    }

    fn is_active_account_admin(
        _institution_code: InstitutionCode,
        _personal_account: AccountId,
        _who: &AccountId,
    ) -> bool {
        false
    }

    fn active_account_admins(
        _institution_code: InstitutionCode,
        _personal_account: AccountId,
    ) -> Option<Vec<AccountId>> {
        None
    }

    fn active_account_admin_records(
        _institution_code: InstitutionCode,
        _personal_account: AccountId,
    ) -> Option<Vec<Admin<AccountId>>> {
        None
    }

    fn active_account_admins_len(
        _institution_code: InstitutionCode,
        _personal_account: AccountId,
    ) -> Option<u32> {
        None
    }

    fn pending_account_exists_for_snapshot(
        _institution_code: InstitutionCode,
        _personal_account: AccountId,
    ) -> bool {
        false
    }

    fn is_pending_account_admin_for_snapshot(
        _institution_code: InstitutionCode,
        _personal_account: AccountId,
        _who: &AccountId,
    ) -> bool {
        false
    }

    fn pending_account_admins_for_snapshot(
        _institution_code: InstitutionCode,
        _personal_account: AccountId,
    ) -> Option<Vec<AccountId>> {
        None
    }

    fn pending_account_admin_records_for_snapshot(
        _institution_code: InstitutionCode,
        _personal_account: AccountId,
    ) -> Option<Vec<Admin<AccountId>>> {
        None
    }

    fn pending_account_admins_len_for_snapshot(
        _institution_code: InstitutionCode,
        _personal_account: AccountId,
    ) -> Option<u32> {
        None
    }
}

/// 判断机构码是否属于公权机构管理员模块。
pub fn is_public_admin_code(code: &InstitutionCode) -> bool {
    is_public_legal_code(code) || is_fixed_governance_code(code)
}

/// 判断机构码是否属于私权法人管理员模块。
pub fn is_private_admin_code(code: &InstitutionCode) -> bool {
    is_private_legal_code(code)
}

/// 判断机构码是否属于非法人机构管理员模块候选。
///
/// 非法人可隶属公法人或私法人;机构码本身不能决定管理员模块。
pub fn is_unincorporated_admin_code(code: &InstitutionCode) -> bool {
    is_unincorporated_code(code)
}

/// 判断机构码能否由公权管理员模块保存。
pub fn can_store_public_admin_code(code: &InstitutionCode) -> bool {
    is_public_admin_code(code) || is_unincorporated_admin_code(code)
}

/// 判断机构码能否由私权管理员模块保存。
pub fn can_store_private_admin_code(code: &InstitutionCode) -> bool {
    is_private_admin_code(code) || is_unincorporated_admin_code(code)
}

/// 判断机构码是否属于个人多签管理员模块。
pub fn is_personal_admin_code(code: &InstitutionCode) -> bool {
    *code == PMUL
}

/// 固定治理机构管理员人数；必须完整匹配机构码和 CID。
///
/// FRG 在 `admins` 中保存联邦注册局全部 215 名管理员；43 个省级 5 人岗位组
/// 由 `entity` 任职关系表达，不再维护第二套管理员分组 storage。
pub fn expected_fixed_governance_admins_len(
    code: InstitutionCode,
    cid_number: &[u8],
) -> Option<u32> {
    primitives::governance_skeleton::fixed_institution_by_identity(code, cid_number)
        .map(|institution| institution.expected_len)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 管理员声明序固定为账户、CID、姓、名，机构值只在前面增加机构码。
    #[test]
    fn institution_assignment_account_id_field_order_matches_node_guard() {
        use codec::Encode;

        let admin = Admin {
            account_id: 7u8,
            cid_number: AdminCidNumber::default(),
            family_name: FamilyName::truncate_from("张".as_bytes().to_vec()),
            given_name: GivenName::truncate_from("三".as_bytes().to_vec()),
        };
        let value = InstitutionAdmins {
            institution_code: *b"NRCG",
            admins: vec![admin.clone()],
        };
        assert_eq!(
            value.encode(),
            (
                *b"NRCG",
                vec![(7u8, admin.cid_number, admin.family_name, admin.given_name)]
            )
                .encode()
        );
    }

    #[test]
    fn admin_field_order_is_account_cid_family_given() {
        use codec::Encode;

        let cid_number = AdminCidNumber::truncate_from(b"CN220-CTZN2-198805200-2026".to_vec());
        let admin = Admin {
            account_id: 7u8,
            cid_number: cid_number.clone(),
            family_name: FamilyName::truncate_from("程".as_bytes().to_vec()),
            given_name: GivenName::truncate_from("伟".as_bytes().to_vec()),
        };
        assert_eq!(
            admin.encode(),
            (
                7u8,
                cid_number,
                admin.family_name.clone(),
                admin.given_name.clone()
            )
                .encode()
        );
    }

    #[test]
    fn missing_person_name_uses_management_default() {
        let admin = Admin {
            account_id: 1u8,
            cid_number: AdminCidNumber::default(),
            family_name: FamilyName::default(),
            given_name: GivenName::default(),
        }
        .normalize_names();
        assert_eq!(admin.family_name.as_slice(), "管理".as_bytes());
        assert_eq!(admin.given_name.as_slice(), "员".as_bytes());
    }

    /// 运行期真值表:公权全强制、私权 LR 岗全强制、私权非 LR/个人多签仅 account_id。
    #[test]
    fn required_admin_elements_operation_truth_table() {
        let public = required_admin_elements(AdminAccountKind::PublicInstitution, false);
        assert!(public.cid && public.family && public.given);

        let private_lr = required_admin_elements(AdminAccountKind::PrivateInstitution, true);
        assert!(private_lr.cid && private_lr.family && private_lr.given);

        let private_other = required_admin_elements(AdminAccountKind::PrivateInstitution, false);
        assert!(!private_other.cid && !private_other.family && !private_other.given);

        let personal = required_admin_elements(AdminAccountKind::PersonalMultisig, false);
        assert!(!personal.cid && !personal.family && !personal.given);
    }

    /// 死规则:个人多签 cid 无论是否 LR 均不强制（节点守卫据此锁死）。
    #[test]
    fn personal_multisig_cid_never_required() {
        for is_lr in [true, false] {
            assert!(!required_admin_elements(AdminAccountKind::PersonalMultisig, is_lr).cid);
        }
    }

    /// `satisfies` 按必填要素校验原始字段:必填项为空即不满足,全不必填则恒满足。
    #[test]
    fn admin_satisfies_by_required_elements() {
        let empty = Admin {
            account_id: 1u8,
            cid_number: AdminCidNumber::default(),
            family_name: FamilyName::default(),
            given_name: GivenName::default(),
        };
        // 全不必填 → 恒满足（私权非 LR / 个人多签）。
        assert!(empty.satisfies(RequiredAdminElements {
            cid: false,
            family: false,
            given: false,
        }));
        // 公权真值表要求四要素完整 → 空字段不满足。
        assert!(!empty.satisfies(required_admin_elements(
            AdminAccountKind::PublicInstitution,
            false,
        )));

        let full = Admin {
            account_id: 1u8,
            cid_number: AdminCidNumber::truncate_from(b"CN220-CTZN2-198805200-2026".to_vec()),
            family_name: FamilyName::truncate_from("张".as_bytes().to_vec()),
            given_name: GivenName::truncate_from("三".as_bytes().to_vec()),
        };
        assert!(full.satisfies(required_admin_elements(
            AdminAccountKind::PublicInstitution,
            false,
        )));
    }
}
