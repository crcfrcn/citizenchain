//! 原所属模块回归，断言与输入保持。
use super::*;
use crate::authentication::login::{AdminSession, NodeInstitutionBinding};

#[test]
#[ignore = "由控制台交付本机 onchina_test 数据库后执行隔离 PostgreSQL 集成回归"]
fn concurrent_security_grant_consumers_cannot_replay_one_grant() {
    let sandbox = crate::database::IsolatedTestDb::new();
    let actor = format!("0x{}", "11".repeat(32));
    let now = Utc::now();
    let institution = format!(
        "GZ018-CREG{}-000000001-2026",
        primitives::cid::number::checksum_char_m1("GZ018CREG0000000012026", false)
    );
    let binding = NodeInstitutionBinding {
        binding_id: "binding-test".into(),
        candidate_id: "candidate-test".into(),
        institution_code: "CREG".into(),
        institution_cid_number: institution.clone(),
        bound_account_id: actor.clone(),
        bound_at: now,
        status: "ACTIVE".into(),
    };
    let ctx = AdminAuthContext {
        account_id: actor.clone(),
        institution_cid_number: institution.clone(),
        institution_code: "CREG".into(),
        admin_level: Some("CITY".into()),
        family_name: "测".into(),
        given_name: "试".into(),
        scope_province_name: Some("贵州省".into()),
        scope_city_name: Some("绥阳市".into()),
        scope_town_name: None,
        cid_short_name: None,
    };
    let payload = json!({"target":"subject-a"});
    let grant = AdminSecurityGrant {
        grant_id: "grant-test".into(),
        action_type: "INSTITUTION_UPDATE".into(),
        actor_account_id: actor.clone(),
        actor_institution_code: "CREG".into(),
        actor_cid_number: institution,
        actor_province_name: "贵州省".into(),
        actor_city_name: Some("绥阳市".into()),
        auth_type: AdminOperationAuth::PasskeyColdSign,
        target: "subject-a".into(),
        payload_hash: hash_json(&payload),
        issued_at: now,
        expires_at: now + Duration::minutes(2),
        consumed: false,
    };
    sandbox
        .db
        .with_client(|conn| {
            repo::upsert_active_node_binding_conn(conn, &binding)?;
            repo::insert_security_grant_conn(conn, &grant)?;
            for token in ["assert-a", "assert-b"] {
                conn.execute(
                    "INSERT INTO assertion(assertion_id,account_id,expires_at) VALUES($1,$2,$3)",
                    &[&token, &actor, &(now + Duration::minutes(2))],
                )
                .unwrap();
            }
            Ok(())
        })
        .unwrap();
    let state = AppState {
        db: sandbox.db.clone(),
        rate_limiter: std::sync::Arc::new(LocalRateLimiter::new()),
        modules: std::sync::Arc::new(crate::modules::ModuleCatalog::new(crate::local_module())),
    };
    let barrier = std::sync::Barrier::new(2);
    let successes = std::thread::scope(|scope| {
        let handles = ["assert-a", "assert-b"]
            .into_iter()
            .map(|token| {
                let state = &state;
                let ctx = &ctx;
                let payload = &payload;
                let barrier = &barrier;
                scope.spawn(move || {
                    let mut headers = HeaderMap::new();
                    headers.insert("x-passkey-assertion", token.parse().unwrap());
                    headers.insert(ADMIN_SECURITY_GRANT_HEADER, "grant-test".parse().unwrap());
                    barrier.wait();
                    consume_admin_security_grant(
                        state,
                        &headers,
                        ctx,
                        AdminActionType::InstitutionUpdate,
                        "subject-a",
                        Some(payload),
                    )
                    .is_ok()
                })
            })
            .collect::<Vec<_>>();
        handles
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .filter(|success| *success)
            .count()
    });
    assert_eq!(successes, 1);
}

#[tokio::test]
#[ignore = "由控制台交付本机 onchina_test 数据库后执行隔离 PostgreSQL 集成回归"]
async fn unbind_requires_passkey_and_its_binding_sessions_and_challenge_are_atomic() {
    let sandbox = crate::database::IsolatedTestDb::new();
    let actor = format!("0x{}", "11".repeat(32));
    let now = Utc::now();
    let institution = format!(
        "GZ018-CREG{}-000000001-2026",
        primitives::cid::number::checksum_char_m1("GZ018CREG0000000012026", false)
    );
    let binding = NodeInstitutionBinding {
        binding_id: "binding-test".into(),
        candidate_id: "candidate-test".into(),
        institution_code: "CREG".into(),
        institution_cid_number: institution.clone(),
        bound_account_id: actor.clone(),
        bound_at: now,
        status: "ACTIVE".into(),
    };
    let challenge = sandbox
        .db
        .with_client(|conn| {
            repo::upsert_active_node_binding_conn(conn, &binding)?;
            repo::upsert_admin_conn(
                conn,
                &AdminUser {
                    id: 1,
                    account_id: actor.clone(),
                    family_name: "测".into(),
                    given_name: "试".into(),
                    institution_code: "CREG".into(),
                    institution_cid_number: Some(institution.clone()),
                    built_in: false,
                    creator_account_id: actor.clone(),
                    created_at: now,
                    updated_at: None,
                    city_name: "绥阳市".into(),
                },
            )?;
            repo::insert_admin_session_conn(
                conn,
                &AdminSession {
                    token: "test-session".into(),
                    account_id: actor.clone(),
                    institution_code: "CREG".into(),
                    candidate_id: binding.candidate_id.clone(),
                    expires_at: now + Duration::hours(1),
                    last_active_at: now,
                },
            )?;
            let preview =
                preview_node_binding_unbind_conn(conn, &AdminActionType::NodeBindingUnbind)?;
            let challenge = AdminActionChallenge {
                action_id: "unbind-test".into(),
                action_type: "NODE_BINDING_UNBIND".into(),
                actor_account_id: actor.clone(),
                actor_institution_code: "CREG".into(),
                actor_cid_number: institution.clone(),
                actor_province_name: "贵州省".into(),
                actor_city_name: Some("绥阳市".into()),
                auth_type: AdminOperationAuth::Passkey,
                target: preview.target,
                payload_text: String::new(),
                payload_hash: "request-hash".into(),
                before_hash: preview.before_hash,
                after_hash: preview.after_hash,
                request_payload: json!({}),
                issued_at: now,
                expires_at: now + Duration::minutes(2),
                consumed: false,
            };
            repo::upsert_action_challenge_conn(conn, &challenge)?;
            Ok(challenge)
        })
        .unwrap();
    let state = AppState {
        db: sandbox.db.clone(),
        rate_limiter: std::sync::Arc::new(LocalRateLimiter::new()),
        modules: std::sync::Arc::new(crate::modules::ModuleCatalog::new(crate::local_module())),
    };
    let mut headers = HeaderMap::new();
    headers.insert("authorization", "Bearer test-session".parse().unwrap());
    let mut expired = challenge.clone();
    expired.expires_at = Utc::now() - Duration::seconds(1);
    sandbox
        .db
        .with_client(|conn| repo::upsert_action_challenge_conn(conn, &expired))
        .unwrap();
    let expired_response = commit_admin_action(
        State(state.clone()),
        headers.clone(),
        Json(CommitAdminActionInput {
            action_id: challenge.action_id.clone(),
            account_id: None,
            signature: None,
            payload_hash: Some(challenge.payload_hash.clone()),
        }),
    )
    .await
    .into_response();
    assert!(matches!(
        expired_response.status(),
        StatusCode::NOT_FOUND | StatusCode::UNPROCESSABLE_ENTITY
    ));
    sandbox
        .db
        .with_client(|conn| repo::upsert_action_challenge_conn(conn, &challenge))
        .unwrap();
    let response = commit_admin_action(
        State(state),
        headers,
        Json(CommitAdminActionInput {
            action_id: challenge.action_id.clone(),
            account_id: None,
            signature: None,
            payload_hash: Some(challenge.payload_hash.clone()),
        }),
    )
    .await
    .into_response();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    sandbox
        .db
        .with_client(|conn| {
            let mut tx = conn.transaction().unwrap();
            repo::lock_node_binding_conn(&mut tx)?;
            apply_node_binding_unbind_conn(&mut tx)?;
            let mut consumed = challenge.clone();
            consumed.consumed = true;
            repo::upsert_action_challenge_conn(&mut tx, &consumed)?;
            drop(tx); // 模拟正式提交前失败，绑定、会话与挑战必须一起回滚。
            assert!(repo::get_active_node_binding_conn(conn)?.is_some());
            assert!(repo::get_admin_session_conn(conn, "test-session")?.is_some());
            assert!(
                !repo::get_action_challenge_conn(conn, &challenge.action_id, &actor)?
                    .unwrap()
                    .consumed
            );
            let mut tx = conn.transaction().unwrap();
            repo::lock_node_binding_conn(&mut tx)?;
            apply_node_binding_unbind_conn(&mut tx)?;
            repo::upsert_action_challenge_conn(&mut tx, &consumed)?;
            tx.commit().unwrap();
            assert!(repo::get_active_node_binding_conn(conn)?.is_none());
            assert!(repo::get_admin_session_conn(conn, "test-session")?.is_none());
            assert!(
                repo::get_action_challenge_conn(conn, &challenge.action_id, &actor)?
                    .unwrap()
                    .consumed
            );
            assert!(apply_node_binding_unbind_conn(conn).is_err());
            Ok(())
        })
        .unwrap();
}
