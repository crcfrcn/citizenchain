//! 管理端操作权限三档分级(读 / 本地写 / 链上写)。
//!
//! 三档之外一律拒绝;写操作一律 ≥ passkey,不存在只会话的写动作:
//! - Session         只读查询:仅需有效会话(会话已是链上已证管理员);由 `require_admin_any`
//!                   保障,不经 AdminActionType(AdminActionType 全是写动作)。
//! - Passkey         本地写:会话 + WebAuthn passkey 断言;只改 onchina 本地库、不产生 extrinsic。
//! - PasskeyColdSign 链上写:会话 + passkey + 冷钱包对真实链载荷签名。

// 权限校验失败必须直接返回统一 Axum Response，不另造可被业务模块分叉解释的错误类型。
#![allow(clippy::result_large_err)]

use axum::http::StatusCode;
use serde::{Deserialize, Serialize};

use crate::api_error;
use crate::authentication::login::AdminAuthContext;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AdminOperationAuth {
    /// 只读查询:仅需有效会话(会话已是链上已证管理员)。写动作不属此档。
    Session,
    /// 本地写:会话 + WebAuthn passkey 断言;只改 onchina 本地库、不产生 extrinsic。
    Passkey,
    /// 链上写:会话 + passkey + 冷钱包对真实链载荷签名(signer ∈ 本机构链上 Active 集合)。
    PasskeyColdSign,
}

/// 管理端动作类型(Tier 中性命名,决策②)。
///
/// 注册局动作按分层命名——Governing = Tier1 创世注册局自身(本期 = 联邦注册局),
/// Subordinate = 其供给的 Tier2 下级注册局(本期 = 市注册局)。命名与具体机构码解耦,
/// 鉴权边界经 `is_tier1_registry` 谓词裁决,不再字面绑定 FRG/CREG。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AdminActionType {
    /// Tier1 创世注册局新增一名 Tier2 下级注册局管理员。
    #[serde(rename = "CREATE_SUBORDINATE_REGISTRY")]
    CreateCityRegistry,
    /// Tier1 删除一名 Tier2 下级注册局管理员。
    #[serde(rename = "DELETE_SUBORDINATE_REGISTRY")]
    DeleteCityRegistry,
    InstitutionCreate,
    InstitutionUpdate,
    InstitutionCreateAccount,
    InstitutionDeleteAccount,
    InstitutionUploadDocument,
    InstitutionDeleteDocument,
    /// 本节点解除当前机构绑定;解绑后必须重新扫码登录并绑定机构。
    NodeBindingUnbind,
    // ───────── 立法与表决(卡 20260630-onchina-legislation-console-framework)─────────
    // 全部产生链上交易(提交 extrinsic / 改提案状态),归 PasskeyColdSign 特殊档。
    /// 发起立法(新法)法律案。
    ProposeEnactLaw,
    /// 发起修法法律案。
    ProposeAmendLaw,
    /// 发起废法法律案。
    ProposeRepealLaw,
    /// 代表机构表决（管理员按当前机构席位投票）。
    CastRepresentativeVote,
    /// 特别案立法公投。
    CastReferendumVote,
    /// 行政签署 / 否决(总统/省长/市长;另线程接入)。
    ExecutiveSign,
    /// 三人会签救济(院长 + 参议长 + 众议长;另线程接入)。
    OverrideSign,
    /// 护宪大法官终审(修宪;另线程接入)。
    GuardVote,
    /// 发起任免案(政府;Phase 4 接入)。
    ProposePersonnel,
    /// 发起预算案(政府;Phase 4 接入)。
    ProposeBudget,
    /// 注册局推送公民身份上链(prepare 生成公民待签载荷 + complete 验签绑定,
    /// 公民上链操作：一次 Passkey 后进入短期业务操作会话，最终链签独立完成。
    CitizenOnchainPush,
    /// 模块扩展动作必须有实际登记及所属模块授权；不能自行降为只会话写入。
    #[serde(untagged)]
    Module(String),
}

// JSON与公开调度共用原解析器；模块字符串不能绕过已撤销动作名的拒绝规则。
impl<'de> Deserialize<'de> for AdminActionType {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let action = String::deserialize(deserializer)?;
        parse_action_type(&action).map_err(|_| serde::de::Error::custom("unknown action_type"))
    }
}

impl AdminActionType {
    pub fn as_str(&self) -> &str {
        match self {
            Self::CreateCityRegistry => "CREATE_SUBORDINATE_REGISTRY",
            Self::DeleteCityRegistry => "DELETE_SUBORDINATE_REGISTRY",
            Self::InstitutionCreate => "INSTITUTION_CREATE",
            Self::InstitutionUpdate => "INSTITUTION_UPDATE",
            Self::InstitutionCreateAccount => "INSTITUTION_CREATE_ACCOUNT",
            Self::InstitutionDeleteAccount => "INSTITUTION_DELETE_ACCOUNT",
            Self::InstitutionUploadDocument => "INSTITUTION_UPLOAD_DOCUMENT",
            Self::InstitutionDeleteDocument => "INSTITUTION_DELETE_DOCUMENT",
            Self::NodeBindingUnbind => "NODE_BINDING_UNBIND",
            Self::ProposeEnactLaw => "PROPOSE_ENACT_LAW",
            Self::ProposeAmendLaw => "PROPOSE_AMEND_LAW",
            Self::ProposeRepealLaw => "PROPOSE_REPEAL_LAW",
            Self::CastRepresentativeVote => "CAST_REPRESENTATIVE_VOTE",
            Self::CastReferendumVote => "CAST_REFERENDUM_VOTE",
            Self::ExecutiveSign => "EXECUTIVE_SIGN",
            Self::OverrideSign => "OVERRIDE_SIGN",
            Self::GuardVote => "GUARD_VOTE",
            Self::ProposePersonnel => "PROPOSE_PERSONNEL",
            Self::ProposeBudget => "PROPOSE_BUDGET",
            Self::CitizenOnchainPush => "CITIZEN_ONCHAIN_PUSH",
            Self::Module(name) => name.as_str(),
        }
    }

    /// 动作 → 鉴权档(穷尽 match,新增动作漏标编译失败=默认拒绝)。
    ///
    /// 三档 = 读 / 本地写 / 链上写。AdminActionType 全是写动作,故只落 Passkey / PasskeyColdSign
    /// 两档;只读查询归 Session,由 `require_admin_any` 会话门保障,不经 AdminActionType。
    pub fn auth_type(&self) -> AdminOperationAuth {
        match self {
            // 本地写(Passkey):只改 onchina 本地库,不产生 extrinsic。
            Self::InstitutionUploadDocument
            | Self::InstitutionDeleteDocument
            | Self::NodeBindingUnbind
            // 最终管理员链签就是该角色唯一钱包签名,创建阶段只额外一次 passkey。
            | Self::CitizenOnchainPush => AdminOperationAuth::Passkey,
            // 链上写(PasskeyColdSign):产生链上交易/凭证、改 Active 集合或高危治理。
            // InstitutionUpdate 改 cid_full_name/法人/所属法人(链上注册凭证签名字段=链上单源),
            //   归链上写;前端本就走冷签。若存在纯本地展示字段,Phase 2/3 再拆出为本地写(Passkey)。
            Self::InstitutionUpdate
            | Self::InstitutionCreate
            | Self::InstitutionCreateAccount
            | Self::CreateCityRegistry
            | Self::DeleteCityRegistry
            | Self::InstitutionDeleteAccount
            // 立法与表决:全部产生链上交易。
            | Self::ProposeEnactLaw
            | Self::ProposeAmendLaw
            | Self::ProposeRepealLaw
            | Self::CastRepresentativeVote
            | Self::CastReferendumVote
            | Self::ExecutiveSign
            | Self::OverrideSign
            | Self::GuardVote
            | Self::ProposePersonnel
            | Self::ProposeBudget
            | Self::Module(_) => AdminOperationAuth::PasskeyColdSign,
        }
    }

    pub(crate) fn requires_module(&self) -> bool {
        matches!(
            self,
            Self::CreateCityRegistry
                | Self::DeleteCityRegistry
                | Self::InstitutionCreate
                | Self::Module(_)
        )
    }

    /// prepare 只允许冷签动作和明确支持的本地解绑，不能泛化放开所有写入。
    pub(crate) fn can_prepare(&self) -> bool {
        self.auth_type() == AdminOperationAuth::PasskeyColdSign || *self == Self::NodeBindingUnbind
    }

    pub(crate) fn is_governance(&self) -> bool {
        matches!(
            self,
            Self::CreateCityRegistry | Self::DeleteCityRegistry | Self::NodeBindingUnbind
        )
    }

    /// 是否要求 Tier1 创世注册局治理能力。注册局自身管理(增删下级注册局、更新/换届本档)
    /// 归此边界；机构元数据更新与文档上传不在其中——任一辖区管理员可对本辖区机构执行,
    /// 注册局资格与 `scope` 共同限定办理权限。机构自定义账户增删属机构自管(不经注册局审批),也不在此边界:
    /// 由机构在册管理员直接冷签 propose_close,链端以 `is_institution_admin` 鉴权。
    /// 与鉴权档正交:不依赖 auth_type,故动作在档间迁移不改变此权限边界。
    pub(crate) fn requires_governing_capability(&self) -> bool {
        matches!(self, Self::CreateCityRegistry | Self::DeleteCityRegistry)
    }
}

pub fn parse_action_type(action_type: &str) -> Result<AdminActionType, axum::response::Response> {
    match action_type {
        "CREATE_SUBORDINATE_REGISTRY" => Ok(AdminActionType::CreateCityRegistry),
        "DELETE_SUBORDINATE_REGISTRY" => Ok(AdminActionType::DeleteCityRegistry),
        "INSTITUTION_CREATE" => Ok(AdminActionType::InstitutionCreate),
        "INSTITUTION_UPDATE" => Ok(AdminActionType::InstitutionUpdate),
        "INSTITUTION_CREATE_ACCOUNT" => Ok(AdminActionType::InstitutionCreateAccount),
        "INSTITUTION_DELETE_ACCOUNT" => Ok(AdminActionType::InstitutionDeleteAccount),
        "INSTITUTION_UPLOAD_DOCUMENT" => Ok(AdminActionType::InstitutionUploadDocument),
        "INSTITUTION_DELETE_DOCUMENT" => Ok(AdminActionType::InstitutionDeleteDocument),
        "NODE_BINDING_UNBIND" => Ok(AdminActionType::NodeBindingUnbind),
        "PROPOSE_ENACT_LAW" => Ok(AdminActionType::ProposeEnactLaw),
        "PROPOSE_AMEND_LAW" => Ok(AdminActionType::ProposeAmendLaw),
        "PROPOSE_REPEAL_LAW" => Ok(AdminActionType::ProposeRepealLaw),
        "CAST_REPRESENTATIVE_VOTE" => Ok(AdminActionType::CastRepresentativeVote),
        "CAST_REFERENDUM_VOTE" => Ok(AdminActionType::CastReferendumVote),
        "EXECUTIVE_SIGN" => Ok(AdminActionType::ExecutiveSign),
        "OVERRIDE_SIGN" => Ok(AdminActionType::OverrideSign),
        "GUARD_VOTE" => Ok(AdminActionType::GuardVote),
        "PROPOSE_PERSONNEL" => Ok(AdminActionType::ProposePersonnel),
        "PROPOSE_BUDGET" => Ok(AdminActionType::ProposeBudget),
        "CITIZEN_ONCHAIN_PUSH" => Ok(AdminActionType::CitizenOnchainPush),
        // 旧注册局名不是模块扩展动作，也不作为新名称的兼容别名。
        other
            if !matches!(other, "CREATE_CITY_REGISTRY" | "DELETE_CITY_REGISTRY")
                && !other.is_empty()
                && other.len() <= 128
                && other
                    .bytes()
                    .all(|v| v.is_ascii_uppercase() || v.is_ascii_digit() || v == b'_') =>
        {
            Ok(AdminActionType::Module(other.to_string()))
        }
        _ => Err(api_error(
            StatusCode::BAD_REQUEST,
            1001,
            "unknown action_type",
        )),
    }
}

pub(crate) fn ensure_action_role_allowed(
    ctx: &AdminAuthContext,
    action_type: &AdminActionType,
) -> Result<(), axum::response::Response> {
    if matches!(
        action_type,
        AdminActionType::CreateCityRegistry
            | AdminActionType::DeleteCityRegistry
            | AdminActionType::InstitutionCreate
            | AdminActionType::CitizenOnchainPush
            | AdminActionType::InstitutionUpdate
            | AdminActionType::InstitutionUploadDocument
            | AdminActionType::InstitutionDeleteDocument
    ) {
        crate::authorization::rules::ensure_registry_admin(ctx)?;
    }
    if ctx.scope_province_name.is_none() && action_type.requires_governing_capability() {
        return Err(api_error(
            StatusCode::FORBIDDEN,
            1003,
            "admin province scope missing",
        ));
    }
    if action_type.requires_governing_capability()
        && !crate::chain_runtime::is_tier1_registry(&ctx.institution_code)
    {
        return Err(api_error(
            StatusCode::FORBIDDEN,
            1003,
            "governing registry admin required",
        ));
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
#[path = "../tests/authentication_operation_auth.rs"]
mod tests;
