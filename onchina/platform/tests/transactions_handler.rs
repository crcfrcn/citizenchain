//! 验证交易恢复阶段及首次提交和安全重提的同一授权。

use super::*;

use super::test_support::chain_session_with_context;

fn required_resume_phase(session: &ChainSignSession) -> ChainResumePhase {
    match chain_resume_phase(session) {
        Ok(phase) => phase,
        Err(error) => panic!("valid recovery fixture should decode: {error}"),
    }
}

#[test]
fn retry_phase_is_projection_only_after_finalized_marker() {
    let fresh = chain_session_with_context(serde_json::json!({}));
    assert_eq!(required_resume_phase(&fresh), ChainResumePhase::Submit);

    let submitted = chain_session_with_context(serde_json::json!({
        "submitted_tx_hash": format!("0x{}", "11".repeat(32)),
        "submitted_block_number": 8,
        "submitted_block_hash": format!("0x{}", "22".repeat(32)),
    }));
    assert!(matches!(
        required_resume_phase(&submitted),
        ChainResumePhase::Recover(_)
    ));
    let submitted_with_cursor = chain_session_with_context(serde_json::json!({
        "submitted_tx_hash": format!("0x{}", "11".repeat(32)),
        "submitted_block_number": 8,
        "submitted_block_hash": format!("0x{}", "22".repeat(32)),
        "recovery_block_number": 7,
        "recovery_block_hash": format!("0x{}", "44".repeat(32)),
    }));
    match required_resume_phase(&submitted_with_cursor) {
        ChainResumePhase::Recover(attempt) => {
            assert_eq!(attempt.recovery_block_number, Some(7));
            assert_eq!(attempt.recovery_block_hash, Some([0x44; 32]));
        }
        other => panic!("expected recovery phase, got {other:?}"),
    }

    let finalized = chain_session_with_context(serde_json::json!({
        "submitted_tx_hash": format!("0x{}", "11".repeat(32)),
        "submitted_block_number": 8,
        "submitted_block_hash": format!("0x{}", "22".repeat(32)),
        "finalized_tx_hash": format!("0x{}", "11".repeat(32)),
        "finalized_block_number": 9,
        "finalized_block_hash": format!("0x{}", "33".repeat(32)),
    }));
    assert!(matches!(
        required_resume_phase(&finalized),
        ChainResumePhase::Project(_)
    ));

    let damaged = chain_session_with_context(serde_json::json!({"submitted_tx_hash": "0x11"}));
    assert!(chain_resume_phase(&damaged).is_err());

    let damaged_cursor = chain_session_with_context(serde_json::json!({
        "submitted_tx_hash": format!("0x{}", "11".repeat(32)),
        "submitted_block_number": 8,
        "submitted_block_hash": format!("0x{}", "22".repeat(32)),
        "recovery_block_number": 7,
    }));
    assert!(chain_resume_phase(&damaged_cursor).is_err());
}

// 两个提交阶段都调用同一所属模块回调；未登记业务和卸载模块必须拒绝。
#[tokio::test]
async fn first_submit_and_safe_resubmit_share_registered_authorization() {
    use crate::modules::model::{ModuleBusiness, ModuleDescription, ModuleRoute};
    use std::sync::atomic::{AtomicUsize, Ordering};
    struct CheckedModule {
        calls: Arc<AtomicUsize>,
    }
    #[async_trait::async_trait]
    impl ModuleBusiness for CheckedModule {
        fn describe(&self) -> ModuleDescription {
            let mut description = crate::modules::test_support::fixture_description();
            description.purposes = vec!["FIXTURE_SUBMIT".into()];
            description
        }
        fn routes(&self) -> Vec<ModuleRoute> {
            Vec::new()
        }
        async fn recheck_submit(
            &self,
            _state: &AppState,
            ctx: &AdminAuthContext,
            _session: &ChainSignSession,
        ) -> Result<(), axum::response::Response> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            if ctx.account_id == "revoked" {
                return Err(api_error(StatusCode::FORBIDDEN, 1003, "permission denied"));
            }
            Ok(())
        }
    }
    let calls = Arc::new(AtomicUsize::new(0));
    let state = AppState {
        db: Db::empty_for_route_test(),
        rate_limiter: Arc::new(LocalRateLimiter::new()),
        modules: Arc::new(crate::modules::ModuleCatalog::new(Some(Arc::new(
            CheckedModule {
                calls: calls.clone(),
            },
        )))),
    };
    let mut ctx = AdminAuthContext {
        binding_revision: "1".into(),
        person_cid: crate::testing::person_cid(),
        role_code: "REGISTRAR".into(),
        account_id: "active".into(),
        institution_cid_number: "fixture".into(),
        institution_code: "FRG".into(),
        admin_level: None,
        family_name: String::new(),
        given_name: String::new(),
        scope_province_name: None,
        scope_city_name: None,
        scope_town_name: None,
        cid_short_name: None,
    };
    for context in [
        serde_json::json!({}),
        serde_json::json!({"submitted_tx_hash":format!("0x{}","11".repeat(32)),"submitted_block_number":8,"submitted_block_hash":format!("0x{}","22".repeat(32))}),
    ] {
        let mut session = chain_session_with_context(context);
        session.purpose = "FIXTURE_SUBMIT".into();
        assert!(
            recheck_business_submit_authorization(&state, &ctx, &session)
                .await
                .is_ok()
        );
        ctx.account_id = "revoked".into();
        assert_eq!(
            recheck_business_submit_authorization(&state, &ctx, &session)
                .await
                .unwrap_err()
                .status(),
            StatusCode::FORBIDDEN
        );
        ctx.account_id = "active".into();
    }
    assert_eq!(calls.load(Ordering::SeqCst), 4);
    let mut unknown = chain_session_with_context(serde_json::json!({}));
    unknown.purpose = "MISSING_MODULE".into();
    assert_eq!(
        recheck_business_submit_authorization(&state, &ctx, &unknown)
            .await
            .unwrap_err()
            .status(),
        StatusCode::SERVICE_UNAVAILABLE
    );
    let host = AppState {
        modules: Arc::new(crate::modules::ModuleCatalog::new(None)),
        ..state
    };
    assert_eq!(
        recheck_business_submit_authorization(&host, &ctx, &unknown)
            .await
            .unwrap_err()
            .status(),
        StatusCode::SERVICE_UNAVAILABLE
    );
    let mut builtin = chain_session_with_context(serde_json::json!({}));
    builtin.purpose = crate::institutions::PURPOSE_INSTITUTION_GOVERNANCE.into();
    builtin.call_data = vec![30, 8];
    *host.db.role_snapshot.lock().unwrap() = Some(ctx.clone());
    assert_eq!(
        recheck_business_submit_authorization(&host, &ctx, &builtin)
            .await
            .unwrap_err()
            .status(),
        StatusCode::FORBIDDEN
    );
    let permission = entitybase::RoleBusinessPermission {
        role_subject: entitybase::RoleSubject {
            cid_number: ctx.institution_cid_number.as_bytes().to_vec(),
            role_code: ctx.role_code.as_bytes().to_vec(),
        },
        business_action_id: entitybase::BusinessActionId {
            module_tag: b"pub-mgmt".to_vec(),
            action_code: 3,
        },
        operation: entitybase::RolePermissionOperation::Propose,
    };
    host.db.role_permissions.lock().unwrap().push(permission);
    assert!(recheck_business_submit_authorization(&host, &ctx, &builtin)
        .await
        .is_ok());
    builtin.call_data = vec![30, 11];
    assert!(recheck_business_submit_authorization(&host, &ctx, &builtin)
        .await
        .is_ok());
    builtin.call_data = vec![31, 11];
    assert_eq!(
        recheck_business_submit_authorization(&host, &ctx, &builtin)
            .await
            .unwrap_err()
            .status(),
        StatusCode::FORBIDDEN
    );
    builtin.call_data = vec![30, 11];
    host.db.role_permissions.lock().unwrap()[0]
        .role_subject
        .role_code = b"OTHER".to_vec();
    assert_eq!(
        recheck_business_submit_authorization(&host, &ctx, &builtin)
            .await
            .unwrap_err()
            .status(),
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn stale_signing_revision_is_rejected_before_any_business_or_chain_call() {
    let state = AppState {
        db: Db::empty_for_route_test(),
        rate_limiter: Arc::new(LocalRateLimiter::new()),
        modules: Arc::new(crate::modules::ModuleCatalog::new(None)),
    };
    let ctx = AdminAuthContext {
        binding_revision: "2".into(),
        person_cid: crate::testing::person_cid(),
        role_code: "REGISTRAR".into(),
        account_id: "same-address".into(),
        institution_cid_number: "original".into(),
        institution_code: "FRG".into(),
        admin_level: None,
        family_name: String::new(),
        given_name: String::new(),
        scope_province_name: None,
        scope_city_name: None,
        scope_town_name: None,
        cid_short_name: None,
    };
    for context in [
        serde_json::json!({}),
        serde_json::json!({"origin_binding_revision":"0","origin_person_cid":"different"}),
        serde_json::json!({"origin_binding_revision":"2","origin_role_code":"OTHER"}),
    ] {
        let session = chain_session_with_context(context);
        assert_eq!(
            recheck_chain_submit_authorization(&state, &ctx, &session)
                .await
                .unwrap_err()
                .status(),
            StatusCode::CONFLICT
        );
    }
}

#[test]
fn institution_write_allowlist_requires_exact_current_encoding_and_actor() {
    use codec::Encode;
    for pallet in [30, 31] {
        // 退役钱包操作call 10始终拒绝，空治理动作也不能通过写入门。
        assert!(validate_institution_write(&[pallet, 10, 0]).is_err());
        let empty = entitybase::InstitutionGovernanceAction {
            role_mutations: vec![],
            assignment_changes: vec![],
            legal_representative_change: None,
        };
        let mut call = vec![pallet, 8];
        call.extend((b"CID".to_vec(), empty, b"ACTOR".to_vec(), b"ROLE".to_vec()).encode());
        assert!(validate_institution_write(&call).is_err());
        let action = entitybase::InstitutionRoleRegistration {
            role_mutations: vec![],
            assignment_changes: vec![entitybase::InstitutionRoleAssignmentChange {
                role_code: b"R1".to_vec(),
                assignments: vec![],
            }],
        };
        let mut call = vec![pallet, 9];
        call.extend(
            (
                b"CID".to_vec(),
                action,
                b"ACTOR".to_vec(),
                b"ROLE".to_vec(),
                [0u8; 32],
                Some(2u32),
            )
                .encode(),
        );
        assert_eq!(
            validate_institution_write(&call).unwrap(),
            Some((b"ACTOR".to_vec(), b"ROLE".to_vec()))
        );
        call.push(0);
        assert!(validate_institution_write(&call).is_err());
    }
    assert_eq!(validate_institution_write(&[48, 9, 0]).unwrap(), None);
}

#[test]
fn policy_write_uses_exact_cid_role_and_rejects_empty_or_trailing_payloads() {
    use codec::Encode;
    let mut action = entitybase::InstitutionPolicyAction {
        permission_changes: vec![],
        threshold: Some(2),
    };
    let encode = |action: &entitybase::InstitutionPolicyAction| {
        let mut call = vec![30, 11];
        call.extend(
            (
                b"institution".to_vec(),
                action,
                b"R_GOV".to_vec(),
                [7u8; 32],
            )
                .encode(),
        );
        call
    };
    let call = encode(&action);
    assert_eq!(
        validate_institution_write(&call).unwrap(),
        Some((b"institution".to_vec(), b"R_GOV".to_vec()))
    );
    let mut trailing = call.clone();
    trailing.push(0);
    assert!(validate_institution_write(&trailing).is_err());
    assert!(validate_institution_write(&call[..call.len() - 1]).is_err());
    action.threshold = None;
    assert!(validate_institution_write(&encode(&action)).is_err());
    action.threshold = Some(0);
    assert!(validate_institution_write(&encode(&action)).is_err());
}
