//! passkey 凭证 / ceremony 状态 / 断言令牌的结构化表读写。
//!
//! ceremony 状态(PasskeyRegistration / PasskeyAuthentication)与凭证(Passkey)以 webauthn-rs
//! 序列化存 JSONB;ceremony 与断言令牌均一次性消费(取出即删),过期由清理扫描回收。

use chrono::{DateTime, Utc};
use postgres::Client;
use webauthn_rs::prelude::Passkey;

pub(super) fn insert_credential_conn(
    conn: &mut Client,
    credential_id: &str,
    account_id: &str,
    passkey: &Passkey,
) -> Result<(), String> {
    let payload =
        serde_json::to_value(passkey).map_err(|e| format!("encode passkey failed: {e}"))?;
    conn.execute(
        "INSERT INTO admin_passkey_credentials(credential_id, account_id, passkey, created_at)
         VALUES ($1, $2, $3, now())
         ON CONFLICT (credential_id) DO UPDATE SET passkey = EXCLUDED.passkey",
        &[&credential_id, &account_id, &payload],
    )
    .map_err(|e| format!("insert passkey credential failed: {e}"))?;
    Ok(())
}

pub(super) fn list_credentials_for_admin_conn(
    conn: &mut Client,
    account_id: &str,
) -> Result<Vec<Passkey>, String> {
    let rows = conn
        .query(
            "SELECT passkey FROM admin_passkey_credentials WHERE account_id = $1",
            &[&account_id],
        )
        .map_err(|e| format!("list passkey credentials failed: {e}"))?;
    rows.iter()
        .map(|r| {
            serde_json::from_value::<Passkey>(r.get(0))
                .map_err(|e| format!("decode passkey failed: {e}"))
        })
        .collect()
}

pub(super) fn update_credential_conn(
    conn: &mut Client,
    credential_id: &str,
    passkey: &Passkey,
) -> Result<(), String> {
    let payload =
        serde_json::to_value(passkey).map_err(|e| format!("encode passkey failed: {e}"))?;
    conn.execute(
        "UPDATE admin_passkey_credentials SET passkey = $2 WHERE credential_id = $1",
        &[&credential_id, &payload],
    )
    .map_err(|e| format!("update passkey credential failed: {e}"))?;
    Ok(())
}

pub(super) fn insert_ceremony_conn(
    conn: &mut Client,
    ceremony_id: &str,
    account_id: &str,
    kind: &str,
    state: &serde_json::Value,
    expires_at: DateTime<Utc>,
) -> Result<(), String> {
    conn.execute(
        "INSERT INTO admin_passkey_ceremonies(ceremony_id, account_id, kind, state, expires_at)
         VALUES ($1, $2, $3, $4, $5)",
        &[&ceremony_id, &account_id, &kind, state, &expires_at],
    )
    .map_err(|e| format!("insert passkey ceremony failed: {e}"))?;
    Ok(())
}

/// 一次性取出并删除 ceremony 状态(校验 admin / kind / 未过期);取不到返回 None。
pub(super) fn take_ceremony_conn(
    conn: &mut Client,
    ceremony_id: &str,
    account_id: &str,
    kind: &str,
    now: DateTime<Utc>,
) -> Result<Option<serde_json::Value>, String> {
    let row = conn
        .query_opt(
            "DELETE FROM admin_passkey_ceremonies
             WHERE ceremony_id = $1 AND account_id = $2
               AND kind = $3 AND expires_at > $4
             RETURNING state",
            &[&ceremony_id, &account_id, &kind, &now],
        )
        .map_err(|e| format!("take passkey ceremony failed: {e}"))?;
    Ok(row.map(|r| r.get(0)))
}

pub(super) fn insert_assertion_conn(
    conn: &mut Client,
    assertion_id: &str,
    account_id: &str,
    expires_at: DateTime<Utc>,
) -> Result<(), String> {
    conn.execute(
        "INSERT INTO admin_passkey_assertions(assertion_id, account_id, expires_at)
         VALUES ($1, $2, $3)",
        &[&assertion_id, &account_id, &expires_at],
    )
    .map_err(|e| format!("insert passkey assertion failed: {e}"))?;
    Ok(())
}

/// 当前 admin 是否已注册任一 passkey 凭证(驱动操作列红点)。
pub(super) fn admin_has_credential_conn(
    conn: &mut Client,
    account_id: &str,
) -> Result<bool, String> {
    let row = conn
        .query_one(
            "SELECT EXISTS(SELECT 1 FROM admin_passkey_credentials WHERE account_id = $1)",
            &[&account_id],
        )
        .map_err(|e| format!("query passkey credential existence failed: {e}"))?;
    Ok(row.get(0))
}

/// 一次性消费断言令牌(校验 admin / 未过期 + 删除);成功返回 true。
pub(super) fn consume_assertion_conn(
    conn: &mut Client,
    assertion_id: &str,
    account_id: &str,
    now: DateTime<Utc>,
) -> Result<bool, String> {
    let row = conn
        .query_opt(
            "DELETE FROM admin_passkey_assertions
             WHERE assertion_id = $1 AND account_id = $2 AND expires_at > $3
             RETURNING assertion_id",
            &[&assertion_id, &account_id, &now],
        )
        .map_err(|e| format!("consume passkey assertion failed: {e}"))?;
    Ok(row.is_some())
}

/// 清理过期 ceremony 与断言令牌(begin / assert 时顺带调用)。
pub(super) fn cleanup_passkey_state_conn(
    conn: &mut Client,
    now: DateTime<Utc>,
) -> Result<(), String> {
    conn.execute(
        "DELETE FROM admin_passkey_ceremonies WHERE expires_at < $1",
        &[&now],
    )
    .map_err(|e| format!("cleanup passkey ceremonies failed: {e}"))?;
    conn.execute(
        "DELETE FROM admin_passkey_assertions WHERE expires_at < $1",
        &[&now],
    )
    .map_err(|e| format!("cleanup passkey assertions failed: {e}"))?;
    Ok(())
}
