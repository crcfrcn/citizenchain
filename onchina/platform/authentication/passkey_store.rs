//! Passkey凭证绑定真实人员和identity绑定；仪式及一次性断言同时锁定当前机构岗位。
use crate::authentication::login::AdminAuthContext;
use chrono::{DateTime, Utc};
use postgres::GenericClient;
use webauthn_rs::prelude::Passkey;
pub(super) fn insert_credential_conn(
    conn: &mut impl GenericClient,
    id: &str,
    subject: &AdminAuthContext,
    passkey: &Passkey,
) -> Result<(), String> {
    let data = serde_json::to_value(passkey).map_err(|e| e.to_string())?;
    let count=conn.execute("INSERT INTO credential(credential_id,person_cid,binding_revision,account_id,passkey_data,created_at) VALUES($1,$2,$3,$4,$5,now()) ON CONFLICT(credential_id) DO UPDATE SET passkey_data=EXCLUDED.passkey_data WHERE credential.person_cid=EXCLUDED.person_cid AND credential.binding_revision=EXCLUDED.binding_revision AND credential.account_id=EXCLUDED.account_id",&[&id,&subject.person_cid,&subject.binding_revision,&subject.account_id,&data]).map_err(|e|e.to_string())?;
    if count != 1 {
        return Err("http:forbidden:credential belongs to another person binding".into());
    }
    Ok(())
}
pub(super) fn list_credentials_conn(
    conn: &mut impl GenericClient,
    subject: &AdminAuthContext,
) -> Result<Vec<Passkey>, String> {
    conn.query("SELECT passkey_data FROM credential WHERE person_cid=$1 AND binding_revision=$2 AND account_id=$3",&[&subject.person_cid,&subject.binding_revision,&subject.account_id]).map_err(|e|e.to_string())?.iter().map(|r|serde_json::from_value(r.get(0)).map_err(|e|e.to_string())).collect()
}
pub(super) fn update_credential_conn(
    conn: &mut impl GenericClient,
    id: &str,
    subject: &AdminAuthContext,
    passkey: &Passkey,
) -> Result<(), String> {
    let data = serde_json::to_value(passkey).map_err(|e| e.to_string())?;
    conn.execute("UPDATE credential SET passkey_data=$2 WHERE credential_id=$1 AND person_cid=$3 AND binding_revision=$4 AND account_id=$5",&[&id,&data,&subject.person_cid,&subject.binding_revision,&subject.account_id]).map_err(|e|e.to_string())?;
    Ok(())
}
pub(super) fn insert_ceremony_conn(
    conn: &mut impl GenericClient,
    id: &str,
    subject: &AdminAuthContext,
    kind: &str,
    state: &serde_json::Value,
    expires: DateTime<Utc>,
) -> Result<(), String> {
    conn.execute("INSERT INTO ceremony(ceremony_id,person_cid,binding_revision,account_id,institution_cid,role_code,record_kind,state_data,expires_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)",&[&id,&subject.person_cid,&subject.binding_revision,&subject.account_id,&subject.institution_cid_number,&subject.role_code,&kind,state,&expires]).map_err(|e|e.to_string())?;
    Ok(())
}
pub(super) fn take_ceremony_conn(
    conn: &mut impl GenericClient,
    id: &str,
    subject: &AdminAuthContext,
    kind: &str,
    now: DateTime<Utc>,
) -> Result<Option<serde_json::Value>, String> {
    conn.query_opt("DELETE FROM ceremony WHERE ceremony_id=$1 AND person_cid=$2 AND binding_revision=$3 AND account_id=$4 AND institution_cid=$5 AND role_code=$6 AND record_kind=$7 AND expires_at>$8 RETURNING state_data",&[&id,&subject.person_cid,&subject.binding_revision,&subject.account_id,&subject.institution_cid_number,&subject.role_code,&kind,&now]).map(|r|r.map(|r|r.get(0))).map_err(|e|e.to_string())
}
pub(super) fn insert_assertion_conn(
    conn: &mut impl GenericClient,
    id: &str,
    subject: &AdminAuthContext,
    expires: DateTime<Utc>,
) -> Result<(), String> {
    conn.execute("INSERT INTO assertion(assertion_id,person_cid,binding_revision,account_id,institution_cid,role_code,expires_at) VALUES($1,$2,$3,$4,$5,$6,$7)",&[&id,&subject.person_cid,&subject.binding_revision,&subject.account_id,&subject.institution_cid_number,&subject.role_code,&expires]).map_err(|e|e.to_string())?;
    Ok(())
}
pub(super) fn has_credential_conn(
    conn: &mut impl GenericClient,
    subject: &AdminAuthContext,
) -> Result<bool, String> {
    conn.query_one("SELECT EXISTS(SELECT 1 FROM credential WHERE person_cid=$1 AND binding_revision=$2 AND account_id=$3)",&[&subject.person_cid,&subject.binding_revision,&subject.account_id]).map(|r|r.get(0)).map_err(|e|e.to_string())
}
/// 只消费当前人员绑定和所选机构岗位的断言；事务回滚会保留它。
pub(super) fn consume_assertion_conn(
    conn: &mut impl GenericClient,
    id: &str,
    subject: &AdminAuthContext,
    now: DateTime<Utc>,
) -> Result<bool, String> {
    conn.query_opt("DELETE FROM assertion WHERE assertion_id=$1 AND person_cid=$2 AND binding_revision=$3 AND account_id=$4 AND institution_cid=$5 AND role_code=$6 AND expires_at>$7 RETURNING assertion_id",&[&id,&subject.person_cid,&subject.binding_revision,&subject.account_id,&subject.institution_cid_number,&subject.role_code,&now]).map(|r|r.is_some()).map_err(|e|e.to_string())
}
pub(super) fn cleanup_passkey_state_conn(
    conn: &mut impl GenericClient,
    now: DateTime<Utc>,
) -> Result<(), String> {
    conn.execute("DELETE FROM ceremony WHERE expires_at<$1", &[&now])
        .map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM assertion WHERE expires_at<$1", &[&now])
        .map_err(|e| e.to_string())?;
    Ok(())
}
