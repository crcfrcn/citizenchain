use super::*;

#[test]
fn automatic_role_is_stable_and_frozen_role_never_switches() {
    let roles = vec!["ROLE_B".to_string(), "ROLE_A".to_string()];
    assert_eq!(choose_role(&roles[..1], None).unwrap(), "ROLE_B");
    assert_eq!(choose_role(&roles, None).unwrap(), "ROLE_A");
    assert_eq!(
        choose_role(&[roles[1].clone(), roles[0].clone()], None).unwrap(),
        "ROLE_A"
    );
    assert_eq!(choose_role(&roles, Some("ROLE_B")).unwrap(), "ROLE_B");
    assert_eq!(
        choose_role(&[], None).unwrap_err().status(),
        StatusCode::FORBIDDEN
    );
    for frozen in ["", "FOREIGN_ROLE", " ROLE_A "] {
        assert_eq!(
            choose_role(&roles, Some(frozen)).unwrap_err().status(),
            StatusCode::FORBIDDEN
        );
    }
    // 固定的ROLE_B被撤销时，即使ROLE_A仍有效也不能切换继续原签名。
    assert_eq!(
        choose_role(&roles[1..], Some("ROLE_B"))
            .unwrap_err()
            .status(),
        StatusCode::FORBIDDEN
    );
}

#[test]
fn each_operation_uses_its_own_business_permission() {
    for (operation, expected) in [
        (
            CitizenOperation::OccupyCid,
            business_action::ACTION_OCCUPY_CID,
        ),
        (
            CitizenOperation::RegisterVotingIdentity,
            business_action::ACTION_REGISTER_VOTING_IDENTITY,
        ),
        (
            CitizenOperation::UpgradeCandidateIdentity,
            business_action::ACTION_UPGRADE_CANDIDATE_IDENTITY,
        ),
        (
            CitizenOperation::RebindWallet,
            business_action::ACTION_ADMIN_REBIND_CID_ACCOUNT_ID,
        ),
        (
            CitizenOperation::RevokeIdentity,
            business_action::ACTION_REVOKE_CID,
        ),
    ] {
        assert_eq!(
            operation.action().module_tag,
            business_action::MODULE_CITIZEN_IDENTITY
        );
        assert_eq!(operation.action().action_code, expected);
    }
}

#[test]
fn frg_named_identity_uses_exact_province_role_but_anonymous_has_no_region() {
    let registry = primitives::governance_skeleton::federal_registry_institution();
    let cid = registry.cid_number.as_bytes();
    let role = primitives::governance_skeleton::province_commissioner_role_code(*b"GD");
    assert!(role_in_scope(cid, &role, None));
    assert!(role_in_scope(cid, &role, Some((b"GD", b"001"))));
    assert!(!role_in_scope(cid, &role, Some((b"GZ", b"001"))));
    assert!(!role_in_scope(cid, &role, Some((b"", b"001"))));
    assert!(!role_in_scope(cid, b"LR", Some((b"GD", b"001"))));
}

#[test]
fn public_requests_never_accept_an_actor_role() {
    use super::super::{
        admin_entry::AdminCreateCitizenInput,
        chain_identity::{CompleteCitizenOnchainInput, PrepareCitizenOnchainInput},
        occupy::{PrepareCitizenRebindInput, PrepareCitizenRevokeInput},
    };
    macro_rules! check {
        ($kind:ty, $value:expr) => {{
            let mut value = $value;
            assert!(serde_json::from_value::<$kind>(value.clone()).is_ok());
            value["actorRoleCode"] = serde_json::json!("FORGED_ROLE");
            assert!(serde_json::from_value::<$kind>(value).is_err());
        }};
    }
    check!(
        AdminCreateCitizenInput,
        serde_json::json!({"cidType":"CTZN"})
    );
    check!(
        PrepareCitizenOnchainInput,
        serde_json::json!({"accountId":"wallet","identityLevel":"voting"})
    );
    check!(
        CompleteCitizenOnchainInput,
        serde_json::json!({"accountId":"wallet","identityLevel":"voting","signResponse":"signature"})
    );
    check!(
        PrepareCitizenRebindInput,
        serde_json::json!({"cidNumber":"cid"})
    );
    check!(PrepareCitizenRevokeInput, serde_json::json!({}));
}

#[test]
fn city_registry_scope_cannot_cross_city_or_province() {
    let cid = format!(
        "GZ018-CREG{}-000000001-2026",
        primitives::cid::number::checksum_char_m1("GZ018CREG0000000012026", false)
    );
    assert!(role_in_scope(
        cid.as_bytes(),
        b"REGISTRAR",
        Some((b"GZ", b"018"))
    ));
    assert!(!role_in_scope(
        cid.as_bytes(),
        b"REGISTRAR",
        Some((b"GZ", b"019"))
    ));
    assert!(!role_in_scope(
        cid.as_bytes(),
        b"REGISTRAR",
        Some((b"GD", b"018"))
    ));
    assert!(role_in_scope(cid.as_bytes(), b"REGISTRAR", None));
}

#[tokio::test]
async fn each_automatic_prepare_keeps_the_real_session_gate() {
    use axum::{body::Body, http::Request};
    use tower::ServiceExt;
    let app = onchina::testing::router_for(std::sync::Arc::new(crate::module::Module::default()));
    for (path, body) in [
        ("/api/admin/citizens", serde_json::json!({"cidType":"CTZN"})),
        (
            "/api/admin/citizens/rebind/prepare",
            serde_json::json!({"cidNumber":"fixture"}),
        ),
        (
            "/api/admin/citizens/fixture/onchain/revoke/prepare",
            serde_json::json!({}),
        ),
        (
            "/api/admin/citizens/fixture/onchain/prepare",
            serde_json::json!({"accountId":"wallet","identityLevel":"voting"}),
        ),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(path)
                    .header("content-type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "{path}");
    }
}

#[test]
fn identity_levels_allow_first_registration_and_only_voting_to_candidate_upgrade() {
    for operation in [
        CitizenOperation::RegisterVotingIdentity,
        CitizenOperation::UpgradeCandidateIdentity,
    ] {
        assert!(ensure_identity_transition(operation, false, false).is_ok());
    }
    assert!(
        ensure_identity_transition(CitizenOperation::UpgradeCandidateIdentity, true, false).is_ok()
    );
    assert_eq!(
        ensure_identity_transition(CitizenOperation::RegisterVotingIdentity, true, false)
            .unwrap_err()
            .status(),
        StatusCode::CONFLICT
    );
    // 竞选同时具有投票记录，必须优先锁定竞选；即使收到异常缺少投票记录的快照也不能降级。
    for voting in [false, true] {
        for operation in [
            CitizenOperation::RegisterVotingIdentity,
            CitizenOperation::UpgradeCandidateIdentity,
        ] {
            assert_eq!(
                ensure_identity_transition(operation, voting, true)
                    .unwrap_err()
                    .status(),
                StatusCode::CONFLICT
            );
        }
    }
}

#[test]
fn identity_level_lock_does_not_block_wallet_rebind_or_legal_revocation() {
    for operation in [
        CitizenOperation::OccupyCid,
        CitizenOperation::RebindWallet,
        CitizenOperation::RevokeIdentity,
    ] {
        assert!(ensure_identity_transition(operation, true, true).is_ok());
    }
}
