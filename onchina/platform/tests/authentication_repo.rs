//! 验证会话上下文复用、设置完成事务和管理员编号并发分配。

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
    assert!(sandbox
        .db
        .with_client(|conn| get_admin_by_account_id_conn(conn, &session.account_id))
        .unwrap()
        .is_none());
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
                complete_passkey_setup_conn(&mut tx, &old.account_id, &old.candidate_id)?;
                // 未提交则设置阶段不能开放。
            }
            assert!(
                get_admin_session_conn(conn, &old.token)?
                    .unwrap()
                    .setup_required
            );
            complete_passkey_setup_conn(conn, &old.account_id, &old.candidate_id)?;
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
#[ignore = "由控制台交付本机 onchina_test 数据库后执行隔离 PostgreSQL 集成回归"]
fn concurrent_admin_id_allocation_is_unique_and_overflow_is_explicit() {
    let sandbox = crate::database::IsolatedTestDb::new();
    let handles = (0..8)
        .map(|_| {
            let db = sandbox.db.clone();
            std::thread::spawn(move || {
                (0..16)
                    .map(|_| db.with_client(next_admin_id_conn).unwrap())
                    .collect::<Vec<_>>()
            })
        })
        .collect::<Vec<_>>();
    let ids = handles
        .into_iter()
        .flat_map(|handle| handle.join().unwrap())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(ids.len(), 128);
    assert_eq!(ids.first(), Some(&1));
    assert_eq!(ids.last(), Some(&128));
    sandbox
        .db
        .with_client(|conn| {
            conn.execute(
                "UPDATE sequence SET last_value=9223372036854775807 WHERE seq_key='admin_id'",
                &[],
            )
            .unwrap();
            assert!(next_admin_id_conn(conn).is_err());
            Ok(())
        })
        .unwrap();
}

#[test]
fn frg_role_province_cannot_be_overwritten_by_institution_location() {
    let institution_location = Some((Some("中枢省".to_string()), Some("锦程市".to_string()), None));
    let scope = authorization_scope_from_sources("FRG", Some(*b"GZ"), institution_location)
        .expect("FRG role province must resolve");

    assert_eq!(scope, (Some("贵州省".to_string()), None, None));
}

#[test]
fn frg_authorization_fails_without_role_province() {
    let result = authorization_scope_from_sources("FRG", None, None);
    assert_eq!(
        result.unwrap_err(),
        format!("{SCOPE_ERROR_PREFIX}FRG authorization requires frg_province_code")
    );
}

/// 作用域类错误必须带 `scope:` 前缀,否则会被当成数据库故障、报成
/// 「请稍后重试」——而这类错误重试永远不会成功。
#[test]
fn scope_errors_are_prefixed_for_classification() {
    for result in [
        authorization_scope_from_sources("FRG", None, None),
        authorization_scope_from_sources("FRG", Some(*b"??"), None),
        authorization_scope_from_sources("SFGY", None, None),
    ] {
        let message = result.expect_err("这些前置条件都不满足,必须失败");
        assert!(
            message.starts_with(SCOPE_ERROR_PREFIX),
            "作用域错误缺少 scope: 前缀,会被误判成数据库故障: {message}"
        );
    }
}

/// 非 Tier1 机构给出作用域后即通过,不再要求本地投影命中。
#[test]
fn non_tier1_accepts_scope_derived_without_projection() {
    let derived_from_cid = Some((Some("贵州省".to_string()), Some("绥阳市".to_string()), None));
    let scope = authorization_scope_from_sources("SFGY", None, derived_from_cid)
        .expect("私权机构作用域可由机构 CID 派生");
    assert_eq!(
        scope,
        (Some("贵州省".to_string()), Some("绥阳市".to_string()), None)
    );
}

/// 机构 CID 的 R5 = 省2 + 市3,私权机构也能解出——这是缺投影时的兜底真源。
#[test]
fn private_institution_cid_yields_province_and_city() {
    let (province, city) =
        cid_scope_codes(b"GZ018-SFGYR-201206100-2026").expect("私权机构 CID 必须能解出省市码");
    assert_eq!(&province, b"GZ");
    assert_eq!(&city, b"018");
}

/// 自然人 CID 去地域化,传进来必须 fail-closed,不得把号段误读成区划码。
#[test]
fn person_cid_scope_is_rejected() {
    assert!(cid_scope_codes(b"CN001-CTZN-000000001-2026").is_err());
}

fn login_membership(cid: &str) -> crate::chain_runtime::ActiveAdminMembership {
    crate::chain_runtime::ActiveAdminMembership {
        institution_code: *b"FRG\0",
        cid_number: cid.into(),
        frg_province_code: Some(*b"GZ"),
    }
}

fn login_binding(member: &crate::chain_runtime::ActiveAdminMembership) -> NodeInstitutionBinding {
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
    memberships: &[crate::chain_runtime::ActiveAdminMembership],
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
fn login_checks_exact_binding_before_creating_a_challenge() {
    use axum::http::StatusCode;
    let sandbox = crate::database::IsolatedTestDb::new();
    let member = login_membership("ZS001-FRG07-249474503-2026");
    let binding = login_binding(&member);
    sandbox
        .db
        .with_client(|conn| upsert_active_node_binding_conn(conn, &binding))
        .unwrap();
    let foreign = login_membership("ZS001-FRG07-249474504-2026");
    let wrong_code = crate::chain_runtime::ActiveAdminMembership {
        institution_code: *b"CREG",
        ..member.clone()
    };
    let other_province = crate::chain_runtime::ActiveAdminMembership {
        frg_province_code: Some(*b"GD"),
        ..member.clone()
    };
    for (memberships, expected) in [
        (vec![], StatusCode::FORBIDDEN),
        (vec![foreign.clone()], StatusCode::FORBIDDEN),
        (vec![wrong_code], StatusCode::FORBIDDEN),
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
            assert_eq!(body["errorCode"], "ONCHINA_LOGIN_ADMIN_NOT_ONCHAIN");
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
                90_000
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
            StatusCode::FORBIDDEN
        );
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
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert!(login_response_body(response).get("data").is_none());
}

#[test]
#[ignore = "由控制台交付本机 onchina_test 数据库后执行隔离 PostgreSQL 集成回归"]
fn login_waits_for_binding_lock_and_rejects_a_concurrent_rebind() {
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
                    StatusCode::FORBIDDEN
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
        0
    );
}
