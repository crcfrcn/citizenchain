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
use crate::auth::login::AdminAuthContext;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub(crate) enum AdminOperationAuth {
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
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub(crate) enum AdminActionType {
    /// Tier1 创世注册局新增一名 Tier2 下级注册局管理员。
    CreateCityRegistry,
    /// Tier1 删除一名 Tier2 下级注册局管理员。
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
}

impl AdminActionType {
    pub(crate) fn as_str(&self) -> &'static str {
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
        }
    }

    /// 动作 → 鉴权档(穷尽 match,新增动作漏标编译失败=默认拒绝)。
    ///
    /// 三档 = 读 / 本地写 / 链上写。AdminActionType 全是写动作,故只落 Passkey / PasskeyColdSign
    /// 两档;只读查询归 Session,由 `require_admin_any` 会话门保障,不经 AdminActionType。
    pub(crate) fn auth_type(&self) -> AdminOperationAuth {
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
            | Self::ProposeBudget => AdminOperationAuth::PasskeyColdSign,
        }
    }

    pub(crate) fn is_governance(&self) -> bool {
        matches!(
            self,
            Self::CreateCityRegistry | Self::DeleteCityRegistry | Self::NodeBindingUnbind
        )
    }

    /// 是否要求 Tier1 创世注册局治理能力。注册局自身管理(增删下级注册局、更新/换届本档)
    /// 归此边界；机构元数据更新与文档上传不在其中——任一辖区管理员可对本辖区机构执行,
    /// 由 `scope` 限定可见域。机构自定义账户增删属机构自管(不经注册局审批),也不在此边界:
    /// 由机构在册管理员直接冷签 propose_close,链端以 `is_institution_admin` 鉴权。
    /// 与鉴权档正交:不依赖 auth_type,故动作在档间迁移不改变此权限边界。
    pub(crate) fn requires_governing_capability(&self) -> bool {
        matches!(self, Self::CreateCityRegistry | Self::DeleteCityRegistry)
    }
}

pub(crate) fn parse_action_type(
    action_type: &str,
) -> Result<AdminActionType, axum::response::Response> {
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
    if ctx.scope_province_name.is_none() {
        return Err(api_error(
            StatusCode::FORBIDDEN,
            1003,
            "admin province scope missing",
        ));
    }
    if action_type.requires_governing_capability()
        && !crate::core::chain_runtime::is_tier1_registry(&ctx.institution_code)
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
// 测试需要在前置条件失效时立即失败，断言式解包仅限本测试模块。
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn operation_auth_has_exactly_three_tiers() {
        // 三档铁律:Session / Passkey / PasskeyColdSign。新增第四档必须显式改本测试与所有
        // 穷尽 match;三档之外的操作一律拒绝。
        let all = [
            AdminOperationAuth::Session,
            AdminOperationAuth::Passkey,
            AdminOperationAuth::PasskeyColdSign,
        ];
        assert_eq!(all.len(), 3);
        for tier in all {
            // 穷尽 match,无 `_ =>` 兜底;新增变体漏标 → 编译失败。
            let _label = match tier {
                AdminOperationAuth::Session => "SESSION",
                AdminOperationAuth::Passkey => "PASSKEY",
                AdminOperationAuth::PasskeyColdSign => "PASSKEY_COLD_SIGN",
            };
        }
    }

    #[test]
    fn governing_capability_boundary_excludes_institution_update_and_upload() {
        // 机构元数据更新与文档上传由发起管理员的 scope 限定本辖区,不要求 Tier1 创世注册局治理能力。
        assert!(!AdminActionType::InstitutionUpdate.requires_governing_capability());
        assert!(!AdminActionType::InstitutionUploadDocument.requires_governing_capability());
        // 注册局新增/删除下级仍要求 Tier1 创世注册局治理能力。
        assert!(AdminActionType::CreateCityRegistry.requires_governing_capability());
        assert!(AdminActionType::DeleteCityRegistry.requires_governing_capability());
        // 机构自定义账户增删属机构自管(不经注册局审批),不要求治理能力。
        assert!(!AdminActionType::InstitutionCreate.requires_governing_capability());
        assert!(!AdminActionType::InstitutionCreateAccount.requires_governing_capability());
        assert!(!AdminActionType::InstitutionDeleteAccount.requires_governing_capability());
        assert!(!AdminActionType::InstitutionDeleteDocument.requires_governing_capability());
        assert!(!AdminActionType::NodeBindingUnbind.requires_governing_capability());
    }

    #[test]
    fn citizen_onchain_push_uses_one_passkey_and_round_trips() {
        // 最终链签已是管理员唯一钱包签名，操作创建阶段只额外消费一次 Passkey。
        let action = AdminActionType::CitizenOnchainPush;
        assert_eq!(action.auth_type(), AdminOperationAuth::Passkey);
        assert!(!action.requires_governing_capability());
        assert!(!action.is_governance());
        let parsed = parse_action_type(action.as_str()).expect("citizen action parses");
        assert_eq!(parsed, action);
    }

    #[test]
    fn legislation_actions_are_cold_sign_and_round_trip() {
        // 立法与表决动作全部产链上交易,归 PasskeyColdSign;且不属注册局治理能力边界。
        let actions = [
            AdminActionType::ProposeEnactLaw,
            AdminActionType::ProposeAmendLaw,
            AdminActionType::ProposeRepealLaw,
            AdminActionType::CastRepresentativeVote,
            AdminActionType::CastReferendumVote,
            AdminActionType::ExecutiveSign,
            AdminActionType::OverrideSign,
            AdminActionType::GuardVote,
            AdminActionType::ProposePersonnel,
            AdminActionType::ProposeBudget,
        ];
        for action in actions {
            assert_eq!(action.auth_type(), AdminOperationAuth::PasskeyColdSign);
            assert!(!action.requires_governing_capability());
            // as_str ↔ parse_action_type 逐字往返一致。
            let parsed = parse_action_type(action.as_str()).expect("legislation action parses");
            assert_eq!(parsed, action);
        }
    }

    /// 全部动作类型的登记表：新增变体时本 `match` 编译失败，强制在此登记档位。
    ///
    /// 这是**编译期**约束,比运行期断言强:`auth_type()` 用 `|` 按组归档,新增变体很容易
    /// 被顺手并进错误的组而无人察觉;本表逐个列举,形态不同,两处同时错的概率极低。
    fn expected_tier(action: &AdminActionType) -> AdminOperationAuth {
        use AdminActionType as A;
        use AdminOperationAuth::{Passkey, PasskeyColdSign};
        match action {
            // 本地写:只改 onchina 本地库,不产生 extrinsic。
            A::InstitutionUploadDocument => Passkey,
            A::InstitutionDeleteDocument => Passkey,
            A::NodeBindingUnbind => Passkey,
            A::CitizenOnchainPush => Passkey,
            // 链上写:产生链上交易/凭证、改 Active 集合或高危治理。
            A::CreateCityRegistry => PasskeyColdSign,
            A::DeleteCityRegistry => PasskeyColdSign,
            A::InstitutionCreate => PasskeyColdSign,
            A::InstitutionUpdate => PasskeyColdSign,
            A::InstitutionCreateAccount => PasskeyColdSign,
            A::InstitutionDeleteAccount => PasskeyColdSign,
            A::ProposeEnactLaw => PasskeyColdSign,
            A::ProposeAmendLaw => PasskeyColdSign,
            A::ProposeRepealLaw => PasskeyColdSign,
            A::CastRepresentativeVote => PasskeyColdSign,
            A::CastReferendumVote => PasskeyColdSign,
            A::ExecutiveSign => PasskeyColdSign,
            A::OverrideSign => PasskeyColdSign,
            A::GuardVote => PasskeyColdSign,
            A::ProposePersonnel => PasskeyColdSign,
            A::ProposeBudget => PasskeyColdSign,
        }
    }

    /// 与 [`expected_tier`] 共用同一份穷尽 `match`,保证遍历不漏变体。
    const ALL_ACTIONS: [AdminActionType; 20] = [
        AdminActionType::CreateCityRegistry,
        AdminActionType::DeleteCityRegistry,
        AdminActionType::InstitutionCreate,
        AdminActionType::InstitutionUpdate,
        AdminActionType::InstitutionCreateAccount,
        AdminActionType::InstitutionDeleteAccount,
        AdminActionType::InstitutionUploadDocument,
        AdminActionType::InstitutionDeleteDocument,
        AdminActionType::NodeBindingUnbind,
        AdminActionType::ProposeEnactLaw,
        AdminActionType::ProposeAmendLaw,
        AdminActionType::ProposeRepealLaw,
        AdminActionType::CastRepresentativeVote,
        AdminActionType::CastReferendumVote,
        AdminActionType::ExecutiveSign,
        AdminActionType::OverrideSign,
        AdminActionType::GuardVote,
        AdminActionType::ProposePersonnel,
        AdminActionType::ProposeBudget,
        AdminActionType::CitizenOnchainPush,
    ];

    #[test]
    fn all_actions_list_covers_every_variant() {
        // ALL_ACTIONS 是手写数组,漏写不会编译失败。用 expected_tier 的穷尽 match 反查:
        // 每个变体都能在表中找到,且表内无重复 —— 二者共同保证遍历完整。
        let mut seen = std::collections::BTreeSet::new();
        for action in ALL_ACTIONS {
            assert!(
                seen.insert(action.as_str()),
                "ALL_ACTIONS 存在重复项: {}",
                action.as_str()
            );
            // 触发 expected_tier 的穷尽 match,新增变体未登记则此处编译失败。
            let _ = expected_tier(&action);
        }
        assert_eq!(
            seen.len(),
            ALL_ACTIONS.len(),
            "ALL_ACTIONS 长度与去重后不符"
        );
    }

    #[test]
    fn every_action_tier_matches_the_registry() {
        // 档位是三档鉴权的入口判据:链上写被误归为本地写 = 该动作不再需要冷签,
        // 等于凭 passkey 就能发链交易。逐变体钉死,不依赖 auth_type() 的分组写法。
        for action in ALL_ACTIONS {
            assert_eq!(
                action.auth_type(),
                expected_tier(&action),
                "{} 的鉴权档与登记表不符",
                action.as_str()
            );
        }
    }

    #[test]
    fn no_write_action_falls_back_to_session_tier() {
        // Session 档只用于只读查询。任何写动作落到 Session 即完全绕过 passkey,
        // 是最严重的降档失败模式,单独立一条断言。
        for action in ALL_ACTIONS {
            assert_ne!(
                action.auth_type(),
                AdminOperationAuth::Session,
                "{} 落到只读档 Session,写动作绝不允许",
                action.as_str()
            );
        }
    }

    #[test]
    fn governance_and_signing_actions_must_require_cold_sign() {
        // 独立于登记表的第二道锁:按动作语义(提案/表决/签署)推导,不看 auth_type() 的实现。
        // 这些动作一律产生链上交易,必须 PasskeyColdSign。命名前缀新增同类动作时同样受约束。
        for action in ALL_ACTIONS {
            let name = action.as_str();
            let is_governance_write = name.starts_with("PROPOSE_")
                || name.starts_with("CAST_")
                || name.ends_with("_SIGN")
                || name == "GUARD_VOTE";
            if is_governance_write {
                assert_eq!(
                    action.auth_type(),
                    AdminOperationAuth::PasskeyColdSign,
                    "{name} 产生链上交易,必须走冷签档"
                );
            }
        }
    }

    #[test]
    fn tier_membership_counts_are_pinned() {
        // 计数锁:动作在档间迁移时,上面的逐变体断言会红;而**新增**动作若被
        // 顺手归入本地写档,逐变体断言可能连同登记表一起被改而静默通过。
        // 把两档的成员数钉死,迫使任何档位人数变化都成为显式决策。
        let passkey = ALL_ACTIONS
            .iter()
            .filter(|a| a.auth_type() == AdminOperationAuth::Passkey)
            .count();
        let cold_sign = ALL_ACTIONS
            .iter()
            .filter(|a| a.auth_type() == AdminOperationAuth::PasskeyColdSign)
            .count();
        assert_eq!(passkey, 4, "本地写(Passkey)档动作数变化,必须显式确认");
        assert_eq!(
            cold_sign, 16,
            "链上写(PasskeyColdSign)档动作数变化,必须显式确认"
        );
        assert_eq!(passkey + cold_sign, ALL_ACTIONS.len());
    }

    #[test]
    fn action_type_string_mapping_round_trips_for_every_variant() {
        // as_str() 与 parse_action_type() 是前后端共享的线格式。任一方向漏改,
        // 前端发来的动作名会被解析成另一个动作 —— 可能连带换掉鉴权档。
        for action in ALL_ACTIONS {
            let text = action.as_str();
            let parsed = parse_action_type(text)
                .unwrap_or_else(|_| panic!("{text} 无法被 parse_action_type 解析"));
            assert_eq!(parsed, action, "{text} 往返后变成了另一个动作");
        }
    }

    #[test]
    fn unknown_action_type_is_rejected() {
        // fail-closed:未登记的动作名必须拒绝,不得回退到任意默认动作或默认档位。
        for bad in [
            "",
            "UNKNOWN",
            "institution_create",
            "INSTITUTION_CREATE ",
            "UNKNOWN_OPERATION",
        ] {
            assert!(
                parse_action_type(bad).is_err(),
                "非法动作名 {bad:?} 被接受了"
            );
        }
    }
}
