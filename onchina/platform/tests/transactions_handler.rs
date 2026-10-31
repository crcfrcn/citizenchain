//! 随统一提交流程迁移的原有恢复与授权回归，夹具及预期保持不变。
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
        assert!(recheck_chain_submit_authorization(&state, &ctx, &session)
            .await
            .is_ok());
        ctx.account_id = "revoked".into();
        assert_eq!(
            recheck_chain_submit_authorization(&state, &ctx, &session)
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
        recheck_chain_submit_authorization(&state, &ctx, &unknown)
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
        recheck_chain_submit_authorization(&host, &ctx, &unknown)
            .await
            .unwrap_err()
            .status(),
        StatusCode::SERVICE_UNAVAILABLE
    );
    let builtin = chain_session_with_context(serde_json::json!({}));
    assert!(recheck_chain_submit_authorization(&host, &ctx, &builtin)
        .await
        .is_ok());
}
