//! 登录认证数据模型与请求/响应 DTO。
//!
//! 本文件只放登录会话、challenge、二维码登录结果和接口 DTO;
//! handler、鉴权守卫、签名验签逻辑分别放在同目录其他文件中。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LoginSignRequest {
    pub(crate) challenge_id: String,
    /// 用户码确定的唯一目标账户；签名请求创建后禁止再由响应方改写。
    pub(crate) account_id: String,
    pub(crate) challenge_text: String,
    pub(crate) challenge_token: String,
    pub(crate) qr_aud: String,
    pub(crate) qr_origin: String,
    pub(crate) origin: String,
    pub(crate) domain: String,
    pub(crate) session_id: String,
    pub(crate) nonce: String,
    pub(crate) issued_at: DateTime<Utc>,
    #[serde(with = "chrono::serde::ts_milliseconds")]
    pub(crate) expires_at: DateTime<Utc>,
    pub(crate) consumed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AdminSession {
    /// 明文只来自本次请求，Serde不写入数据库；定位键另存原SHA-256。
    #[serde(skip)]
    pub(crate) token: String,
    pub(crate) account_id: String,
    pub(crate) institution_code: String,
    /// 会话签发时命中的链上机构候选。每次鉴权必须与节点当前绑定严格一致。
    pub(crate) candidate_id: String,
    /// 登录时已核验的上下文；业务读取只消费此快照，不重新读链鉴权。
    pub(crate) institution_cid_number: String,
    pub(crate) admin_level: Option<String>,
    pub(crate) family_name: String,
    pub(crate) given_name: String,
    pub(crate) scope_province_name: Option<String>,
    pub(crate) scope_city_name: Option<String>,
    pub(crate) scope_town_name: Option<String>,
    pub(crate) cid_short_name: Option<String>,
    pub(crate) capabilities: crate::modules::capability::CapabilitySet,
    pub(crate) workspace: crate::workspace::InstitutionWorkspace,
    /// 首次密钥设置未完成时，只允许初始化通路；不保存凭证副本。
    pub(crate) setup_required: bool,
    #[serde(with = "chrono::serde::ts_milliseconds")]
    pub(crate) expires_at: DateTime<Utc>,
    #[serde(with = "chrono::serde::ts_milliseconds")]
    pub(crate) last_active_at: DateTime<Utc>,
}

impl AdminSession {
    pub(crate) fn new(
        token: String,
        candidate_id: String,
        now: DateTime<Utc>,
        admin: AdminIdentifyOutput,
    ) -> Self {
        Self {
            token,
            candidate_id,
            expires_at: now + chrono::Duration::hours(8),
            last_active_at: now,
            account_id: admin.account_id,
            institution_code: admin.institution_code,
            institution_cid_number: admin.institution_cid_number,
            admin_level: admin.admin_level,
            family_name: admin.family_name,
            given_name: admin.given_name,
            scope_province_name: admin.scope_province_name,
            scope_city_name: admin.scope_city_name,
            scope_town_name: admin.scope_town_name,
            cid_short_name: admin.cid_short_name,
            capabilities: admin.capabilities,
            workspace: admin.workspace,
            setup_required: admin.setup_required,
        }
    }

    pub(crate) fn context(&self) -> AdminAuthContext {
        AdminAuthContext {
            account_id: self.account_id.clone(),
            institution_code: self.institution_code.clone(),
            institution_cid_number: self.institution_cid_number.clone(),
            admin_level: self.admin_level.clone(),
            family_name: self.family_name.clone(),
            given_name: self.given_name.clone(),
            scope_province_name: self.scope_province_name.clone(),
            scope_city_name: self.scope_city_name.clone(),
            scope_town_name: self.scope_town_name.clone(),
            cid_short_name: self.cid_short_name.clone(),
        }
    }

    pub(crate) fn identify(&self) -> AdminIdentifyOutput {
        AdminIdentifyOutput {
            account_id: self.account_id.clone(),
            institution_code: self.institution_code.clone(),
            institution_cid_number: self.institution_cid_number.clone(),
            admin_level: self.admin_level.clone(),
            family_name: self.family_name.clone(),
            given_name: self.given_name.clone(),
            scope_province_name: self.scope_province_name.clone(),
            scope_city_name: self.scope_city_name.clone(),
            scope_town_name: self.scope_town_name.clone(),
            cid_short_name: self.cid_short_name.clone(),
            capabilities: self.capabilities,
            workspace: self.workspace.clone(),
            setup_required: self.setup_required,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct AdminInstitutionCandidate {
    pub candidate_id: String,
    pub institution_code: String,
    pub admin_level: Option<String>,
    pub institution_cid_number: Option<String>,
    pub frg_province_code: Option<String>,
    pub cid_full_name: Option<String>,
    pub cid_short_name: Option<String>,
    pub scope_province_name: Option<String>,
    pub scope_city_name: Option<String>,
    pub scope_town_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct NodeInstitutionBinding {
    pub binding_id: String,
    /// 绑定只持久化链上身份键；名称和行政权限禁止写入绑定表，使用时从各自真源派生。
    pub candidate_id: String,
    pub institution_code: String,
    pub institution_cid_number: String,
    pub bound_account_id: String,
    pub bound_at: DateTime<Utc>,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NodeBindingChallenge {
    pub(crate) binding_challenge_id: String,
    pub(crate) account_id: String,
    pub(crate) candidates: Vec<AdminInstitutionCandidate>,
    #[serde(with = "chrono::serde::ts_milliseconds")]
    pub(crate) expires_at: DateTime<Utc>,
    pub(crate) consumed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub(crate) struct QrLoginResultRecord {
    pub(crate) session_id: String,
    pub(crate) access_token: String,
    #[serde(with = "chrono::serde::ts_milliseconds")]
    pub(crate) expires_at: DateTime<Utc>,
    pub(crate) account_id: String,
    pub(crate) institution_code: String,
    pub(crate) created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminAuthContext {
    pub account_id: String,
    /// 当前会话绑定的准确机构 CID；机构码只负责分类，不能代替机构唯一身份。
    pub institution_cid_number: String,
    /// 所属机构码(3/4 字符文本,前端据此渲染工作台入口与能力)。
    pub institution_code: String,
    /// 行政层级标签(NATIONAL/PROVINCE/CITY/TOWN);私权法人/非法人无层级为 None。
    pub admin_level: Option<String>,
    pub family_name: String,
    pub given_name: String,
    pub scope_province_name: Option<String>,
    /// 市级及以下机构有值：登记的市（用于列表按市过滤、生成时强制锁定）。
    pub scope_city_name: Option<String>,
    /// 镇级机构有值：登记的镇（用于列表按镇过滤、生成时强制锁定）。
    pub scope_town_name: Option<String>,
    /// 当前管理员所属机构简称,字段名与 subjects.cid_short_name 保持唯一命名。
    pub cid_short_name: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AdminAuthOutput {
    pub(crate) setup_required: bool,
    pub(crate) ok: bool,
    pub(crate) account_id: String,
    pub(crate) institution_cid_number: String,
    pub(crate) institution_code: String,
    pub(crate) admin_level: Option<String>,
    /// 机构能力位(后端单源,前端据此渲染工作台入口)。
    pub(crate) capabilities: crate::modules::capability::CapabilitySet,
    /// 当前机构工作台清单,用于前端按机构类型挂载 UI。
    pub(crate) workspace: crate::workspace::InstitutionWorkspace,
    pub(crate) family_name: String,
    pub(crate) given_name: String,
    pub(crate) scope_province_name: Option<String>,
    pub(crate) scope_city_name: Option<String>,
    pub(crate) scope_town_name: Option<String>,
    pub(crate) cid_short_name: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AdminIdentifyOutput {
    pub(crate) setup_required: bool,
    pub(crate) account_id: String,
    /// 扫码登录成功后由节点激活绑定派生，禁止浏览器自报。
    pub(crate) institution_cid_number: String,
    pub(crate) institution_code: String,
    pub(crate) admin_level: Option<String>,
    /// 机构能力位(后端单源,前端据此渲染工作台入口)。
    pub(crate) capabilities: crate::modules::capability::CapabilitySet,
    /// 当前机构工作台清单,用于前端按机构类型挂载 UI。
    pub(crate) workspace: crate::workspace::InstitutionWorkspace,
    pub(crate) family_name: String,
    pub(crate) given_name: String,
    pub(crate) scope_province_name: Option<String>,
    pub(crate) scope_city_name: Option<String>,
    pub(crate) scope_town_name: Option<String>,
    pub(crate) cid_short_name: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AdminQrSignRequestInput {
    /// 管理员先出示的完整 QR_V1/k=5 钱包码；后端只从中读取规范 `b.account_id`。
    pub(crate) identity_qr: String,
    pub(crate) origin: Option<String>,
    pub(crate) domain: Option<String>,
    pub(crate) session_id: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AdminQrSignRequestOutput {
    pub(crate) challenge_id: String,
    pub(crate) challenge_payload: String,
    pub(crate) login_qr_payload: String,
    pub(crate) origin: String,
    pub(crate) domain: String,
    pub(crate) session_id: String,
    pub(crate) expires_at: i64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AdminQrCompleteInput {
    pub(crate) challenge_id: String,
    pub(crate) session_id: Option<String>,
    pub(crate) account_id: String,
    pub(crate) signature: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AdminQrResultQuery {
    pub(crate) challenge_id: String,
    pub(crate) session_id: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AdminQrResultOutput {
    pub(crate) status: String,
    pub(crate) message: String,
    pub(crate) access_token: Option<String>,
    pub(crate) expires_at: Option<i64>,
    pub(crate) admin: Option<AdminIdentifyOutput>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NodeBindingConfirmInput {
    pub(crate) binding_challenge_id: String,
    pub(crate) candidate_id: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NodeBindingRequiredOutput {
    pub(crate) binding_challenge_id: String,
    pub(crate) account_id: String,
    pub(crate) candidates: Vec<AdminInstitutionCandidate>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AdminLoginCompleteOutput {
    pub(crate) status: String,
    pub(crate) access_token: Option<String>,
    pub(crate) expires_at: Option<i64>,
    pub(crate) admin: Option<AdminIdentifyOutput>,
    pub(crate) binding: Option<NodeBindingRequiredOutput>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AdminVerifyOutput {
    pub(crate) access_token: String,
    pub(crate) expires_at: i64,
    pub(crate) admin: AdminIdentifyOutput,
}
