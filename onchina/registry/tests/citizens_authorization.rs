use super::*;

fn roles() -> Vec<OperationRole> {
    vec![
        OperationRole {
            role_code: "ROLE_A".into(),
            role_name: "身份登记员".into(),
        },
        OperationRole {
            role_code: "ROLE_B".into(),
            role_name: "登记负责人".into(),
        },
    ]
}

#[test]
fn unique_role_is_automatic_and_multiple_require_a_valid_selection() {
    let options = roles();
    assert_eq!(choose_role(&options[..1], None).unwrap(), "ROLE_A");
    assert_eq!(choose_role(&options, Some("ROLE_B")).unwrap(), "ROLE_B");
    assert_eq!(
        choose_role(&options, None).unwrap_err().status(),
        StatusCode::BAD_REQUEST
    );
    for selected in ["", "FOREIGN_ROLE", " ROLE_A "] {
        assert_eq!(
            choose_role(&options, Some(selected)).unwrap_err().status(),
            StatusCode::FORBIDDEN
        );
    }
    assert_eq!(
        choose_role(&[], None).unwrap_err().status(),
        StatusCode::FORBIDDEN
    );
    // 曾选岗位被撤销后，不能自动切换到剩下的唯一岗位继续原签名。
    assert!(choose_role(&options[..1], Some("ROLE_B")).is_err());
}

#[test]
fn each_operation_uses_its_own_business_permission() {
    for (operation, expected) in [
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
fn optional_prepare_role_and_required_frozen_complete_role() {
    let _: super::super::chain_identity::PrepareCitizenOnchainInput =
        serde_json::from_value(serde_json::json!({
            "accountId": "wallet", "identityLevel": "voting"
        }))
        .unwrap();
    let _: super::super::occupy::PrepareCitizenRebindInput =
        serde_json::from_value(serde_json::json!({
            "cidNumber": "cid"
        }))
        .unwrap();
    let _: super::super::occupy::PrepareCitizenRevokeInput =
        serde_json::from_value(serde_json::json!({})).unwrap();
    assert!(
        serde_json::from_value::<super::super::chain_identity::CompleteCitizenOnchainInput>(
            serde_json::json!({
                "accountId": "wallet", "identityLevel": "voting", "signResponse": "signature"
            })
        )
        .is_err()
    );
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
