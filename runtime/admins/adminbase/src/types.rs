//! 管理员记录、生命周期状态和姓名规范化的唯一实现。
use crate::*;

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
/// `cid_number` 引用identity真源；名册account_id供快照使用，运行期调用授权由机构查询接口按当前CID绑定解析。
/// 字段完整性按(机构类型, 是否 LR 岗)分层要求，由 `required_admin_elements` 单源 +
/// `ChainPhaseCheck` 期段门控：创世期允许空、运行期(Operation)强制。
/// `family_name`、`given_name` 与链上中国公民姓名字段逐字对齐。机构岗位和任职由 entity 独立保存。
#[derive(
    Encode, Decode, DecodeWithMemTracking, Clone, Debug, TypeInfo, MaxEncodedLen, PartialEq, Eq,
)]
pub struct Admin<AccountId> {
    pub account_id: AccountId,
    /// 管理员本人公民 CID（引用 identity 真源）。运行期由所属机构查询接口参与身份授权解析；
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
    /// publicadmin 或 privateadmin。
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
