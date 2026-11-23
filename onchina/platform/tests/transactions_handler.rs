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
        wallet_revision: "0".into(),
        wallet_namespace: crate::chain_runtime::AdminPallet::PublicAdmins,
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
    let builtin = chain_session_with_context(serde_json::json!({}));
    assert!(recheck_business_submit_authorization(&host, &ctx, &builtin)
        .await
        .is_ok());
}

#[tokio::test]
async fn stale_signing_revision_is_rejected_before_any_business_or_chain_call() {
    let state = AppState {
        db: Db::empty_for_route_test(),
        rate_limiter: Arc::new(LocalRateLimiter::new()),
        modules: Arc::new(crate::modules::ModuleCatalog::new(None)),
    };
    let ctx = AdminAuthContext {
        wallet_revision: "2".into(),
        wallet_namespace: crate::chain_runtime::AdminPallet::PublicAdmins,
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
        serde_json::json!({"origin_wallet_revision":"0","origin_wallet_namespace":"PublicAdmins"}),
        serde_json::json!({"origin_wallet_revision":"2","origin_wallet_namespace":"PrivateAdmins"}),
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
fn institution_write_allowlist_rejects_old_actions_without_rewriting_signatures() {
    use codec::Encode;
    let old = [
        entitybase::InstitutionGovernanceAction::<[u8; 32]>::ReplaceAdmins { admins: vec![] },
        entitybase::InstitutionGovernanceAction::ReplaceAdminsAndMutateRoles {
            admins: vec![],
            role_mutations: vec![],
            assignment_changes: vec![],
            legal_representative_change: None,
        },
    ];
    for pallet in [30, 31] {
        for action in old.clone() {
            let mut call = vec![pallet, 8];
            call.extend(
                (
                    b"CID".to_vec(),
                    action.clone(),
                    b"CID".to_vec(),
                    b"ROLE".to_vec(),
                )
                    .encode(),
            );
            assert!(validate_institution_write(&call).is_err());
            let mut call = vec![pallet, 9];
            call.extend(
                (
                    b"CID".to_vec(),
                    action,
                    b"REGISTRY".to_vec(),
                    b"ROLE".to_vec(),
                    [0u8; 32],
                    None::<u32>,
                )
                    .encode(),
            );
            assert!(validate_institution_write(&call).is_err());
        }
    }
    let action = entitybase::InstitutionGovernanceAction::<[u8; 32]>::MutateRolesAndAssignments {
        role_mutations: vec![],
        assignment_changes: vec![],
        legal_representative_change: None,
    };
    let mut call = vec![31, 9];
    call.extend(
        (
            b"CID".to_vec(),
            action,
            b"REGISTRY".to_vec(),
            b"ROLE".to_vec(),
            [0u8; 32],
            Some(2u32),
        )
            .encode(),
    );
    assert!(validate_institution_write(&call).is_ok());
    call.push(0);
    assert!(validate_institution_write(&call).is_err());
    assert!(validate_institution_write(&[48, 9, 0]).is_ok()); // 个人身份模块不进入机构动作门。
}
