//! 登录认证数据模型与请求/响应 DTO。
//!
//! 本文件只放登录会话、challenge、二维码登录结果和接口 DTO;
//! handler、鉴权守卫、签名验签逻辑分别放在同目录其他文件中。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    pub(crate) expire_at: DateTime<Utc>,
    pub(crate) consumed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct AdminSession {
    pub(crate) token: String,
    pub(crate) account_id: String,
    pub(crate) institution_code: String,
    /// 会话签发时命中的链上机构候选。每次鉴权必须与节点当前绑定严格一致。
    pub(crate) candidate_id: String,
    pub(crate) expire_at: DateTime<Utc>,
    #[serde(default = "default_now_utc")]
    pub(crate) last_active_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct AdminInstitutionCandidate {
    pub(crate) candidate_id: String,
    pub(crate) institution_code: String,
    pub(crate) admin_level: Option<String>,
    pub(crate) institution_cid_number: Option<String>,
    pub(crate) frg_province_code: Option<String>,
    pub(crate) cid_full_name: Option<String>,
    pub(crate) cid_short_name: Option<String>,
    pub(crate) scope_province_name: Option<String>,
    pub(crate) scope_city_name: Option<String>,
    pub(crate) scope_town_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct NodeInstitutionBinding {
    pub(crate) binding_id: String,
    /// 绑定只持久化链上身份键；名称和行政权限禁止写入绑定表，使用时从各自真源派生。
    pub(crate) candidate_id: String,
    pub(crate) institution_code: String,
    pub(crate) institution_cid_number: String,
    pub(crate) frg_province_code: Option<String>,
    pub(crate) bound_account_id: String,
    pub(crate) bound_at: DateTime<Utc>,
    pub(crate) status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct NodeBindingChallenge {
    pub(crate) binding_challenge_id: String,
    pub(crate) account_id: String,
    pub(crate) candidates: Vec<AdminInstitutionCandidate>,
    pub(crate) expire_at: DateTime<Utc>,
    pub(crate) consumed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct QrLoginResultRecord {
    pub(crate) session_id: String,
    pub(crate) access_token: String,
    pub(crate) expire_at: DateTime<Utc>,
    pub(crate) account_id: String,
    pub(crate) institution_code: String,
    pub(crate) created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AdminAuthContext {
    pub(crate) account_id: String,
    /// 当前会话绑定的准确机构 CID；机构码只负责分类，不能代替机构唯一身份。
    pub(crate) institution_cid_number: String,
    /// 所属机构码(3/4 字符文本,前端据此渲染工作台入口与能力)。
    pub(crate) institution_code: String,
    /// 行政层级标签(NATIONAL/PROVINCE/CITY/TOWN);私权法人/非法人无层级为 None。
    pub(crate) admin_level: Option<String>,
    pub(crate) family_name: String,
    pub(crate) given_name: String,
    pub(crate) scope_province_name: Option<String>,
    /// 市级及以下机构有值：登记的市（用于列表按市过滤、生成时强制锁定）。
    pub(crate) scope_city_name: Option<String>,
    /// 镇级机构有值：登记的镇（用于列表按镇过滤、生成时强制锁定）。
    pub(crate) scope_town_name: Option<String>,
    /// 当前管理员所属机构简称,字段名与 subjects.cid_short_name 保持唯一命名。
    pub(crate) cid_short_name: Option<String>,
}

#[derive(Serialize)]
pub(crate) struct AdminAuthOutput {
    pub(crate) ok: bool,
    pub(crate) account_id: String,
    pub(crate) institution_cid_number: String,
    pub(crate) institution_code: String,
    pub(crate) admin_level: Option<String>,
    /// 机构能力位(后端单源,前端据此渲染工作台入口)。
    pub(crate) capabilities: crate::platform::capability::CapabilitySet,
    /// 当前机构工作台清单,用于前端按机构类型挂载 UI。
    pub(crate) workspace: crate::workspace::InstitutionWorkspace,
    pub(crate) family_name: String,
    pub(crate) given_name: String,
    pub(crate) scope_province_name: Option<String>,
    pub(crate) scope_city_name: Option<String>,
    pub(crate) scope_town_name: Option<String>,
    pub(crate) cid_short_name: Option<String>,
}

#[derive(Serialize)]
pub(crate) struct AdminIdentifyOutput {
    pub(crate) account_id: String,
    /// 扫码登录成功后由节点激活绑定派生，禁止浏览器自报。
    pub(crate) institution_cid_number: String,
    pub(crate) institution_code: String,
    pub(crate) admin_level: Option<String>,
    /// 机构能力位(后端单源,前端据此渲染工作台入口)。
    pub(crate) capabilities: crate::platform::capability::CapabilitySet,
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
pub(crate) struct AdminQrSignRequestInput {
    /// 管理员先出示的完整 QR_V1/k=5 钱包码；后端只从中读取规范 `b.account_id`。
    pub(crate) identity_qr: String,
    pub(crate) origin: Option<String>,
    pub(crate) domain: Option<String>,
    pub(crate) session_id: Option<String>,
}

#[derive(Serialize)]
pub(crate) struct AdminQrSignRequestOutput {
    pub(crate) challenge_id: String,
    pub(crate) challenge_payload: String,
    pub(crate) login_qr_payload: String,
    pub(crate) origin: String,
    pub(crate) domain: String,
    pub(crate) session_id: String,
    pub(crate) expire_at: i64,
}

#[derive(Deserialize)]
pub(crate) struct AdminQrCompleteInput {
    pub(crate) challenge_id: String,
    pub(crate) session_id: Option<String>,
    pub(crate) account_id: String,
    pub(crate) signature: String,
}

#[derive(Deserialize)]
pub(crate) struct AdminQrResultQuery {
    pub(crate) challenge_id: String,
    pub(crate) session_id: String,
}

#[derive(Serialize)]
pub(crate) struct AdminQrResultOutput {
    pub(crate) status: String,
    pub(crate) message: String,
    pub(crate) access_token: Option<String>,
    pub(crate) expire_at: Option<i64>,
    pub(crate) admin: Option<AdminIdentifyOutput>,
}

#[derive(Deserialize)]
pub(crate) struct NodeBindingConfirmInput {
    pub(crate) binding_challenge_id: String,
    pub(crate) candidate_id: String,
}

#[derive(Serialize)]
pub(crate) struct NodeBindingRequiredOutput {
    pub(crate) binding_challenge_id: String,
    pub(crate) account_id: String,
    pub(crate) candidates: Vec<AdminInstitutionCandidate>,
}

#[derive(Serialize)]
pub(crate) struct AdminLoginCompleteOutput {
    pub(crate) status: String,
    pub(crate) access_token: Option<String>,
    pub(crate) expire_at: Option<i64>,
    pub(crate) admin: Option<AdminIdentifyOutput>,
    pub(crate) binding: Option<NodeBindingRequiredOutput>,
}

#[derive(Serialize)]
pub(crate) struct AdminVerifyOutput {
    pub(crate) access_token: String,
    pub(crate) expire_at: i64,
    pub(crate) admin: AdminIdentifyOutput,
}

pub(crate) fn default_now_utc() -> DateTime<Utc> {
    Utc::now()
}
