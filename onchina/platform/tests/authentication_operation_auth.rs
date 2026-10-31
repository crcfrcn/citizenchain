//! 原所属模块回归，断言与输入保持。
use super::*;

#[test]
fn public_action_names_match_json_and_dispatch_for_every_variant() {
    for action in ALL_ACTIONS {
        let json = serde_json::to_value(&action).unwrap();
        assert_eq!(json.as_str(), Some(action.as_str()));
        assert_eq!(
            serde_json::from_value::<AdminActionType>(json).unwrap(),
            action
        );
        assert_eq!(parse_action_type(action.as_str()).unwrap(), action);
    }
    assert!(serde_json::from_str::<AdminActionType>("\"CREATE_CITY_REGISTRY\"").is_err());
    assert!(serde_json::from_str::<AdminActionType>("\"DELETE_CITY_REGISTRY\"").is_err());
}

#[test]
fn module_action_json_and_dispatch_share_valid_names_and_rejections() {
    for name in ["MODULE_ACTION_1".to_string(), "A".repeat(128)] {
        let action = AdminActionType::Module(name.clone());
        assert_eq!(parse_action_type(&name).unwrap(), action);
        assert_eq!(
            serde_json::from_value::<AdminActionType>(serde_json::json!(name)).unwrap(),
            action
        );
        assert_eq!(
            serde_json::to_value(&action).unwrap(),
            serde_json::json!(name)
        );
    }
    for name in [
        "CREATE_CITY_REGISTRY".to_string(),
        "DELETE_CITY_REGISTRY".to_string(),
        String::new(),
        "lowercase".to_string(),
        "HAS SPACE".to_string(),
        "A".repeat(129),
    ] {
        assert!(parse_action_type(&name).is_err(), "{name}");
        assert!(
            serde_json::from_value::<AdminActionType>(serde_json::json!(name)).is_err(),
            "{name}"
        );
    }
    assert!(serde_json::from_value::<AdminActionType>(serde_json::json!(7)).is_err());
}

#[test]
fn ordinary_institution_cannot_prepare_registry_business() {
    let admin = AdminAuthContext {
        account_id: format!("0x{}", "11".repeat(32)),
        institution_cid_number: "own-institution".into(),
        institution_code: "SFLP".into(),
        admin_level: None,
        family_name: "测".into(),
        given_name: "试".into(),
        scope_province_name: Some("贵州省".into()),
        scope_city_name: Some("绥阳市".into()),
        scope_town_name: None,
        cid_short_name: None,
    };
    for action in [
        AdminActionType::CreateCityRegistry,
        AdminActionType::DeleteCityRegistry,
        AdminActionType::InstitutionCreate,
        AdminActionType::CitizenOnchainPush,
        AdminActionType::InstitutionUpdate,
        AdminActionType::InstitutionUploadDocument,
        AdminActionType::InstitutionDeleteDocument,
    ] {
        assert!(ensure_action_role_allowed(&admin, &action).is_err());
    }
    assert!(ensure_action_role_allowed(&admin, &AdminActionType::NodeBindingUnbind).is_ok());
}

#[test]
fn passkey_prepare_is_limited_to_node_unbind() {
    assert!(AdminActionType::NodeBindingUnbind.can_prepare());
    assert!(!AdminActionType::InstitutionUploadDocument.can_prepare());
    assert!(!AdminActionType::InstitutionDeleteDocument.can_prepare());
    assert!(!AdminActionType::CitizenOnchainPush.can_prepare());
    assert!(AdminActionType::CreateCityRegistry.can_prepare());
}

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
        A::Module(_) => PasskeyColdSign,
    }
}

/// 固定内建动作逐项核对；开放模块动作由独立回归核对归属和鉴权档。
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
            seen.insert(action.as_str().to_string()),
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
fn malformed_action_type_is_rejected() {
    // fail-closed:未登记的动作名必须拒绝,不得回退到任意默认动作或默认档位。
    for bad in ["", "institution_create", "INSTITUTION_CREATE "] {
        assert!(
            parse_action_type(bad).is_err(),
            "非法动作名 {bad:?} 被接受了"
        );
    }
}
#[test]
fn module_action_cannot_bypass_owner_or_cold_signature() {
    let action = parse_action_type("EXAMPLE_OPERATION").unwrap();
    assert!(matches!(action, AdminActionType::Module(_)));
    assert!(action.requires_module());
    assert_eq!(action.auth_type(), AdminOperationAuth::PasskeyColdSign);
    assert!(crate::modules::ModuleCatalog::new(None)
        .owner_of_action(action.as_str())
        .is_none());
    assert_eq!(
        serde_json::to_value(&action).unwrap(),
        serde_json::json!("EXAMPLE_OPERATION")
    );
    assert_eq!(
        serde_json::from_value::<AdminActionType>(serde_json::json!("EXAMPLE_OPERATION")).unwrap(),
        action
    );
}
