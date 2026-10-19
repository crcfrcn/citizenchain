//! 随统一提交流程迁移的原有恢复与授权回归，夹具及预期保持不变。
use super::*;
use chrono::Duration;
use crate::registry::citizens::occupy::{CITIZEN_IDENTITY_PALLET_INDEX, OCCUPY_CID_CALL_INDEX};

fn chain_session_with_context(context: serde_json::Value) -> ChainSignSession {
        ChainSignSession {
            request_id: "request-1".to_string(),
            purpose: PURPOSE_CITIZEN_OCCUPY.to_string(),
            account_id: "0x1111111111111111111111111111111111111111111111111111111111111111"
                .to_string(),
            call_data: vec![CITIZEN_IDENTITY_PALLET_INDEX, OCCUPY_CID_CALL_INDEX],
            nonce: 3,
            signing_hash: "aa".repeat(32),
            context,
            expires_at: Utc::now() + Duration::minutes(10),
            consumed_at: None,
        }
    }

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
            "submitted_from_block_number": 8,
            "submitted_from_block_hash": format!("0x{}", "22".repeat(32)),
        }));
        assert!(matches!(
            required_resume_phase(&submitted),
            ChainResumePhase::Recover(_)
        ));
        let submitted_with_cursor = chain_session_with_context(serde_json::json!({
            "submitted_tx_hash": format!("0x{}", "11".repeat(32)),
            "submitted_from_block_number": 8,
            "submitted_from_block_hash": format!("0x{}", "22".repeat(32)),
            "recovery_cursor_block_number": 7,
            "recovery_cursor_block_hash": format!("0x{}", "44".repeat(32)),
        }));
        match required_resume_phase(&submitted_with_cursor) {
            ChainResumePhase::Recover(attempt) => {
                assert_eq!(attempt.recovery_cursor_block_number, Some(7));
                assert_eq!(attempt.recovery_cursor_block_hash, Some([0x44; 32]));
            }
            other => panic!("expected recovery phase, got {other:?}"),
        }

        let finalized = chain_session_with_context(serde_json::json!({
            "submitted_tx_hash": format!("0x{}", "11".repeat(32)),
            "submitted_from_block_number": 8,
            "submitted_from_block_hash": format!("0x{}", "22".repeat(32)),
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
            "submitted_from_block_number": 8,
            "submitted_from_block_hash": format!("0x{}", "22".repeat(32)),
            "recovery_cursor_block_number": 7,
        }));
        assert!(chain_resume_phase(&damaged_cursor).is_err());
    }

#[test]
    fn first_submit_and_safe_resubmit_share_authorization_policy() {
        assert_eq!(
            chain_authorization_policy(PURPOSE_CITIZEN_OCCUPY),
            ChainAuthorizationPolicy::Registry
        );
        assert_eq!(
            chain_authorization_policy(PURPOSE_CITIZEN_ADMIN_REBIND),
            ChainAuthorizationPolicy::Registry
        );
        assert_eq!(
            chain_authorization_policy(crate::private::membership::PURPOSE_PLATFORM_PRICE_PROPOSAL),
            ChainAuthorizationPolicy::Platform
        );
        assert_eq!(
            chain_authorization_policy(
                crate::legislation::law::action::PURPOSE_LEGISLATION_PROPOSE
            ),
            ChainAuthorizationPolicy::Legislation
        );
        assert_eq!(
            chain_authorization_policy(crate::institutions::admins::PURPOSE_INSTITUTION_GOVERNANCE),
            ChainAuthorizationPolicy::RuntimeOnly
        );
    }


#[tokio::test]
#[ignore = "由控制台交付本机 onchina_test 数据库后执行隔离 PostgreSQL 集成回归"]
async fn recovery_lease_and_projection_commit_survive_revocation_failure_and_retry() {
    use crate::authentication::{repo, login::NodeInstitutionBinding};
    use crate::core::chain_citizen_identity::{FinalizedCitizenIdentity, FinalizedCidStatus};
    let sandbox=crate::core::db::IsolatedTestDb::new();
    let owner=format!("GZ018-CREG{}-000000001-2026",primitives::cid::number::checksum_char_m1("GZ018CREG0000000012026",false));
    let citizen=format!("CN000-CTZN{}-100000000-2026",primitives::cid::number::checksum_char_m1("CN000CTZN1000000002026",true));
    let binding=NodeInstitutionBinding { binding_id:"original".into(),candidate_id:"one-institution".into(),institution_code:"CREG".into(),institution_cid_number:owner.clone(),bound_account_id:format!("0x{}","11".repeat(32)),bound_at:Utc::now(),status:"ACTIVE".into() };
    sandbox.db.with_client(|conn| repo::upsert_active_node_binding_conn(conn,&binding)).unwrap();
    let mut session=chain_session_with_context(serde_json::json!({
        "origin_binding_id":binding.binding_id,"origin_institution_cid_number":owner,"origin_institution_code":"CREG",
        "cid_number":citizen,"finalized_tx_hash":format!("0x{}","55".repeat(32)),"finalized_block_number":7,"finalized_block_hash":format!("0x{}","22".repeat(32)),
        "validated":{"cid_type":"CTZN","family_name":"","given_name":"","citizen_sex":"","citizen_birth_date":"","province_name":"贵州省","city_name":"绥阳市","province_code":"GZ","city_code":"018","town_code":"","birth_province_code":"","birth_city_code":"","birth_town_code":"","voting_eligible":false}
    }));
    session.expires_at=Utc::now()-Duration::hours(1); // 已提交事务恢复不受签名会话到期阻断。
    sandbox.db.with_client(|conn| {
        conn.execute("INSERT INTO chain_sign_sessions(request_id,purpose,account_id,call_data,nonce,signing_hash,context,expires_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8)", &[&session.request_id,&session.purpose,&session.account_id,&hex::encode(&session.call_data),&i64::from(session.nonce),&session.signing_hash,&session.context,&session.expires_at]).unwrap();
        // 没有当前管理员行，恢复依然只按已确认会话的原机构完成，不能用于发新交易。
        assert_eq!(conn.query_one("SELECT COUNT(*) FROM admins",&[]).unwrap().get::<_,i64>(0),0);
        Ok(())
    }).unwrap();
    let lease=SessionLease::acquire(&sandbox.db,&session.request_id).unwrap().unwrap();
    assert!(SessionLease::acquire(&sandbox.db,&session.request_id).unwrap().is_none());
    let snapshot=FinalizedCitizenIdentity { genesis_hash:[1;32],finalized_block_hash:[0x22;32],finalized_block_number:7,chain_now_seconds:1_700_000_000,cid_status:FinalizedCidStatus::Active,
        registrar_cid_number:Some(binding.institution_cid_number.as_bytes().to_vec()),commitment:Some([3;32]),residence_province_code:b"GZ".to_vec(),residence_city_code:b"018".to_vec(),account_id:Some([0x33;32]),binding_revision:Some(1),identity_version:0,voting:None,candidate:None };
    assert!(commit_finalized_projection(&sandbox.db,&session,&lease,"different-finalized-hash",Some(&snapshot)).is_err());
    assert!(sandbox.db.find_citizen_by_cid(&citizen).unwrap().is_none()); // 消费失败须回滚已写的公民/ID。
    let hash=format!("0x{}","55".repeat(32));
    let output=commit_finalized_projection(&sandbox.db,&session,&lease,&hash,Some(&snapshot)).unwrap().unwrap();
    assert!(output.id>0);
    assert!(sandbox.db.find_chain_sign_session(&session.request_id).unwrap().unwrap().consumed_at.is_some());
    assert!(commit_finalized_projection(&sandbox.db,&session,&lease,&hash,Some(&snapshot)).is_err());
    assert_eq!(sandbox.db.find_citizen_by_cid(&citizen).unwrap().unwrap().id,output.id);
    assert!(origin_matches_binding(&session,&binding));
    let other=NodeInstitutionBinding { institution_cid_number:"different-institution".into(),..binding.clone() };
    assert!(!origin_matches_binding(&session,&other));
    let fresh=chain_session_with_context(serde_json::json!({}));
    assert!(!origin_matches_binding(&fresh,&binding));
}
