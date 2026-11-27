//! 岗位激活：复用现有钱包持有证明，所选人员、岗位和真实绑定固定在一次性会话。
use crate::roles::{InstitutionRolesState, RoleAccess};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    io::ErrorKind,
    sync::{Mutex, OnceLock},
};
use tauri::AppHandle;
#[derive(Debug, Clone)]
struct AccessSession {
    access: RoleAccess,
    signer_public_key: String,
    payload: Vec<u8>,
    expires_at: u64,
}
static SESSIONS: OnceLock<Mutex<HashMap<String, AccessSession>>> = OnceLock::new();
fn sessions() -> &'static Mutex<HashMap<String, AccessSession>> {
    SESSIONS.get_or_init(|| Mutex::new(HashMap::new()))
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredAccess {
    accesses: Vec<RoleAccess>,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoleAccessRequest {
    pub request_json: String,
    pub request_id: String,
    pub expected_payload_hash: String,
    pub payload_hex: String,
}
use nodeclient::signing::now_secs;
fn path(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    Ok(nodedesktop::security::app_data_dir(app)?.join("role-access.json"))
}
fn load(app: &AppHandle) -> Result<Vec<RoleAccess>, String> {
    match std::fs::read_to_string(path(app)?) {
        Ok(raw) => serde_json::from_str::<StoredAccess>(&raw)
            .map(|s| s.accesses)
            .map_err(|e| format!("岗位激活记录损坏:{e}")),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(e.to_string()),
    }
}
fn save(app: &AppHandle, accesses: Vec<RoleAccess>) -> Result<(), String> {
    let raw = serde_json::to_string(&StoredAccess { accesses }).map_err(|e| e.to_string())?;
    nodedesktop::security::write_text_atomic_restricted(&path(app)?, &raw)
}
fn check_code(state: &InstitutionRolesState, expected: Option<&str>) -> Result<(), String> {
    if expected.is_some_and(|s| {
        primitives::cid::code::institution_code_from_str(s) != Some(state.institution_code)
    }) {
        return Err("机构码与任职所属机构不一致".into());
    }
    Ok(())
}
pub(crate) fn selected_access(
    state: &InstitutionRolesState,
    person: &str,
    role: &str,
    account: &str,
) -> Result<RoleAccess, String> {
    let a = state
        .current_assignment(person, role)
        .ok_or("所选人员没有当前有效任职或绑定")?;
    if a.account_id.as_deref() != Some(account) {
        return Err("签名钱包不是该人员当前绑定".into());
    }
    Ok(RoleAccess {
        cid_number: state.cid_number.clone(),
        person_cid: person.into(),
        role_code: role.into(),
        account_id: account.into(),
        binding_revision: a.binding_revision.clone().ok_or("人员绑定版本缺失")?,
        activated_at_ms: 0,
    })
}
#[tauri::command]
pub async fn build_role_access_request(
    app: AppHandle,
    signer_public_key: String,
    cid_number: String,
    person_cid: String,
    role_code: String,
    expected_institution_code: Option<String>,
) -> Result<RoleAccessRequest, String> {
    if !nodecontrol::identity::current_status(&app)?.running {
        return Err("节点未运行，不能核验当前岗位".into());
    }
    let public = nodedesktop::validation::normalize_public_key(&signer_public_key)?;
    let account = nodeclient::signing::signer_account_id_from_public_key(&public)?;
    let state = tauri::async_runtime::spawn_blocking(move || {
        crate::storage::fetch_roles(&cid_number)?.ok_or("机构不存在".to_string())
    })
    .await
    .map_err(|e| e.to_string())??;
    check_code(&state, expected_institution_code.as_deref())?;
    let access = selected_access(&state, &person_cid, &role_code, &account)?;
    let bytes: [u8; 32] = hex::decode(public.trim_start_matches("0x"))
        .map_err(|e| e.to_string())?
        .try_into()
        .map_err(|_| "钱包持有证明公钥长度无效")?;
    // 此处消费所属primitives现有固定协议，不另造二维码字段、签名域或钱包解码合同。
    let payload = primitives::sign::role_access_payload(
        state.cid_number.as_bytes(),
        &state.institution_code,
        state.kind,
        &bytes,
        now_secs()?,
        &rand::random(),
    )
    .ok_or("机构CID超出本机证明协议长度")?;
    let request_id = nodeclient::signing::generate_request_id_public("role-access");
    let expires_at = now_secs()? + nodeclient::signing::DEFAULT_TTL_SECS;
    let hash = format!(
        "0x{}",
        hex::encode(nodeclient::signing::sha256_hash_public(&payload))
    );
    let payload_hex = format!("0x{}", hex::encode(&payload));
    let request = nodeclient::signing::QrSignRequest {
        proto: nodeclient::signing::QR_V1.into(),
        kind: nodeclient::signing::QR_KIND_SIGN_REQUEST,
        id: request_id.clone(),
        expires_at,
        body: nodeclient::signing::SignRequestBody {
            action: primitives::sign::QR_ACTION_ROLE_ACCESS,
            sig_alg: 0,
            signer_public_key: nodeclient::signing::public_key_b64(&bytes)?,
            payload: nodeclient::signing::payload_b64(&payload),
        },
    };
    let mut items = sessions().lock().map_err(|_| "激活会话锁异常")?;
    let now = now_secs()?;
    items.retain(|_, s| s.expires_at >= now);
    if items.len() >= 256 {
        return Err("岗位激活请求已达上限".into());
    }
    items.insert(
        request_id.clone(),
        AccessSession {
            access,
            signer_public_key: public,
            payload,
            expires_at,
        },
    );
    Ok(RoleAccessRequest {
        request_json: serde_json::to_string(&request).map_err(|e| e.to_string())?,
        request_id,
        expected_payload_hash: hash,
        payload_hex,
    })
}
#[tauri::command]
pub async fn verify_role_access(
    app: AppHandle,
    request_id: String,
    signer_public_key: String,
    expected_payload_hash: String,
    payload_hex: String,
    response_json: String,
) -> Result<RoleAccess, String> {
    let session = sessions()
        .lock()
        .map_err(|_| "激活会话锁异常")?
        .remove(&request_id)
        .ok_or("激活会话不存在或已消费")?;
    let response: nodeclient::signing::QrSignResponse =
        serde_json::from_str(&response_json).map_err(|e| e.to_string())?;
    let public = nodedesktop::validation::normalize_public_key(&signer_public_key)?;
    let actual_public =
        nodedesktop::validation::normalize_public_key(&response.body.signer_public_key)?;
    if session.expires_at < now_secs()?
        || response.expires_at != session.expires_at
        || response.proto != nodeclient::signing::QR_V1
        || response.kind != nodeclient::signing::QR_KIND_SIGN_RESPONSE
        || response.id != request_id
        || public != session.signer_public_key
        || actual_public != public
    {
        return Err("激活响应与本机冻结请求不一致或已过期".into());
    }
    let hash = hex::encode(nodeclient::signing::sha256_hash_public(&session.payload));
    if expected_payload_hash.trim_start_matches("0x") != hash
        || payload_hex.trim_start_matches("0x") != hex::encode(&session.payload)
    {
        return Err("激活载荷或摘要与原会话不一致".into());
    }
    let account: [u8; 32] = hex::decode(session.access.account_id.trim_start_matches("0x"))
        .map_err(|e| e.to_string())?
        .try_into()
        .map_err(|_| "人员账户长度无效")?;
    if !nodeclient::signing::verify_message_signature(&account, &session.payload, &response.body)? {
        return Err("钱包持有证明签名无效".into());
    }
    if !nodecontrol::identity::current_status(&app)?.running {
        return Err("节点未运行，不能确认当前任职".into());
    }
    let access = session.access;
    let cid = access.cid_number.clone();
    let state = tauri::async_runtime::spawn_blocking(move || {
        crate::storage::fetch_roles(&cid)?.ok_or("机构不存在".to_string())
    })
    .await
    .map_err(|e| e.to_string())??;
    if !state.matches_access(&access) {
        return Err("签名期间人员绑定或岗位任职已变化".into());
    }
    if session.expires_at < now_secs()? {
        return Err("确认期间激活证明已过期".into());
    }
    let mut access = access;
    access.activated_at_ms = now_secs()?.checked_mul(1000).ok_or("时间超限")?;
    let mut records = load(&app)?;
    records.retain(|a| {
        !(a.cid_number == access.cid_number
            && a.role_code == access.role_code
            && a.person_cid == access.person_cid)
    });
    records.push(access.clone());
    save(&app, records)?;
    nodedesktop::security::append_audit_log(&app, "role_access", "granted")?;
    Ok(access)
}
#[tauri::command]
pub async fn get_role_access(
    app: AppHandle,
    cid_number: String,
    expected_institution_code: Option<String>,
) -> Result<Vec<RoleAccess>, String> {
    if !nodecontrol::identity::current_status(&app)?.running {
        return Err("节点未运行，不能读取有效岗位激活".into());
    }
    let records = load(&app)?;
    let state =
        tauri::async_runtime::spawn_blocking(move || crate::storage::fetch_roles(&cid_number))
            .await
            .map_err(|e| e.to_string())??;
    let Some(state) = state else {
        return Ok(Vec::new());
    };
    check_code(&state, expected_institution_code.as_deref())?;
    Ok(records
        .into_iter()
        .filter(|a| state.matches_access(a))
        .collect())
}
#[tauri::command]
pub fn revoke_role_access(
    app: AppHandle,
    cid_number: String,
    person_cid: String,
    role_code: String,
    unlock_password: String,
) -> Result<(), String> {
    let unlock = nodedesktop::security::ensure_unlock_password(&unlock_password)?;
    nodecontrol::password::verify_device_login_password(&app, unlock)?;
    let mut records = load(&app)?;
    let before = records.len();
    records.retain(|a| {
        !(a.cid_number == cid_number && a.person_cid == person_cid && a.role_code == role_code)
    });
    if before == records.len() {
        return Err("岗位激活不存在".into());
    }
    save(&app, records)?;
    nodedesktop::security::append_audit_log(&app, "role_access", "revoked")
}
#[tauri::command]
pub async fn has_role_access(app: AppHandle) -> Result<bool, String> {
    if !nodecontrol::identity::current_status(&app)?.running {
        return Ok(false);
    }
    let records = load(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        let mut states = HashMap::new();
        for a in records {
            if !states.contains_key(&a.cid_number) {
                states.insert(
                    a.cid_number.clone(),
                    crate::storage::fetch_roles(&a.cid_number)?,
                );
            }
            if states[&a.cid_number]
                .as_ref()
                .is_some_and(|s| s.matches_access(&a))
            {
                return Ok(true);
            }
        }
        Ok(false)
    })
    .await
    .map_err(|e| e.to_string())?
}
