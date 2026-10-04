//! 机构/账户两层数据模型
//!
//! 链端 `InstitutionAccounts::<T>(cid_number, account_name) → account_info`
//! 是 DoubleMap，一个 cid_number 下可挂多个机构账户。
//! cid 系统这里对应拆两层:
//!
//! - `Institution`:每个 cid_number 唯一,存机构展示信息(cid_full_name 等),
//!   **不**进链。
//! - `InstitutionAccount`:以 `(cid_number, account_name)` 为复合 key,account_name 是
//!   **进链的 name**,一个机构下可挂多个。
//!
//! 详见 `feedback_institutions_two_layer.md`。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::cid::InstitutionCategory;

pub const EDUCATION_TYPE_NATIONAL_CITIZEN_EDU_COMMITTEE: &str = "NATIONAL_CITIZEN_EDU_COMMITTEE";
pub const EDUCATION_TYPE_CITY_CITIZEN_EDU_COMMITTEE: &str = "CITY_CITIZEN_EDU_COMMITTEE";

// ── 账户链上状态 ───────────────────────────────────────

/// 机构账户链上状态。
///
/// 账户是否激活只以链上事实为准。CID 创建账户时只是登记
/// `(cid_number, account_name)`,默认 `NotOnChain`;链上机构注册或新增账户成功后,
/// 由同步接口写成 `ActiveOnChain`;链上注销后写成 `RevokedOnChain`。
/// 机构(每个 cid_number 唯一)。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Institution {
    /// CID 号,参与链上派生。
    pub cid_number: String,
    /// 机构全称。列表可用简称优先展示,详情页同时展示全称和简称。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cid_full_name: Option<String>,
    /// 机构简称。确定性公权机构必须写入规范简称,不得重复写全称。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cid_short_name: Option<String>,
    /// 机构展示分类(公权机构/私权机构)。法律主体类型由机构码和父级属性单独判定。
    pub category: InstitutionCategory,
    /// 盈利属性("0"/"1")。
    pub p1: String,
    /// 所属省名称(如"安徽省")。
    pub province_name: String,
    /// 所属市名称(如"合肥市")。
    pub city_name: String,
    /// 所属镇名称。非镇目录机构为空。
    #[serde(default)]
    pub town_name: String,
    /// 所属省代码(r5 前 2 字符)。
    pub province_code: String,
    /// 所属市代码(r5 后 3 字符)。链投影公权机构的稳定地域键,市名改动时保持不变。
    #[serde(default)]
    pub city_code: String,
    /// 所属镇代码。只有镇目录机构填写。
    #[serde(default)]
    pub town_code: String,
    /// 机构类型代码(ZF/LF/SF/...)。
    pub institution_code: String,
    /// 教育机构业务分类。只用于教育 tab 分类,不参与 CID 号生成。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub education_type: Option<String>,
    /// 私权机构类型。仅私权机构有值,取值见 `private/common::PrivateType`。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub private_type: Option<String>,
    /// 合伙企业形态。仅 private_type=PARTNERSHIP 时有值:GENERAL / LIMITED。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub partnership_kind: Option<String>,
    /// 是否具有法人资格。仅私权机构有值;公权机构由主体属性 G 表达法人资格。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub has_legal_personality: Option<bool>,
    /// 从属关系引用。字段值始终是另一个机构已有的 `cid_number`,不是第二套身份 ID。
    /// - 需要挂靠的非法人组织(UNIN):指向所属法人。
    ///   个体经营(SFGT)和无限合伙(SFGP)是独立非法人,不填写本字段。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_cid_number: Option<String>,
    /// 法定代表人公开身份；初始化目录机构没有真实任免资料时允许为空。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub legal_representative: Option<LegalRepresentative>,
    /// 法定代表人证件照数据库记录的鉴权读取地址。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub legal_representative_photo_path: Option<String>,
    /// 法定代表人证件照原始文件名。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub legal_representative_photo_name: Option<String>,
    /// 法定代表人证件照 MIME 类型。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub legal_representative_photo_mime: Option<String>,
    /// 法定代表人证件照大小(字节)。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub legal_representative_photo_size: Option<u64>,
    /// 创建人账户 ID；链上创世投影没有独立创建人时为空。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub creator_account_id: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// 法定代表人公开身份。人的姓名只保存分离的姓、名，不保存拼接姓名。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LegalRepresentative {
    pub family_name: String,
    pub given_name: String,
    /// 必须指向正常状态公民的 cid_number。
    pub cid_number: String,
    /// 由注册局公民记录派生，不接受机构表单另填。
    pub account_id: String,
}

/// 机构下的多签账户(复合 key = (cid_number, account_name))。
#[derive(Debug, Clone, Deserialize)]
pub struct InstitutionAccount {
    /// 所属机构的 cid_number。
    pub cid_number: String,
    /// 账户名称,**进链的 name 字段**。同 cid_number 下必须唯一。
    pub account_name: String,
    /// 链上派生的规范账户 ID。上链成功后填入。
    pub account_id: Option<String>,
    pub creator_account_id: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl Serialize for InstitutionAccount {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;

        let kind = primitives::account_derive::institution_kind_by_name(
            self.cid_number.as_bytes(),
            self.account_name.as_bytes(),
        )
        .ok_or_else(|| serde::ser::Error::custom("institution account_name is empty"))?;
        let account_kind = match kind.institution_protocol_kind() {
            Some(primitives::account_derive::InstitutionProtocolAccountKind::Main) => "main",
            Some(primitives::account_derive::InstitutionProtocolAccountKind::Fee) => "fee",
            Some(primitives::account_derive::InstitutionProtocolAccountKind::Stake) => "stake",
            Some(primitives::account_derive::InstitutionProtocolAccountKind::SafetyFund) => {
                "safety_fund"
            }
            Some(primitives::account_derive::InstitutionProtocolAccountKind::He) => "he",
            Some(primitives::account_derive::InstitutionProtocolAccountKind::Clearing) => {
                "clearing"
            }
            Some(
                primitives::account_derive::InstitutionProtocolAccountKind::FederalCitizenSecurityFund,
            ) => "federal_citizen_security_fund",
            None => "named",
        };
        let can_close = kind.is_closable_institution_account();
        let mut state = serializer.serialize_struct("InstitutionAccount", 8)?;
        state.serialize_field("cid_number", &self.cid_number)?;
        state.serialize_field("account_name", &self.account_name)?;
        state.serialize_field("account_id", &self.account_id)?;
        state.serialize_field("account_kind", account_kind)?;
        state.serialize_field("can_close", &can_close)?;
        state.serialize_field("can_delete", &can_close)?;
        state.serialize_field("creator_account_id", &self.creator_account_id)?;
        state.serialize_field("created_at", &self.created_at)?;
        state.end()
    }
}

/// 机构资料库文档(注册文件/许可证/章程等)。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstitutionDocument {
    /// 自增文档 ID。
    pub id: u64,
    /// 所属机构 cid_number。
    pub cid_number: String,
    /// 原始文件名。
    pub file_name: String,
    /// 文档类型(公司章程/营业许可证/股东会决议/法人授权书/其他)。
    pub doc_type: String,
    /// 文件大小(字节)。
    pub file_size: u64,
    /// 服务端存储路径(相对于 data/documents/)。
    pub file_path: String,
    /// 上传人账户 ID。
    pub uploader_account_id: String,
    pub uploaded_at: DateTime<Utc>,
}

/// 文档类型枚举值。
pub const VALID_DOC_TYPES: &[&str] =
    &["公司章程", "营业许可证", "股东会决议", "法人授权书", "其他"];

// ─── 请求/响应 DTO ──────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateInstitutionAdminInput {
    /// 管理员姓；缺失时先查公民资料，仍缺失则使用“管理”。
    #[serde(default)]
    pub family_name: Option<String>,
    /// 管理员名；缺失时先查公民资料，仍缺失则使用“员”。
    #[serde(default)]
    pub given_name: Option<String>,
    /// 机构初始管理员账户 ID。
    pub account_id: String,
}

#[derive(Debug, Deserialize)]
// serde 创建 DTO:字段是前端 JSON API 契约,7 个私权创建 handler 反序列化后按分支消费;
// subject_property 为第 6 步原子创建业务预留(当前接口固定关闭),非死代码。
#[allow(dead_code)]
pub struct CreateInstitutionInput {
    /// 主体属性(G/F/S)。仅保留给第 6 步新原子创建业务的资料录入，当前接口固定关闭。
    pub subject_property: String,
    pub p1: Option<String>,
    pub province_name: Option<String>,
    pub city_name: String,
    /// 镇级公权机构运行期注册时必填;非镇级机构必须为空。
    #[serde(default)]
    pub town_name: Option<String>,
    pub institution: String,
    /// 教育机构业务分类。仅 `institution=JY` 的教育入口使用,不参与 CID 号生成。
    #[serde(default)]
    pub education_type: Option<String>,
    /// 机构全称。私权、公权和教育新增都应在创建阶段写入 cid_full_name。
    pub cid_full_name: Option<String>,
    /// 所属法人身份ID。仅需要挂靠的非法人(F)使用;个体经营和无限合伙是独立非法人,
    /// 不接受所属法人。
    #[serde(default)]
    pub parent_cid_number: Option<String>,
    pub cid_short_name: Option<String>,
    /// 私权机构类型。私权入口创建时必传,由后端锁定主体属性和机构码。
    #[serde(default)]
    pub private_type: Option<String>,
    /// 合伙类型。private_type=PARTNERSHIP 时必传,其它类型不接收。
    #[serde(default)]
    pub partnership_kind: Option<String>,
    /// 旧创建输入中的初始管理员集合；公私权按目标类型编码。
    /// 机构治理阈值不得再由管理员人数推导，恢复创建前须另立方案。
    #[serde(default)]
    pub admins: Vec<CreateInstitutionAdminInput>,
}

/// 机构详情页提交的可编辑字段。私权类型由身份 ID 机构码决定,创建后不允许改。
#[derive(Debug, Deserialize)]
pub struct UpdateInstitutionInput {
    #[serde(default)]
    pub cid_full_name: Option<String>,
    // serde 编辑 DTO 契约字段:前端可提交简称,当前后端更新流程未消费,保留契约形。
    #[serde(default)]
    #[allow(dead_code)]
    pub cid_short_name: Option<String>,
    /// 所属法人 cid_number(仅 F 可设置;S/G 传值会被拒)
    #[serde(default)]
    pub parent_cid_number: Option<String>,
    /// 法定代表人姓、名、CID 与证件照资料在机构编辑保存时必填。
    #[serde(default)]
    pub family_name: Option<String>,
    #[serde(default)]
    pub given_name: Option<String>,
    #[serde(default)]
    pub legal_representative_cid_number: Option<String>,
    #[serde(default)]
    pub legal_representative_photo_path: Option<String>,
    #[serde(default)]
    pub legal_representative_photo_name: Option<String>,
    #[serde(default)]
    pub legal_representative_photo_mime: Option<String>,
    #[serde(default)]
    pub legal_representative_photo_size: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LegalRepresentativePhoto {
    pub file_path: String,
    pub file_name: String,
    pub mime_type: String,
    pub file_size: u64,
}

#[derive(Debug, Deserialize)]
pub struct CreateAccountInput {
    pub account_name: String,
    /// 发起人当前任职的机构岗位码;runtime 据此在 origin 处校验发起提案权限。
    pub proposer_role_code: String,
}

/// 关闭机构自定义账户提案的请求体。DELETE 也带 Json body 传岗位码,
/// 与新增账户一致由 runtime 在 origin 处以 `is_institution_admin` + 岗位码校验。
#[derive(Debug, Deserialize)]
pub struct DeleteAccountInput {
    /// 发起人当前任职的机构岗位码。
    pub proposer_role_code: String,
}

/// /api/institutions/list 的列表过滤维度(查询参数,不是存储 category)。
///
/// JY 教育机构统一归教育 tab,私权目标类型归 private tab,公权目录仍承接公权本体
/// 和公权下属非法人:
/// - `Private`:私权 tab = 目标私权类型,可用 private_type 继续过滤;
/// - `Gov`:公权 tab = 非 JY 公权机构 + 父级为公法人的非 JY 非法人;
/// - `Education`:教育 tab = 确定性国家/市公民教育委员会 + 法人学校 + F+JY 分支机构。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstitutionListFilter {
    Private,
    Gov,
    Education,
}

impl InstitutionListFilter {
    /// 拼进列表 SQL 的静态过滤子句(三分支均为静态字面量,无注入面)。
    pub fn sql_clause(&self) -> &'static str {
        match self {
            Self::Private => {
                "AND s.category = 'PRIVATE_INSTITUTION' AND s.private_type IS NOT NULL"
            }
            Self::Gov => {
                "AND ((s.category = 'GOV_INSTITUTION'
                       AND s.institution_code NOT IN ('NED', 'CEDU', 'GUN', 'SUN', 'GSCH', 'SFSC'))
                      OR (s.institution_code IN ('SFGT', 'SFGP', 'UNIN')
                          AND s.institution_code NOT IN ('NED', 'CEDU', 'GUN', 'SUN', 'GSCH', 'SFSC')
                          AND par.category = 'GOV_INSTITUTION')))"
            }
            Self::Education => {
                "AND s.institution_code IN ('GUN', 'SUN', 'GSCH', 'SFSC')"
            }
        }
    }
}

#[derive(Debug, Serialize)]
pub struct InstitutionListRow {
    pub cid_number: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cid_full_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cid_short_name: Option<String>,
    pub category: InstitutionCategory,
    pub p1: String,
    pub province_name: String,
    pub city_name: String,
    #[serde(default)]
    pub town_name: String,
    pub institution_code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub education_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub private_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub partnership_kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_legal_personality: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_cid_number: Option<String>,
    pub account_count: usize,
    pub created_at: DateTime<Utc>,
    /// 创建该机构的登录管理员姓；与管理员记录字段同名，不保存合并姓名。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub creator_family_name: Option<String>,
    /// 创建该机构的登录管理员名；页面展示时再与姓合并。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub creator_given_name: Option<String>,
    /// 创建者角色:"FEDERAL_REGISTRY" / "CITY_REGISTRY" / None
    #[serde(skip_serializing_if = "Option::is_none")]
    pub creator_institution_code: Option<String>,
}

/// 法人机构搜索结果项(用于 F 详情页"所属法人"选择器)
#[derive(Debug, Serialize)]
pub struct ParentInstitutionRow {
    pub cid_number: String,
    pub cid_full_name: String,
    /// 私权机构类型。前端只用于展示父级机构事实,不派生链上业务角色。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub private_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub partnership_kind: Option<String>,
    pub category: InstitutionCategory,
    /// 盈利属性。非法人创建时前端按"盈利属性附属于所属法人"用它推导 F 的 p1
    /// (公法人父级恒 0;私法人父级继承该值),后端 `unincorporated_org::inherited_p1` 复核。
    pub p1: String,
    pub province_name: String,
    pub city_name: String,
    #[serde(default)]
    pub town_name: String,
}

#[derive(Debug, Serialize)]
pub struct InstitutionDetailOutput {
    pub institution: Institution,
    pub accounts: Vec<InstitutionAccount>,
    /// 创建该机构的登录管理员姓(按 creator_account_id 账户反查 admins)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub creator_family_name: Option<String>,
    /// 创建该机构的登录管理员名(按 creator_account_id 账户反查 admins)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub creator_given_name: Option<String>,
    /// 创建者角色:"FEDERAL_REGISTRY" / "CITY_REGISTRY"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub creator_institution_code: Option<String>,
}
