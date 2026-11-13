//! 原所属模块回归，断言与输入保持。
use super::*;

#[test]
#[ignore = "由控制台交付本机 onchina_test 数据库后执行隔离 PostgreSQL 集成回归"]
fn concurrent_first_registration_cannot_create_two_credentials_without_stepup() {
    use webauthn_authenticator_rs::{softpasskey::SoftPasskey, WebauthnAuthenticator};
    let sandbox = crate::database::IsolatedTestDb::new();
    let (state, _, session) = crate::testing::session_fixture(&sandbox.db, true);
    let other = crate::authentication::login::AdminSession {
        token: "other-session".into(),
        ..session.clone()
    };
    sandbox
        .db
        .with_client(|conn| crate::authentication::repo::insert_admin_session_conn(conn, &other))
        .unwrap();
    let app = crate::build_routes(state);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let origin = Url::parse("https://onchina.crcfrcn.com:1988").unwrap();
    let mut requests = Vec::new();
    for token in [&session.token, &other.token] {
        let (status, begin) = runtime.block_on(call_token(
            app.clone(),
            "/api/admin/auth/passkey/register/begin",
            Some(serde_json::json!({})),
            None,
            token,
        ));
        assert_eq!(status, StatusCode::OK);
        let mut authenticator = WebauthnAuthenticator::new(SoftPasskey::new(true));
        let credential = authenticator
            .do_registration(
                origin.clone(),
                serde_json::from_value(begin["data"]["challenge"].clone()).unwrap(),
            )
            .unwrap();
        requests.push((
            token.clone(),
            serde_json::json!({"ceremonyId":begin["data"]["ceremonyId"],"credential":credential}),
        ));
    }
    let barrier = std::sync::Barrier::new(2);
    let statuses = std::thread::scope(|scope| {
        let handles = requests
            .into_iter()
            .map(|(token, body)| {
                let app = app.clone();
                let barrier = &barrier;
                scope.spawn(move || {
                    let runtime = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .unwrap();
                    barrier.wait();
                    runtime
                        .block_on(call_token(
                            app,
                            "/api/admin/auth/passkey/register/finish",
                            Some(body),
                            None,
                            &token,
                        ))
                        .0
                })
            })
            .collect::<Vec<_>>();
        handles
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .collect::<Vec<_>>()
    });
    assert_eq!(
        statuses
            .iter()
            .filter(|status| **status == StatusCode::OK)
            .count(),
        1
    );
    assert_eq!(
        statuses
            .iter()
            .filter(|status| **status == StatusCode::FORBIDDEN)
            .count(),
        1
    );
    sandbox
        .db
        .with_client(|conn| {
            let count: i64 = conn
                .query_one("SELECT COUNT(*) FROM credential", &[])
                .unwrap()
                .get(0);
            assert_eq!(count, 1);
            for token in [&session.token, &other.token] {
                assert!(
                    !crate::authentication::repo::get_admin_session_conn(conn, token)?
                        .unwrap()
                        .setup_required
                );
            }
            Ok(())
        })
        .unwrap();
}

use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use tower::ServiceExt;
async fn call_token(
    app: axum::Router,
    path: &str,
    payload: Option<serde_json::Value>,
    assertion: Option<&str>,
    token: &str,
) -> (StatusCode, serde_json::Value) {
    let mut request = Request::builder()
        .method(if payload.is_some() { "POST" } else { "GET" })
        .uri(path)
        .header("authorization", format!("Bearer {token}"));
    if let Some(assertion) = assertion {
        request = request.header(PASSKEY_ASSERTION_HEADER, assertion);
    }
    let body = payload
        .map(|payload| payload.to_string())
        .unwrap_or_default();
    let response = app
        .oneshot(
            request
                .header("content-type", "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    (
        status,
        serde_json::from_slice(&to_bytes(response.into_body(), 1024 * 1024).await.unwrap())
            .unwrap(),
    )
}

async fn call(
    app: axum::Router,
    path: &str,
    payload: Option<serde_json::Value>,
    assertion: Option<&str>,
) -> (StatusCode, serde_json::Value) {
    call_token(app, path, payload, assertion, "fixture-session").await
}

#[test]
#[ignore = "由控制台交付本机 onchina_test 数据库后执行隔离 PostgreSQL 集成回归"]
fn mandatory_setup_and_credential_update_follow_real_http_and_webauthn() {
    use webauthn_authenticator_rs::{softpasskey::SoftPasskey, WebauthnAuthenticator};

    let sandbox = crate::database::IsolatedTestDb::new();
    let (state, _, session) = crate::testing::session_fixture(&sandbox.db, true);
    let app = crate::build_routes(state);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let (status, check) = call(app.clone(), "/api/admin/auth/check", None, None).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(check["data"]["setupRequired"], true);
        let (status, rejected) = call(app.clone(), "/api/admin/cid/meta", None, None).await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(rejected["errorCode"], "ONCHINA_PASSKEY_SETUP_REQUIRED");
        let origin = Url::parse("https://onchina.crcfrcn.com:1988").unwrap();
        let mut authenticator = WebauthnAuthenticator::new(SoftPasskey::new(true));
        let (status, begin) = call(app.clone(), "/api/admin/auth/passkey/register/begin", Some(serde_json::json!({})), None).await;
        assert_eq!(status, StatusCode::OK);
        let registration = authenticator.do_registration(origin.clone(), serde_json::from_value(begin["data"]["challenge"].clone()).unwrap()).unwrap();
        let body = serde_json::json!({"ceremonyId":begin["data"]["ceremonyId"],"credential":registration});
        assert_eq!(call(app.clone(), "/api/admin/auth/passkey/register/finish", Some(body), None).await.0, StatusCode::OK);
        assert_eq!(call(app.clone(), "/api/admin/cid/meta", None, None).await.0, StatusCode::OK);
        assert!(!sandbox.db.with_client(|conn| Ok(crate::authentication::repo::get_admin_session_conn(conn, &session.token)?.unwrap().setup_required)).unwrap());

        // 已有凭证再注册：无旧密钥断言必须拒绝，不能复用首次初始化特例。
        let (_, begin) = call(app.clone(), "/api/admin/auth/passkey/register/begin", Some(serde_json::json!({})), None).await;
        let registration = authenticator.do_registration(origin.clone(), serde_json::from_value(begin["data"]["challenge"].clone()).unwrap()).unwrap();
        let body = serde_json::json!({"ceremonyId":begin["data"]["ceremonyId"],"credential":registration});
        assert_eq!(call(app.clone(), "/api/admin/auth/passkey/register/finish", Some(body.clone()), None).await.0, StatusCode::FORBIDDEN);
        let (_, assertion_begin) = call(app.clone(), "/api/admin/auth/passkey/assert/begin", Some(serde_json::json!({})), None).await;
        let credential = authenticator.do_authentication(origin, serde_json::from_value(assertion_begin["data"]["challenge"].clone()).unwrap()).unwrap();
        let (status, assertion_finish) = call(app.clone(), "/api/admin/auth/passkey/assert/finish", Some(serde_json::json!({"ceremonyId":assertion_begin["data"]["ceremonyId"],"credential":credential})), None).await;
        assert_eq!(status, StatusCode::OK);
        let assertion = assertion_finish["data"]["assertionId"].as_str().unwrap();
        let mut invalid = body.clone();
        invalid["credential"]["response"]["clientDataJSON"] = serde_json::json!("e30");
        assert_eq!(call(app.clone(), "/api/admin/auth/passkey/register/finish", Some(invalid), Some(assertion)).await.0, StatusCode::BAD_REQUEST);
        // 校验失败应回滚断言与ceremony消费；正确重试只写入一次。
        assert_eq!(call(app.clone(), "/api/admin/auth/passkey/register/finish", Some(body.clone()), Some(assertion)).await.0, StatusCode::OK);
        assert_eq!(call(app, "/api/admin/auth/passkey/register/finish", Some(body), Some(assertion)).await.0, StatusCode::FORBIDDEN);
        let count: i64 = sandbox.db.with_client(|conn| Ok(conn.query_one("SELECT COUNT(*) FROM credential", &[]).unwrap().get(0))).unwrap();
        assert_eq!(count, 2);
    });
}

#[test]
#[ignore = "由控制台交付本机 onchina_test 数据库后执行隔离 PostgreSQL 集成回归"]
fn assertion_is_bound_to_account_expiry_and_single_consumption() {
    let sandbox = crate::database::IsolatedTestDb::new();
    let actor = format!("0x{}", "11".repeat(32));
    let other = format!("0x{}", "22".repeat(32));
    let now = Utc::now();
    sandbox
        .db
        .with_client(|conn| {
            store::insert_assertion_conn(conn, "live", &actor, now + Duration::seconds(120))?;
            store::insert_assertion_conn(conn, "expired", &actor, now - Duration::seconds(1))?;
            assert!(!store::consume_assertion_conn(
                conn, "missing", &actor, now
            )?);
            assert!(!store::consume_assertion_conn(conn, "live", &other, now)?);
            assert!(!store::consume_assertion_conn(
                conn, "expired", &actor, now
            )?);
            assert!(store::consume_assertion_conn(conn, "live", &actor, now)?);
            assert!(!store::consume_assertion_conn(conn, "live", &actor, now)?);
            store::insert_assertion_conn(
                conn,
                "transaction",
                &actor,
                now + Duration::seconds(120),
            )?;
            {
                let mut tx = conn.transaction().map_err(|e| e.to_string())?;
                assert!(store::consume_assertion_conn(
                    &mut tx,
                    "transaction",
                    &actor,
                    now
                )?);
                assert!(!store::consume_assertion_conn(
                    &mut tx,
                    "transaction",
                    &actor,
                    now
                )?);
                // 上层治理失败回滚后，断言必须仍可由后续合法事务消费。
                tx.rollback().map_err(|e| e.to_string())?;
            }
            let mut tx = conn.transaction().map_err(|e| e.to_string())?;
            assert!(store::consume_assertion_conn(
                &mut tx,
                "transaction",
                &actor,
                now
            )?);
            tx.commit().map_err(|e| e.to_string())?;
            assert!(!store::consume_assertion_conn(
                conn,
                "transaction",
                &actor,
                now
            )?);
            Ok(())
        })
        .unwrap();
}

#[test]
fn admin_user_uuid_is_stable_for_canonical_account_id() {
    let account_id = "0x1111111111111111111111111111111111111111111111111111111111111111";
    let a = admin_user_uuid(account_id);
    let b = admin_user_uuid(account_id);
    assert_eq!(a, b, "相同规范账户 ID 的 user handle 必须稳定");
    let c = admin_user_uuid("0x2222222222222222222222222222222222222222222222222222222222222222");
    assert_ne!(a, c, "不同账户 user handle 必须不同");
}

#[test]
fn build_webauthn_config_validation() {
    // 直接调内层函数,不碰进程级 env(避免与并行测试争用)。
    assert!(
        build_webauthn_from("localhost", "not a url").is_err(),
        "非法 origin 必须 fail-closed"
    );
    assert!(
        build_webauthn_from("onchina.crcfrcn.com", "https://onchina.crcfrcn.com:1988").is_ok(),
        "onchina.crcfrcn.com HTTPS 默认配置必须合法"
    );
}

#[test]
fn passkey_register_and_authenticate_round_trip() {
    use webauthn_authenticator_rs::softpasskey::SoftPasskey;
    use webauthn_authenticator_rs::WebauthnAuthenticator;
    use webauthn_rs::prelude::Url;

    let webauthn = build_webauthn_from("onchina.crcfrcn.com", "https://onchina.crcfrcn.com:1988")
        .expect("build");
    let origin = Url::parse("https://onchina.crcfrcn.com:1988").unwrap();
    // falsify_uv=true:软认证器提供 UserVerification(配置要求 UV)。
    let mut authenticator = WebauthnAuthenticator::new(SoftPasskey::new(true));

    // 注册往返:server 出挑战 → 软认证器签 → server 验证落凭证。
    let user_id = admin_user_uuid("0xtestadmin");
    let (ccr, reg_state) = webauthn
        .start_passkey_registration(user_id, "0xtestadmin", "0xtestadmin", None)
        .expect("start registration");
    let reg = authenticator
        .do_registration(origin.clone(), ccr)
        .expect("authenticator register");
    let passkey = webauthn
        .finish_passkey_registration(&reg, &reg_state)
        .expect("finish registration");

    // 断言往返:server 出挑战 → 软认证器签 → server 验证 → 命中注册凭证。
    let (rcr, auth_state) = webauthn
        .start_passkey_authentication(std::slice::from_ref(&passkey))
        .expect("start authentication");
    let auth = authenticator
        .do_authentication(origin, rcr)
        .expect("authenticator authenticate");
    let result = webauthn
        .finish_passkey_authentication(&auth, &auth_state)
        .expect("finish authentication");
    assert_eq!(
        result.cred_id(),
        passkey.cred_id(),
        "断言结果必须命中注册的凭证"
    );
}
