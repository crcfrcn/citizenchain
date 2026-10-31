//! 原所属模块回归，断言与输入保持。
use super::*;

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
        build_webauthn_from("onchina.local", "https://onchina.local:8964").is_ok(),
        "onchina.local HTTPS 默认配置必须合法"
    );
}

#[test]
fn passkey_register_and_authenticate_round_trip() {
    use webauthn_authenticator_rs::softpasskey::SoftPasskey;
    use webauthn_authenticator_rs::WebauthnAuthenticator;
    use webauthn_rs::prelude::Url;

    let webauthn =
        build_webauthn_from("onchina.local", "https://onchina.local:8964").expect("build");
    let origin = Url::parse("https://onchina.local:8964").unwrap();
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
