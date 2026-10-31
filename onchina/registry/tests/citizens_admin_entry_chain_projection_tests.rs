//! 原所属模块回归，断言与输入保持。
use super::*;
use crate::citizens::chain_identity::IdentityOperations;
use chrono::Duration;
use onchina::transactions::handler::origin_matches_binding;
use onchina::transactions::handler::test_support::chain_session_with_context;
use onchina::transactions::session::SessionLease;
#[tokio::test]
#[ignore = "由控制台交付本机 onchina_test 数据库后执行隔离 PostgreSQL 集成回归"]
async fn recovery_lease_and_projection_commit_survive_revocation_failure_and_retry() {
    use onchina::authentication::{login::NodeInstitutionBinding, repo};
    use onchina::chain_identity::{FinalizedCidStatus, FinalizedCitizenIdentity};
    let sandbox = onchina::database::IsolatedTestDb::new();
    let owner = format!(
        "GZ018-CREG{}-000000001-2026",
        primitives::cid::number::checksum_char_m1("GZ018CREG0000000012026", false)
    );
    let citizen = format!(
        "CN000-CTZN{}-100000000-2026",
        primitives::cid::number::checksum_char_m1("CN000CTZN1000000002026", true)
    );
    let binding = NodeInstitutionBinding {
        binding_id: "original".into(),
        candidate_id: "one-institution".into(),
        institution_code: "CREG".into(),
        institution_cid_number: owner.clone(),
        bound_account_id: format!("0x{}", "11".repeat(32)),
        bound_at: Utc::now(),
        status: "ACTIVE".into(),
    };
    sandbox
        .db
        .with_client(|conn| repo::upsert_active_node_binding_conn(conn, &binding))
        .unwrap();
    let mut session = chain_session_with_context(serde_json::json!({
        "origin_binding_id":binding.binding_id,"origin_institution_cid":owner,"origin_institution_code":"CREG",
        "cid_number":citizen,"finalized_tx_hash":format!("0x{}","55".repeat(32)),"finalized_block_number":7,"finalized_block_hash":format!("0x{}","22".repeat(32)),
        "validated":{"cidType":"CTZN","familyName":"","givenName":"","citizenSex":"","citizenBirthDate":"","provinceName":"贵州省","cityName":"绥阳市","provinceCode":"GZ","cityCode":"018","townCode":"","birthProvinceCode":"","birthCityCode":"","birthTownCode":"","votingEligible":false}
    }));
    session.purpose = crate::citizens::occupy::PURPOSE_CITIZEN_OCCUPY.into();
    session.call_data = vec![
        crate::citizens::occupy::CITIZEN_IDENTITY_PALLET_INDEX,
        crate::citizens::occupy::OCCUPY_CID_CALL_INDEX,
    ];
    session.expires_at = Utc::now() - Duration::hours(1); // 已提交事务恢复不受签名会话到期阻断。
    sandbox.db.with_client(|conn| {
        conn.execute("INSERT INTO signing(request_id,sign_purpose,account_id,call_data,nonce_value,signing_hash,context_json,expires_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8)", &[&session.request_id,&session.purpose,&session.account_id,&hex::encode(&session.call_data),&i64::from(session.nonce),&session.signing_hash,&session.context,&session.expires_at]).unwrap();
        // 没有当前管理员行，恢复依然只按已确认会话的原机构完成，不能用于发新交易。
        assert_eq!(conn.query_one("SELECT COUNT(*) FROM admin",&[]).unwrap().get::<_,i64>(0),0);
        Ok(())
    }).unwrap();
    let lease = SessionLease::acquire(&sandbox.db, &session.request_id)
        .unwrap()
        .unwrap();
    assert!(SessionLease::acquire(&sandbox.db, &session.request_id)
        .unwrap()
        .is_none());
    let snapshot = FinalizedCitizenIdentity {
        genesis_hash: [1; 32],
        finalized_block_hash: [0x22; 32],
        finalized_block_number: 7,
        chain_now_seconds: 1_700_000_000,
        cid_status: FinalizedCidStatus::Active,
        registrar_cid_number: Some(binding.institution_cid_number.as_bytes().to_vec()),
        commitment: Some([3; 32]),
        residence_province_code: b"GZ".to_vec(),
        residence_city_code: b"018".to_vec(),
        account_id: Some([0x33; 32]),
        binding_revision: Some(1),
        identity_version: 0,
        voting: None,
        candidate: None,
    };
    let mut finalized = onchina::chain_submit::FinalizedChainSubmit {
        tx_hash: "different-finalized-hash".into(),
        block_hash: [0x22; 32],
        block_number: 7,
    };
    assert!(super::commit_module_projection(
        &sandbox.db,
        &session,
        &lease,
        &finalized,
        Some(&snapshot)
    )
    .await
    .is_err());
    assert!(sandbox.db.find_citizen_by_cid(&citizen).unwrap().is_none()); // 消费失败须回滚已写的公民/ID。
    let hash = format!("0x{}", "55".repeat(32));
    finalized.tx_hash = hash.clone();
    let output =
        super::commit_module_projection(&sandbox.db, &session, &lease, &finalized, Some(&snapshot))
            .await
            .unwrap()
            .unwrap();
    assert!(output["id"].as_u64().unwrap() > 0);
    assert!(sandbox
        .db
        .find_chain_sign_session(&session.request_id)
        .unwrap()
        .unwrap()
        .consumed_at
        .is_some());
    assert!(super::commit_module_projection(
        &sandbox.db,
        &session,
        &lease,
        &finalized,
        Some(&snapshot)
    )
    .await
    .is_err());
    assert_eq!(
        sandbox
            .db
            .find_citizen_by_cid(&citizen)
            .unwrap()
            .unwrap()
            .id,
        output["id"].as_u64().unwrap()
    );
    assert!(origin_matches_binding(&session, &binding));
    let other = NodeInstitutionBinding {
        institution_cid_number: "different-institution".into(),
        ..binding.clone()
    };
    assert!(!origin_matches_binding(&session, &other));
    let fresh = chain_session_with_context(serde_json::json!({}));
    assert!(!origin_matches_binding(&fresh, &binding));
}
