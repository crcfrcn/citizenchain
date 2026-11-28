//! 验证会话上下文复用、设置完成事务和所选人员岗位约束。

use super::*;

#[test]
#[ignore = "由控制台交付本机 onchina_test 数据库后执行隔离 PostgreSQL 集成回归"]
fn ready_session_reads_verified_context_without_admin_lookup_or_chain() {
    let sandbox = crate::database::IsolatedTestDb::new();
    let (state, headers, session) = crate::testing::session_fixture(&sandbox.db, false);
    let ctx = crate::authentication::login::require_admin_any(&state, &headers).unwrap();
    assert_eq!(ctx.scope_province_name.as_deref(), Some("贵州省"));
    assert_eq!(ctx.institution_cid_number, session.institution_cid_number);
    let issued = AdminSession::new(
        "new-session".into(),
        session.candidate_id.clone(),
        session.last_active_at,
        session.identify(),
    );
    assert_eq!(
        issued.context().scope_province_name,
        ctx.scope_province_name
    );
    assert_eq!(
        issued.expires_at,
        issued.last_active_at + Duration::hours(8)
    );
    let stored = serde_json::to_value(&issued).unwrap();
    assert!(!stored.as_object().unwrap().contains_key("token"));
    assert_eq!(stored["setupRequired"], false);
    delete_admin_session(&sandbox.db, &session.token).unwrap();
    assert_eq!(
        crate::authentication::login::require_admin_any(&state, &headers)
            .unwrap_err()
            .status(),
        axum::http::StatusCode::UNAUTHORIZED
    );
}

#[test]
#[ignore = "由控制台交付本机 onchina_test 数据库后执行隔离 PostgreSQL 集成回归"]
fn setup_gate_and_activity_update_preserve_atomic_completion() {
    let sandbox = crate::database::IsolatedTestDb::new();
    let (state, headers, mut old) = crate::testing::session_fixture(&sandbox.db, true);
    assert!(crate::authentication::login::require_admin_session(&state, &headers).is_ok());
    assert_eq!(
        crate::authentication::login::require_admin_any(&state, &headers)
            .unwrap_err()
            .status(),
        axum::http::StatusCode::FORBIDDEN
    );
    sandbox
        .db
        .with_client(|conn| {
            {
                let mut tx = conn.transaction().map_err(|error| error.to_string())?;
                complete_passkey_setup_conn(&mut tx, &old.context())?;
                // 未提交则设置阶段不能开放。
            }
            assert!(
                get_admin_session_conn(conn, &old.token)?
                    .unwrap()
                    .setup_required
            );
            complete_passkey_setup_conn(conn, &old.context())?;
            old.last_active_at = Utc::now();
            touch_admin_session_conn(conn, &old)?;
            let current = get_admin_session_conn(conn, &old.token)?.unwrap();
            assert!(!current.setup_required);
            assert_eq!(current.scope_province_name, old.scope_province_name);
            Ok(())
        })
        .unwrap();
    assert!(crate::authentication::login::require_admin_any(&state, &headers).is_ok());
}

#[test]
#[ignore = "由控制台交付本机 onchina_test 数据库后执行隔离 PostgreSQL 集成回归"]
fn old_or_expired_session_cannot_bypass_setup() {
    let sandbox = crate::database::IsolatedTestDb::new();
    let (state, headers, mut session) = crate::testing::session_fixture(&sandbox.db, false);
    session.expires_at = Utc::now() - Duration::seconds(1);
    sandbox
        .db
        .with_client(|conn| insert_admin_session_conn(conn, &session))
        .unwrap();
    assert_eq!(
        crate::authentication::login::require_admin_any(&state, &headers)
            .unwrap_err()
            .status(),
        axum::http::StatusCode::UNAUTHORIZED
    );
    sandbox
        .db
        .with_client(|conn| {
            let mut payload = serde_json::to_value(&session).unwrap();
            payload.as_object_mut().unwrap().remove("setupRequired");
            conn.execute(
                "INSERT INTO session(token_hash,payload_json) VALUES($1,$2)",
                &[&admin_session_token_hash(&session.token), &payload],
            )
            .unwrap();
            assert!(get_admin_session_conn(conn, &session.token)?.is_none());
            Ok(())
        })
        .unwrap();
}

#[test]
fn private_cid_has_scope_and_person_cid_does_not() {
    use primitives::cid::number::cid_scope_codes;
    let (province, city) = cid_scope_codes(b"GZ018-SFGYR-201206100-2026").unwrap();
    assert_eq!(&province, b"GZ");
    assert_eq!(&city, b"018");
    assert!(cid_scope_codes(crate::testing::person_cid().as_bytes()).is_err());
}

fn login_membership(cid: &str) -> crate::chain_runtime::ActiveMembership {
    crate::chain_runtime::ActiveMembership {
        institution_code: *b"FRG\0",
        cid_number: cid.into(),
        person_cid: crate::testing::person_cid(),
        role_code: "REGISTRAR".into(),
        role_name: "登记员".into(),
        town_code: vec![],
        frg_province_code: Some(*b"GZ"),
    }
}

fn login_binding(member: &crate::chain_runtime::ActiveMembership) -> NodeInstitutionBinding {
    NodeInstitutionBinding {
        binding_id: uuid::Uuid::new_v4().to_string(),
        candidate_id: member.candidate_id(),
        institution_code: "FRG".into(),
        institution_cid_number: member.cid_number.clone(),
        // 本次登录使用另一账户，机构归属不能被误写为只允许首次绑定操作者。
        bound_account_id: format!("0x{}", "11".repeat(32)),
        bound_at: Utc::now(),
        status: "ACTIVE".into(),
    }
}

fn create_test_login(
    db: &crate::Db,
    memberships: &[crate::chain_runtime::ActiveMembership],
) -> axum::response::Response {
    crate::authentication::login::create_login_request(
        db,
        format!("0x{}", "22".repeat(32)),
        memberships,
        crate::platform_url(),
        crate::PLATFORM_HOST.into(),
        "login-test-session".into(),
    )
}

fn login_response_body(response: axum::response::Response) -> serde_json::Value {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        serde_json::from_slice(
            &axum::body::to_bytes(response.into_body(), 1024 * 1024)
                .await
                .unwrap(),
        )
        .unwrap()
    })
}

#[test]
#[ignore = "由控制台交付本机 onchina_test 数据库后执行隔离 PostgreSQL 集成回归"]
fn login_accepts_each_current_assignment_without_changing_host_binding() {
    use axum::http::StatusCode;
    let sandbox = crate::database::IsolatedTestDb::new();
    let member = login_membership("ZS001-FRG07-249474503-2026");
    let binding = login_binding(&member);
    sandbox
        .db
        .with_client(|conn| upsert_active_node_binding_conn(conn, &binding))
        .unwrap();
    let foreign = login_membership("ZS001-FRG07-249474504-2026");
    let wrong_code = crate::chain_runtime::ActiveMembership {
        institution_code: *b"CREG",
        ..member.clone()
    };
    let other_province = crate::chain_runtime::ActiveMembership {
        frg_province_code: Some(*b"GD"),
        ..member.clone()
    };
    for (memberships, expected) in [
        (vec![], StatusCode::FORBIDDEN),
        (vec![foreign.clone()], StatusCode::OK),
        (vec![wrong_code], StatusCode::OK),
        (vec![member.clone()], StatusCode::OK),
        (vec![foreign, member.clone()], StatusCode::OK),
        // FRG的省任职限定登录后作用域，不改变同一机构CID的节点归属。
        (vec![other_province], StatusCode::OK),
    ] {
        sandbox
            .db
            .with_client(|conn| {
                conn.execute("DELETE FROM login", &[]).unwrap();
                Ok(())
            })
            .unwrap();
        let response = create_test_login(&sandbox.db, &memberships);
        assert_eq!(response.status(), expected);
        let body = login_response_body(response);
        let count: i64 = sandbox
            .db
            .with_client(|conn| {
                Ok(conn
                    .query_one("SELECT COUNT(*) FROM login", &[])
                    .unwrap()
                    .get(0))
            })
            .unwrap();
        if expected == StatusCode::FORBIDDEN {
            assert_eq!(count, 0);
            assert_eq!(body["code"], 2002);
            assert!(body.get("data").is_none());
        } else {
            assert_eq!(count, 1);
            assert!(body["data"]["loginQrPayload"].is_string());
            let challenge = sandbox
                .db
                .with_client(|conn| {
                    get_login_sign_request_conn(conn, body["data"]["challengeId"].as_str().unwrap())
                })
                .unwrap()
                .unwrap();
            assert_eq!(challenge.account_id, format!("0x{}", "22".repeat(32)));
            assert_eq!(
                challenge.expires_at.timestamp_millis() - challenge.issued_at.timestamp_millis(),
                300_000
            );
            assert!(!challenge.consumed);
        }
    }
    for mut broken in [
        NodeInstitutionBinding {
            institution_code: "CREG".into(),
            ..binding.clone()
        },
        NodeInstitutionBinding {
            institution_cid_number: "another-institution".into(),
            ..binding.clone()
        },
        NodeInstitutionBinding {
            candidate_id: "another-candidate".into(),
            ..binding
        },
    ] {
        broken.binding_id = uuid::Uuid::new_v4().to_string();
        sandbox
            .db
            .with_client(|conn| {
                conn.execute("DELETE FROM login", &[]).unwrap();
                upsert_active_node_binding_conn(conn, &broken)
            })
            .unwrap();
        assert_eq!(
            create_test_login(&sandbox.db, &[member.clone()]).status(),
            StatusCode::OK
        );
        assert_eq!(
            sandbox
                .db
                .with_client(|conn| Ok(conn
                    .query_one("SELECT COUNT(*) FROM login", &[])
                    .unwrap()
                    .get::<_, i64>(0)))
                .unwrap(),
            1
        );
    }
}

#[test]
#[ignore = "由控制台交付本机 onchina_test 数据库后执行隔离 PostgreSQL 集成回归"]
fn first_binding_login_remains_available_and_failed_storage_returns_no_qr() {
    use axum::http::StatusCode;
    let sandbox = crate::database::IsolatedTestDb::new();
    let member = login_membership("ZS001-FRG07-249474503-2026");
    assert_eq!(
        create_test_login(&sandbox.db, &[]).status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        create_test_login(&sandbox.db, &[member.clone()]).status(),
        StatusCode::OK
    );
    assert!(sandbox
        .db
        .with_client(get_active_node_binding_conn)
        .unwrap()
        .is_none());
    assert_eq!(
        sandbox
            .db
            .with_client(|conn| Ok(conn
                .query_one("SELECT COUNT(*) FROM session", &[])
                .unwrap()
                .get::<_, i64>(0)))
            .unwrap(),
        0
    );
    sandbox.db.with_client(|conn| {
        conn.batch_execute("DELETE FROM login; ALTER TABLE login ADD CONSTRAINT reject_login_test CHECK(false)").unwrap();
        Ok(())
    }).unwrap();
    let response = create_test_login(&sandbox.db, &[member]);
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert!(login_response_body(response).get("data").is_none());
    assert_eq!(
        sandbox
            .db
            .with_client(|conn| Ok(conn
                .query_one("SELECT COUNT(*) FROM login", &[])
                .unwrap()
                .get::<_, i64>(0)))
            .unwrap(),
        0
    );
    sandbox
        .db
        .with_client(|conn| {
            conn.batch_execute(
                "ALTER TABLE login DROP CONSTRAINT reject_login_test; DROP TABLE binding",
            )
            .unwrap();
            Ok(())
        })
        .unwrap();
    let response = create_test_login(
        &sandbox.db,
        &[login_membership("ZS001-FRG07-249474503-2026")],
    );
    assert_eq!(response.status(), StatusCode::OK);
    assert!(login_response_body(response)["data"]["challengeId"].is_string());
}

#[test]
#[ignore = "由控制台交付本机 onchina_test 数据库后执行隔离 PostgreSQL 集成回归"]
fn login_waits_for_host_lock_without_treating_host_as_authorization() {
    use axum::http::StatusCode;
    let sandbox = crate::database::IsolatedTestDb::new();
    let member = login_membership("ZS001-FRG07-249474503-2026");
    sandbox
        .db
        .with_client(|conn| upsert_active_node_binding_conn(conn, &login_binding(&member)))
        .unwrap();
    sandbox
        .db
        .with_client(|conn| {
            let mut tx = conn.transaction().unwrap();
            lock_node_binding_conn(&mut tx)?;
            let barrier = std::sync::Barrier::new(2);
            std::thread::scope(|scope| {
                let (sender, receiver) = std::sync::mpsc::channel();
                let barrier_ref = &barrier;
                let db = sandbox.db.clone();
                let worker = scope.spawn(move || {
                    barrier_ref.wait();
                    sender
                        .send(create_test_login(&db, &[member]).status())
                        .unwrap();
                });
                barrier.wait();
                // 持有锁时不得提前生成或返回挑战；随后切换到该账户不属于的机构。
                assert!(matches!(
                    receiver.recv_timeout(std::time::Duration::from_millis(150)),
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout)
                ));
                let changed = login_membership("ZS001-FRG07-249474504-2026");
                upsert_active_node_binding_conn(&mut tx, &login_binding(&changed))?;
                tx.commit().unwrap();
                assert_eq!(
                    receiver
                        .recv_timeout(std::time::Duration::from_secs(5))
                        .unwrap(),
                    StatusCode::OK
                );
                worker.join().unwrap();
                Ok(())
            })
        })
        .unwrap();
    assert_eq!(
        sandbox
            .db
            .with_client(|conn| Ok(conn
                .query_one("SELECT COUNT(*) FROM login", &[])
                .unwrap()
                .get::<_, i64>(0)))
            .unwrap(),
        1
    );
}

#[test]
fn person_role_and_binding_are_mandatory_session_fields() {
    let now = Utc::now().timestamp_millis();
    let raw = serde_json::json!({"bindingRevision":"1","personCid":crate::testing::person_cid(),"roleCode":"REGISTRAR","accountId":format!("0x{}","11".repeat(32)),"institutionCode":"FRG","candidateId":"candidate","institutionCidNumber":"original","adminLevel":null,"familyName":"","givenName":"","scopeProvinceName":null,"scopeCityName":null,"scopeTownName":null,"cidShortName":null,"capabilities":crate::modules::capability::EMPTY,"workspace":{"workspace_kind":"registry","workspace_title":"fixture","workspace_sections":[],"workspace_modules":[]},"setupRequired":false,"expiresAt":now+60000,"lastActiveAt":now});
    assert!(serde_json::from_value::<AdminSession>(raw.clone()).is_ok());
    for field in ["personCid", "roleCode", "bindingRevision"] {
        let mut missing = raw.clone();
        missing.as_object_mut().unwrap().remove(field);
        assert!(serde_json::from_value::<AdminSession>(missing).is_err());
    }
    let mut retired = raw;
    retired["walletRevision"] = "0".into();
    assert!(serde_json::from_value::<AdminSession>(retired).is_err());
}

#[tokio::test]
#[ignore = "使用独立onchina_test数据库和显式权威快照验证真实Router"]
async fn protected_http_rejects_changed_person_binding_role_and_scope() {
    use tower::ServiceExt;
    let sandbox = crate::database::IsolatedTestDb::new();
    let (state, _, session) = crate::testing::session_fixture(&sandbox.db, false);
    let app = crate::build_routes(state);
    let call = |token: &str| {
        axum::http::Request::builder()
            .uri("/api/admin/auth/check")
            .header("authorization", format!("Bearer {token}"))
            .body(axum::body::Body::empty())
            .unwrap()
    };
    assert_eq!(
        app.clone()
            .oneshot(call(&session.token))
            .await
            .unwrap()
            .status(),
        axum::http::StatusCode::OK
    );
    for mutation in ["version", "wallet", "role", "province", "institution"] {
        let mut actual = session.context();
        match mutation {
            "version" => actual.binding_revision = "2".into(),
            "wallet" => actual.account_id = format!("0x{}", "22".repeat(32)),
            "role" => actual.role_code = "OTHER".into(),
            "province" => actual.scope_province_name = Some("广东省".into()),
            _ => actual.institution_cid_number = "another-institution".into(),
        }
        *sandbox.db.role_snapshot.lock().unwrap() = Some(actual);
        assert_eq!(
            app.clone()
                .oneshot(call(&session.token))
                .await
                .unwrap()
                .status(),
            axum::http::StatusCode::UNAUTHORIZED,
            "{mutation}"
        );
    }
    let mut current = session.clone();
    current.token = "returned-wallet-session".into();
    current.binding_revision = "2".into();
    *sandbox.db.role_snapshot.lock().unwrap() = Some(current.context());
    sandbox
        .db
        .with_client(|conn| insert_admin_session_conn(conn, &current))
        .unwrap();
    assert_eq!(
        app.oneshot(call(&current.token)).await.unwrap().status(),
        axum::http::StatusCode::OK
    );
}
