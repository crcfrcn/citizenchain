//! 公民身份上链准备 handler。
//!
//! 本模块只处理公民账户签名与 `citizen-identity` call data 构造。
//! 本地建档不要求链账户；注册局准备推送链上身份时才绑定账户并验签。

// 身份准备辅助函数必须原样返回统一 Axum 拒绝响应，禁止不同入口各自改写错误语义。
#![allow(clippy::result_large_err)]

use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use chrono::{Datelike, Duration, NaiveDate, Utc};
use codec::{Compact, Encode};
use serde::{Deserialize, Serialize};
use sp_core::{sr25519, Pair};
use uuid::Uuid;

use crate::crypto::pubkey::same_account_id;
use crate::domains::citizens::admin_entry::{
    citizen_record_from_row, resolve_citizen_account, ResolvedCitizenAccount,
};
use crate::*;

/// 身份写入授权的有效期（秒，按链上时间推算）；
/// 不得超过链端 `MAX_CID_AUTHORIZATION_LIFETIME_SECS`（600 秒）。
const CITIZEN_IDENTITY_AUTHORIZATION_LIFETIME_SECS: u64 = 180;

const CITIZEN_IDENTITY_PALLET_INDEX: u8 = 10;
const REGISTER_VOTING_IDENTITY_CALL_INDEX: u8 = 0;
const UPGRADE_TO_CANDIDATE_IDENTITY_CALL_INDEX: u8 = 1;
const MIN_ONCHAIN_CITIZEN_AGE_YEARS: u8 = 16;

#[derive(Clone, Copy, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CitizenOnchainIdentityLevel {
    Voting,
    Candidate,
}

impl CitizenOnchainIdentityLevel {
    fn as_str(self) -> &'static str {
        match self {
            CitizenOnchainIdentityLevel::Voting => "voting",
            CitizenOnchainIdentityLevel::Candidate => "candidate",
        }
    }

    fn call_index(self) -> u8 {
        match self {
            CitizenOnchainIdentityLevel::Voting => REGISTER_VOTING_IDENTITY_CALL_INDEX,
            CitizenOnchainIdentityLevel::Candidate => UPGRADE_TO_CANDIDATE_IDENTITY_CALL_INDEX,
        }
    }
}

#[derive(Deserialize)]
pub(crate) struct PrepareCitizenOnchainInput {
    pub(crate) account_id: String,
    pub(crate) actor_role_code: String,
    pub(crate) identity_level: CitizenOnchainIdentityLevel,
}

#[derive(Serialize)]
pub(crate) struct PrepareCitizenOnchainOutput {
    pub(crate) cid_number: String,
    pub(crate) actor_role_code: String,
    pub(crate) identity_level: CitizenOnchainIdentityLevel,
    pub(crate) account_id: String,
    pub(crate) ss58_address: String,
    pub(crate) payload_hex: String,
    pub(crate) sign_request: String,
    pub(crate) action_label_zh: String,
    pub(crate) expires_at: i64,
}

#[derive(Deserialize)]
pub(crate) struct CompleteCitizenOnchainInput {
    pub(crate) account_id: String,
    pub(crate) actor_role_code: String,
    pub(crate) identity_level: CitizenOnchainIdentityLevel,
    pub(crate) sign_response: String,
}

#[derive(Serialize)]
pub(crate) struct CompleteCitizenOnchainOutput {
    pub(crate) request_id: String,
    pub(crate) cid_number: String,
    pub(crate) actor_role_code: String,
    pub(crate) identity_level: CitizenOnchainIdentityLevel,
    pub(crate) account_id: String,
    pub(crate) ss58_address: String,
    pub(crate) chain_action: u16,
    pub(crate) call_data_hex: String,
    pub(crate) citizen_signature: String,
    pub(crate) citizen_identity_chain_sign_request: String,
}

/// 所有注册局都可查询的链上公开 CID 绑定，不包含任何链下公民档案。
#[derive(Serialize)]
pub(crate) struct FinalizedCitizenBindingOutput {
    pub(crate) cid_number: String,
    pub(crate) cid_status: String,
    pub(crate) binding_active: bool,
    pub(crate) account_id: Option<String>,
    pub(crate) ss58_address: Option<String>,
    pub(crate) binding_revision: Option<u64>,
    pub(crate) voting_identity: bool,
    pub(crate) candidate_identity: bool,
    pub(crate) registry_rebind_required: bool,
    pub(crate) registrar_cid_number: Option<String>,
    pub(crate) finalized_block_number: u32,
    pub(crate) finalized_block_hash: String,
    pub(crate) genesis_hash: String,
}

pub(crate) async fn finalized_citizen_binding(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(cid_number): Path<String>,
) -> impl IntoResponse {
    let ctx = match require_admin_any(&state, &headers) {
        Ok(value) => value,
        Err(response) => return response,
    };
    if let Err(response) = ensure_registry_admin(&ctx) {
        return response;
    }
    let cid_number = cid_number.trim().to_string();
    if crate::cid::validate_cid_number_format(cid_number.as_str()).is_err() {
        return api_error(StatusCode::BAD_REQUEST, 1001, "CID 格式错误");
    }
    let snapshot = match crate::core::chain_citizen_identity::read_finalized_citizen_identity(
        cid_number.as_str(),
    )
    .await
    {
        Ok(value) => value,
        Err(err) => {
            tracing::error!(cid_number = %cid_number, error = %err, "read finalized citizen binding failed");
            return api_error(StatusCode::BAD_GATEWAY, 2004, "finalized 公民绑定读取失败");
        }
    };
    if snapshot.is_unoccupied() {
        return api_error(StatusCode::NOT_FOUND, 1004, "链上 CID 不存在");
    }
    // Revoked 记录可在链上保留最后账户映射供审计，但接口中的 account_id 只表达
    // 当前有效控制钱包；吊销后必须为空，禁止把旧账户继续展示成当前授权。
    let active_binding = snapshot.active_binding();
    let binding_active = active_binding.is_some();
    let account_id = active_binding.map(|(account_id, _)| format!("0x{}", hex::encode(account_id)));
    let ss58_address = account_id
        .as_deref()
        .and_then(crate::crypto::pubkey::account_id_to_ss58);
    Json(ApiResponse {
        code: 0,
        message: "ok".to_string(),
        data: FinalizedCitizenBindingOutput {
            cid_number,
            cid_status: snapshot.cid_status.as_str().to_string(),
            binding_active,
            account_id,
            ss58_address,
            binding_revision: snapshot.binding_revision,
            voting_identity: snapshot.voting.is_some(),
            candidate_identity: snapshot.candidate.is_some(),
            registry_rebind_required: snapshot.registry_rebind_required(),
            registrar_cid_number: snapshot
                .registrar_cid_number
                .map(|value| String::from_utf8_lossy(&value).into_owned()),
            finalized_block_number: snapshot.finalized_block_number,
            finalized_block_hash: format!("0x{}", hex::encode(snapshot.finalized_block_hash)),
            genesis_hash: format!("0x{}", hex::encode(snapshot.genesis_hash)),
        },
    })
    .into_response()
}

pub(crate) async fn prepare_citizen_onchain_signature(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(cid_number): Path<String>,
    Json(input): Json<PrepareCitizenOnchainInput>,
) -> impl IntoResponse {
    let ctx = match require_admin_any(&state, &headers) {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    if let Err(resp) = ensure_registry_admin(&ctx) {
        return resp;
    }
    // 本步只构造待公民签名的载荷，不产生 extrinsic、不建冷签会话，属 Session 档。
    // 整个业务操作的那一次 Passkey 消费在 `complete_citizen_onchain_signature`
    // ——即真正创建冷签会话、授权链上写的那一步，保持「一次操作一次 passkey」。
    let citizen_account = match resolve_citizen_account(input.account_id.as_str()) {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let actor_role_code = match validate_actor_role_code(input.actor_role_code.as_str()) {
        Ok(value) => value,
        Err(resp) => return resp,
    };
    let record = match state.db.find_citizen_by_cid(cid_number.as_str()) {
        Ok(Some(v)) => v,
        Ok(None) => return api_error(StatusCode::NOT_FOUND, 1004, "公民档案不存在"),
        Err(err) => {
            tracing::error!(error = %err, "query citizen by cid failed");
            return api_error(StatusCode::INTERNAL_SERVER_ERROR, 1004, "公民档案查询失败");
        }
    };
    if let Err(resp) = ensure_record_in_admin_scope(&ctx, &record) {
        return resp;
    }
    if let Err(resp) = ensure_account_available(&state, &record, &citizen_account) {
        return resp;
    }
    let payload =
        match build_citizen_identity_payload(&record, &citizen_account, input.identity_level) {
            Ok(v) => v,
            Err(resp) => return resp,
        };
    // 创世哈希与身份版本取自同一 finalized 快照；有效期按链上时间推算，不用本机时间。
    let snapshot = match crate::core::chain_citizen_identity::read_finalized_citizen_identity(
        record.cid_number.as_str(),
    )
    .await
    {
        Ok(value) => value,
        Err(err) => {
            tracing::error!(cid_number = %record.cid_number, error = %err, "read finalized citizen identity failed");
            return api_error(StatusCode::BAD_GATEWAY, 2004, "finalized 公民身份读取失败");
        }
    };
    let authorization_expires_at = snapshot
        .chain_now_seconds
        .saturating_add(CITIZEN_IDENTITY_AUTHORIZATION_LIFETIME_SECS);
    let authorization_bytes = build_citizen_identity_authorization_bytes(
        &snapshot.genesis_hash,
        &payload.payload_bytes,
        snapshot.identity_version,
        authorization_expires_at,
    );
    let issued_at = Utc::now();
    let expires_at = issued_at + Duration::seconds(180);
    let request_id = format!("citizen-identity-{}", Uuid::new_v4());
    let sign_request = match crate::core::qr::build_sign_request_bytes(
        request_id.as_str(),
        issued_at.timestamp(),
        expires_at.timestamp(),
        citizen_account.account_id.as_str(),
        &authorization_bytes,
        crate::core::qr::action_citizen_identity(),
    ) {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let operation = CitizenOnchainOperation {
        operation_id: request_id,
        registrar_account_id: ctx.account_id,
        institution_code: ctx.institution_code,
        actor_role_code: actor_role_code.clone(),
        cid_number: record.cid_number.clone(),
        citizen_account_id: citizen_account.account_id.clone(),
        identity_level: payload.identity_level.as_str().to_string(),
        payload_hex: hex::encode(&payload.payload_bytes),
        expected_identity_version: snapshot.identity_version,
        authorization_expires_at,
        expires_at,
    };
    if let Err(err) = state.db.insert_citizen_onchain_operation(&operation) {
        tracing::error!(error = %err, "insert citizen onchain operation failed");
        return api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            1004,
            "公民签名操作落库失败",
        );
    }

    Json(ApiResponse {
        code: 0,
        message: "ok".to_string(),
        data: PrepareCitizenOnchainOutput {
            cid_number: record.cid_number,
            actor_role_code,
            identity_level: payload.identity_level,
            account_id: citizen_account.account_id,
            ss58_address: citizen_account.ss58_address,
            payload_hex: format!("0x{}", hex::encode(authorization_bytes)),
            sign_request,
            action_label_zh: crate::core::qr::action_label_zh("citizen_identity"),
            expires_at: expires_at.timestamp(),
        },
    })
    .into_response()
}

pub(crate) async fn complete_citizen_onchain_signature(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(cid_number): Path<String>,
    Json(input): Json<CompleteCitizenOnchainInput>,
) -> impl IntoResponse {
    let ctx = match require_admin_any(&state, &headers) {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    if let Err(resp) = ensure_registry_admin(&ctx) {
        return resp;
    }
    // 链上写(PasskeyColdSign 档):本步凭公民回签创建管理员冷签会话，是整个
    // 公民上链操作真正被授权的那一刻，故这里消费本次操作唯一的一次 Passkey。
    let passkey = match crate::auth::passkey::require_passkey_assertion(
        &state,
        &headers,
        ctx.account_id.as_str(),
    ) {
        Ok(proof) => proof,
        Err(resp) => return resp,
    };
    let citizen_account = match resolve_citizen_account(input.account_id.as_str()) {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let actor_role_code = match validate_actor_role_code(input.actor_role_code.as_str()) {
        Ok(value) => value,
        Err(resp) => return resp,
    };
    let record = match state.db.find_citizen_by_cid(cid_number.as_str()) {
        Ok(Some(v)) => v,
        Ok(None) => return api_error(StatusCode::NOT_FOUND, 1004, "公民档案不存在"),
        Err(err) => {
            tracing::error!(error = %err, "query citizen by cid failed");
            return api_error(StatusCode::INTERNAL_SERVER_ERROR, 1004, "公民档案查询失败");
        }
    };
    if let Err(resp) = ensure_record_in_admin_scope(&ctx, &record) {
        return resp;
    }
    if let Err(resp) = ensure_account_available(&state, &record, &citizen_account) {
        return resp;
    }
    let payload =
        match build_citizen_identity_payload(&record, &citizen_account, input.identity_level) {
            Ok(v) => v,
            Err(resp) => return resp,
        };
    let sign_response = match crate::core::qr::parse_sign_response(input.sign_response.as_str()) {
        Ok(v) => v,
        Err(err) => {
            let detail = format!("公民钱包签名响应无效: {err}");
            return api_error(StatusCode::BAD_REQUEST, 1001, detail.as_str());
        }
    };
    let operation_id = match sign_response
        .id
        .as_deref()
        .map(str::trim)
        .filter(|id| !id.is_empty())
    {
        Some(value) => value,
        None => return api_error(StatusCode::BAD_REQUEST, 1001, "公民签名响应缺少操作编号"),
    };
    let operation = match state.db.find_citizen_onchain_operation(operation_id) {
        Ok(Some(value)) => value,
        Ok(None) => return api_error(StatusCode::GONE, 2003, "公民签名操作不存在或已失效"),
        Err(err) => {
            tracing::error!(error = %err, "query citizen onchain operation failed");
            return api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                1004,
                "公民签名操作查询失败",
            );
        }
    };
    if operation.registrar_account_id != ctx.account_id
        || operation.institution_code != ctx.institution_code
        || operation.actor_role_code != actor_role_code
        || operation.cid_number != cid_number
        || operation.citizen_account_id != citizen_account.account_id
        || operation.identity_level != input.identity_level.as_str()
        || operation.payload_hex != hex::encode(&payload.payload_bytes)
    {
        return api_error(
            StatusCode::FORBIDDEN,
            1003,
            "公民签名响应与当前业务操作不一致",
        );
    }
    let account_id = sign_response.body.account_id;
    if !same_account_id(account_id.as_str(), citizen_account.account_id.as_str()) {
        return api_error(StatusCode::FORBIDDEN, 1003, "签名钱包与录入钱包不一致");
    }
    let citizen_signature = sign_response.body.signature;
    // 身份版本与有效期取 prepare 阶段落库的值，不在此重新推算。
    let snapshot = match crate::core::chain_citizen_identity::read_finalized_citizen_identity(
        cid_number.as_str(),
    )
    .await
    {
        Ok(value) => value,
        Err(err) => {
            tracing::error!(cid_number = %cid_number, error = %err, "read finalized citizen identity failed");
            return api_error(StatusCode::BAD_GATEWAY, 2004, "finalized 公民身份读取失败");
        }
    };
    let authorization_bytes = build_citizen_identity_authorization_bytes(
        &snapshot.genesis_hash,
        &payload.payload_bytes,
        operation.expected_identity_version,
        operation.authorization_expires_at,
    );
    if !verify_citizen_identity_signature(
        citizen_account.account_id.as_str(),
        &authorization_bytes,
        citizen_signature.as_str(),
    ) {
        return api_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            2004,
            "公民钱包签名校验失败",
        );
    }
    match state.db.consume_citizen_onchain_operation(operation_id) {
        Ok(true) => {}
        Ok(false) => return api_error(StatusCode::CONFLICT, 2003, "公民签名操作已消费或已过期"),
        Err(err) => {
            tracing::error!(error = %err, "consume citizen onchain operation failed");
            return api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                1004,
                "公民签名操作消费失败",
            );
        }
    }

    let actor_cid_number = match active_registry_cid_number(&state) {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let signature_bytes = match parse_signature_bytes(citizen_signature.as_str()) {
        Some(v) => v,
        None => return api_error(StatusCode::BAD_REQUEST, 1001, "公民签名格式错误"),
    };
    let call = encode_citizen_identity_call(
        payload.identity_level,
        &actor_cid_number,
        actor_role_code.as_str(),
        &payload.payload_bytes,
        operation.expected_identity_version,
        operation.authorization_expires_at,
        &signature_bytes,
    );
    let action = crate::core::institution_call::chain_action_code(
        CITIZEN_IDENTITY_PALLET_INDEX,
        payload.identity_level.call_index(),
    );
    // D7:QR 载荷 = 完整 runtime 签名载荷(与钱包解码器扩展尾规则对齐),
    // CitizenWallet 只签名一次并显示响应二维码；OnChina 回扫后经
    // /api/admin/chain/submit 统一组装和提交。
    let prepared =
        match crate::core::chain_submit::prepare_signing(&call, ctx.account_id.as_str()).await {
            Ok(v) => v,
            Err(err) => {
                tracing::error!(error = %err, "prepare identity push signing failed");
                return api_error(
                    StatusCode::BAD_GATEWAY,
                    1004,
                    "链签名载荷准备失败(链不可用)",
                );
            }
        };
    let issued_at = Utc::now();
    let expires_at = issued_at + Duration::seconds(600);
    let request_id = format!("citizen-chain-{}", Uuid::new_v4());
    let chain_sign_request = match crate::core::qr::build_sign_request_bytes(
        request_id.as_str(),
        issued_at.timestamp(),
        expires_at.timestamp(),
        ctx.account_id.as_str(),
        &prepared.payload,
        action,
    ) {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let session = crate::domains::citizens::occupy::ChainSignSession {
        request_id: request_id.clone(),
        purpose: crate::domains::citizens::occupy::PURPOSE_CITIZEN_IDENTITY_PUSH.to_string(),
        account_id: ctx.account_id.clone(),
        call_data: call.clone(),
        nonce: prepared.nonce,
        signing_hash: prepared.signing_hash_hex.clone(),
        context: serde_json::json!({
            "cid_number": record.cid_number,
            "identity_level": payload.identity_level.as_str(),
            "actor_role_code": actor_role_code.clone(),
            "citizen_account_id": citizen_account.account_id,
            "ss58_address": citizen_account.ss58_address,
        }),
        expires_at,
        consumed_at: None,
    };
    if let Err(err) = state.db.insert_chain_sign_session(&session, &passkey) {
        tracing::error!(error = %err, "insert identity push session failed");
        return api_error(StatusCode::INTERNAL_SERVER_ERROR, 1004, "冷签会话落库失败");
    }

    Json(ApiResponse {
        code: 0,
        message: "ok".to_string(),
        data: CompleteCitizenOnchainOutput {
            request_id,
            cid_number: record.cid_number,
            actor_role_code,
            identity_level: payload.identity_level,
            account_id: citizen_account.account_id,
            ss58_address: citizen_account.ss58_address,
            chain_action: action,
            call_data_hex: format!("0x{}", hex::encode(call)),
            citizen_signature,
            citizen_identity_chain_sign_request: chain_sign_request,
        },
    })
    .into_response()
}

#[derive(Clone)]
struct CitizenOnchainOperation {
    operation_id: String,
    registrar_account_id: String,
    institution_code: String,
    actor_role_code: String,
    cid_number: String,
    citizen_account_id: String,
    identity_level: String,
    payload_hex: String,
    /// 公民签名时链上该 CID 的身份版本。
    expected_identity_version: u64,
    /// 链上授权有效期（Unix 秒）；与公民签名覆盖的值必须一致。
    authorization_expires_at: u64,
    expires_at: chrono::DateTime<Utc>,
}

impl Db {
    fn insert_citizen_onchain_operation(
        &self,
        operation: &CitizenOnchainOperation,
    ) -> Result<(), String> {
        let operation = operation.clone();
        self.with_client(move |conn| {
            conn.execute(
                "DELETE FROM citizen_onchain_operations WHERE expires_at < now()",
                &[],
            )
            .map_err(|e| format!("delete expired citizen onchain operations failed: {e}"))?;
            conn.execute(
                "INSERT INTO citizen_onchain_operations
                 (operation_id, registrar_account_id, institution_code, actor_role_code, cid_number,
                  citizen_account_id, identity_level, payload_hex,
                  expected_identity_version, authorization_expires_at, expires_at)
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)",
                &[
                    &operation.operation_id,
                    &operation.registrar_account_id,
                    &operation.institution_code,
                    &operation.actor_role_code,
                    &operation.cid_number,
                    &operation.citizen_account_id,
                    &operation.identity_level,
                    &operation.payload_hex,
                    &(operation.expected_identity_version as i64),
                    &(operation.authorization_expires_at as i64),
                    &operation.expires_at,
                ],
            )
            .map_err(|e| format!("insert citizen onchain operation failed: {e}"))?;
            Ok(())
        })
    }

    fn find_citizen_onchain_operation(
        &self,
        operation_id: &str,
    ) -> Result<Option<CitizenOnchainOperation>, String> {
        let operation_id = operation_id.to_string();
        self.with_client(move |conn| {
            let row = conn
                .query_opt(
                    "SELECT operation_id, registrar_account_id, institution_code, actor_role_code,
                        cid_number, citizen_account_id, identity_level, payload_hex,
                        expected_identity_version, authorization_expires_at, expires_at
                 FROM citizen_onchain_operations
                 WHERE operation_id = $1 AND citizen_signed_at IS NULL AND expires_at >= now()",
                    &[&operation_id],
                )
                .map_err(|e| format!("query citizen onchain operation failed: {e}"))?;
            Ok(row.map(|row| CitizenOnchainOperation {
                operation_id: row.get(0),
                registrar_account_id: row.get(1),
                institution_code: row.get(2),
                actor_role_code: row.get(3),
                cid_number: row.get(4),
                citizen_account_id: row.get(5),
                identity_level: row.get(6),
                payload_hex: row.get(7),
                expected_identity_version: row.get::<_, i64>(8) as u64,
                authorization_expires_at: row.get::<_, i64>(9) as u64,
                expires_at: row.get(10),
            }))
        })
    }

    fn consume_citizen_onchain_operation(&self, operation_id: &str) -> Result<bool, String> {
        let operation_id = operation_id.to_string();
        self.with_client(move |conn| {
            conn.execute(
                "UPDATE citizen_onchain_operations SET citizen_signed_at = now()
             WHERE operation_id = $1 AND citizen_signed_at IS NULL AND expires_at >= now()",
                &[&operation_id],
            )
            .map(|count| count == 1)
            .map_err(|e| format!("consume citizen onchain operation failed: {e}"))
        })
    }

    pub(crate) fn find_citizen_by_cid(
        &self,
        cid_number: &str,
    ) -> Result<Option<CitizenRecord>, String> {
        let cid_number = cid_number.trim().to_string();
        if cid_number.is_empty() {
            return Ok(None);
        }
        self.with_client(move |conn| {
            let row = conn
                .query_opt(
                    "SELECT COALESCE(id, 0), cid_number, passport_no, family_name,
                            given_name, citizen_sex, citizen_birth_date, account_id,
                            binding_revision, binding_finalized_block_number,
                            binding_finalized_block_hash, citizen_status, voting_eligible,
                            passport_valid_from, passport_valid_until, status_updated_at,
                            province_code, city_code, town_code,
                            birth_province_code, birth_city_code, birth_town_code,
                            archive_hash, onchain_tx_hash, onchain_block_number, onchain_at,
                            creator_account_id, created_at, updater_account_id, updated_at
                     FROM citizens
                     WHERE cid_number = $1
                     ORDER BY created_at DESC
                     LIMIT 1",
                    &[&cid_number],
                )
                .map_err(|e| format!("query citizen by cid failed: {e}"))?;
            Ok(row.as_ref().map(citizen_record_from_row))
        })
    }
}

struct CitizenIdentityPayloadBytes {
    /// extrinsic 的 payload 参数本体（不含防重放包装）。
    payload_bytes: Vec<u8>,
    identity_level: CitizenOnchainIdentityLevel,
}

/// 公民签名覆盖的完整字节：`genesis_hash ++ payload ++ version ++ expires_at`；
/// 字段序与链端 `CitizenIdentityAuthorization` 严格一致。
fn build_citizen_identity_authorization_bytes(
    genesis_hash: &[u8; 32],
    payload_bytes: &[u8],
    expected_identity_version: u64,
    authorization_expires_at: u64,
) -> Vec<u8> {
    let mut out = Vec::with_capacity(32 + payload_bytes.len() + 16);
    out.extend_from_slice(genesis_hash);
    out.extend_from_slice(payload_bytes);
    out.extend(expected_identity_version.to_le_bytes());
    out.extend(authorization_expires_at.to_le_bytes());
    out
}

pub(crate) fn ensure_registry_admin(
    ctx: &crate::auth::login::AdminAuthContext,
) -> Result<(), axum::response::Response> {
    if crate::core::chain_runtime::is_tier1_registry(&ctx.institution_code)
        || crate::core::chain_runtime::is_subordinate_registry(&ctx.institution_code)
    {
        Ok(())
    } else {
        Err(api_error(
            StatusCode::FORBIDDEN,
            1003,
            "只有注册局管理员可以推送公民身份上链",
        ))
    }
}

pub(crate) fn ensure_record_in_admin_scope(
    ctx: &crate::auth::login::AdminAuthContext,
    record: &CitizenRecord,
) -> Result<(), axum::response::Response> {
    let scope = crate::scope::get_visible_scope(ctx);
    let province_name =
        crate::cid::china::area_name_by_codes(record.province_code.as_str(), None, None)
            .map(|(province, _, _)| province.to_string())
            .unwrap_or_default();
    let city_name = crate::cid::china::area_name_by_codes(
        record.province_code.as_str(),
        Some(record.city_code.as_str()),
        None,
    )
    .and_then(|(_, city, _)| city.map(str::to_string))
    .unwrap_or_default();
    if !scope.includes_province(province_name.as_str()) || !scope.includes_city(city_name.as_str())
    {
        return Err(api_error(
            StatusCode::FORBIDDEN,
            1003,
            "公民档案不在当前注册局办理范围内",
        ));
    }
    Ok(())
}

fn ensure_account_available(
    state: &AppState,
    record: &CitizenRecord,
    citizen_account: &ResolvedCitizenAccount,
) -> Result<(), axum::response::Response> {
    if let Some(existing) = record.account_id.as_deref() {
        if !same_account_id(existing, citizen_account.account_id.as_str()) {
            return Err(api_error(
                StatusCode::CONFLICT,
                1005,
                "该公民已绑定其他链账户",
            ));
        }
    }
    match state
        .db
        .find_citizen_by_account_id(citizen_account.account_id.as_str())
    {
        Ok(Some(existing)) if existing.cid_number != record.cid_number => Err(api_error(
            StatusCode::CONFLICT,
            1005,
            "该链账户已绑定其他公民档案",
        )),
        Ok(_) => Ok(()),
        Err(err) => {
            tracing::error!(error = %err, "query citizen account duplicate failed");
            Err(api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                1004,
                "链账户查重失败",
            ))
        }
    }
}

fn build_voting_identity_payload(
    record: &CitizenRecord,
    citizen_account: &ResolvedCitizenAccount,
) -> Result<CitizenIdentityPayloadBytes, axum::response::Response> {
    if record.citizen_status != CitizenStatus::Normal
        || record.computed_identity_status() != CitizenStatus::Normal
    {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            1001,
            "只有有效公民档案可以推送上链",
        ));
    }
    if !record.voting_eligible {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            1001,
            "无选举资格的公民不能推送链上投票身份",
        ));
    }
    let birth_date = NaiveDate::parse_from_str(record.citizen_birth_date.as_str(), "%Y-%m-%d")
        .map_err(|_| api_error(StatusCode::BAD_REQUEST, 1001, "公民出生日期格式错误"))?;
    // BFF 侧防误推门:未满 16 周岁不推送链上身份。年龄不入链载荷,链上不存/不算年龄;
    // 竞选身份的最小年龄由链端按 birth_date 复核,投票身份靠状态+护照有效期窗口判定能否投票。
    let age = citizen_age_years(Utc::now().date_naive(), birth_date);
    if age < MIN_ONCHAIN_CITIZEN_AGE_YEARS {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            1001,
            "未满16周岁不能推送链上身份",
        ));
    }
    let account_id = parse_account_id_bytes(citizen_account.account_id.as_str())
        .ok_or_else(|| api_error(StatusCode::BAD_REQUEST, 1001, "account_id 格式错误"))?;
    let valid_from = passport_date_u32(record.passport_valid_from.as_str(), "passport_valid_from")?;
    let valid_until =
        passport_date_u32(record.passport_valid_until.as_str(), "passport_valid_until")?;

    let mut out = Vec::new();
    append_bounded_bytes(&mut out, record.cid_number.as_bytes(), 32, "cid_number")?;
    out.extend_from_slice(&account_id);
    out.extend(valid_from.to_le_bytes());
    out.extend(valid_until.to_le_bytes());
    out.push(0); // CitizenStatus::Normal
    append_bounded_bytes(
        &mut out,
        record.province_code.as_bytes(),
        16,
        "province_code",
    )?;
    append_bounded_bytes(&mut out, record.city_code.as_bytes(), 16, "city_code")?;
    append_bounded_bytes(&mut out, record.town_code.as_bytes(), 16, "town_code")?;
    Ok(CitizenIdentityPayloadBytes {
        payload_bytes: out,
        identity_level: CitizenOnchainIdentityLevel::Voting,
    })
}

fn build_citizen_identity_payload(
    record: &CitizenRecord,
    citizen_account: &ResolvedCitizenAccount,
    identity_level: CitizenOnchainIdentityLevel,
) -> Result<CitizenIdentityPayloadBytes, axum::response::Response> {
    let mut payload = build_voting_identity_payload(record, citizen_account)?;
    if identity_level == CitizenOnchainIdentityLevel::Voting {
        return Ok(payload);
    }

    append_bounded_bytes(
        &mut payload.payload_bytes,
        record.birth_province_code.as_bytes(),
        16,
        "birth_province_code",
    )?;
    append_bounded_bytes(
        &mut payload.payload_bytes,
        record.birth_city_code.as_bytes(),
        16,
        "birth_city_code",
    )?;
    append_bounded_bytes(
        &mut payload.payload_bytes,
        record.birth_town_code.as_bytes(),
        16,
        "birth_town_code",
    )?;
    append_bounded_bytes(
        &mut payload.payload_bytes,
        record.family_name.trim().as_bytes(),
        128,
        "family_name",
    )?;
    append_bounded_bytes(
        &mut payload.payload_bytes,
        record.given_name.trim().as_bytes(),
        128,
        "given_name",
    )?;
    let sex = match record.citizen_sex.trim().to_ascii_uppercase().as_str() {
        "MALE" => 0,
        "FEMALE" => 1,
        _ => {
            return Err(api_error(
                StatusCode::BAD_REQUEST,
                1001,
                "公民性别不能编码为参选身份",
            ));
        }
    };
    payload.payload_bytes.push(sex);
    // birth_date: u32 YYYYMMDD(LE),CandidateIdentityPayload 末字段。
    // 出生日期是注册局新增公民时必填、写入后不可修改的档案字段(citizen_birth_date),
    // 链端凭此实时计算竞选公民年龄。SCALE 布局须与链端结构体逐字节一致。
    let birth_date = NaiveDate::parse_from_str(record.citizen_birth_date.as_str(), "%Y-%m-%d")
        .map_err(|_| api_error(StatusCode::BAD_REQUEST, 1001, "公民出生日期格式错误"))?;
    let birth_date_u32 =
        birth_date.year() as u32 * 10_000 + birth_date.month() * 100 + birth_date.day();
    payload.payload_bytes.extend(birth_date_u32.to_le_bytes());
    payload.identity_level = CitizenOnchainIdentityLevel::Candidate;
    Ok(payload)
}

fn append_bounded_bytes(
    out: &mut Vec<u8>,
    bytes: &[u8],
    max_len: usize,
    field: &str,
) -> Result<(), axum::response::Response> {
    if bytes.is_empty() || bytes.len() > max_len {
        let detail = format!("{field} 长度不合法");
        return Err(api_error(StatusCode::BAD_REQUEST, 1001, detail.as_str()));
    }
    out.extend(Compact(bytes.len() as u32).encode());
    out.extend_from_slice(bytes);
    Ok(())
}

fn passport_date_u32(value: &str, field: &str) -> Result<u32, axum::response::Response> {
    let date = NaiveDate::parse_from_str(value, "%Y-%m-%d").map_err(|_| {
        let detail = format!("{field} 必须是 YYYY-MM-DD");
        api_error(StatusCode::BAD_REQUEST, 1001, detail.as_str())
    })?;
    Ok((date.year() as u32) * 10_000 + date.month() * 100 + date.day())
}

fn citizen_age_years(today: NaiveDate, birth_date: NaiveDate) -> u8 {
    let mut age = today.year() - birth_date.year();
    if (today.month(), today.day()) < (birth_date.month(), birth_date.day()) {
        age -= 1;
    }
    u8::try_from(age.max(0)).unwrap_or(u8::MAX)
}

fn verify_citizen_identity_signature(
    citizen_account_id: &str,
    payload: &[u8],
    signature_hex: &str,
) -> bool {
    let Some(account_id_bytes) = parse_account_id_bytes(citizen_account_id) else {
        return false;
    };
    let Some(signature) = parse_signature_bytes(signature_hex) else {
        return false;
    };
    let message =
        primitives::sign::signing_message(primitives::sign::OP_SIGN_CITIZEN_IDENTITY, payload);
    let public = sr25519::Public::from_raw(account_id_bytes);
    let signature = sr25519::Signature::from_raw(signature);
    sr25519::Pair::verify(&signature, message, &public)
}

fn parse_signature_bytes(signature_hex: &str) -> Option<[u8; 64]> {
    let raw = hex::decode(signature_hex.trim_start_matches("0x")).ok()?;
    raw.try_into().ok()
}

pub(crate) fn active_registry_cid_number(
    state: &AppState,
) -> Result<String, axum::response::Response> {
    let binding = crate::auth::repo::active_node_binding(&state.db).map_err(|err| {
        tracing::error!(error = %err, "query active registry binding failed");
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            1004,
            "注册局绑定查询失败",
        )
    })?;
    let cid_number = binding
        .map(|binding| binding.institution_cid_number)
        .ok_or_else(|| api_error(StatusCode::BAD_REQUEST, 1001, "当前注册局缺少机构 CID 绑定"))?;
    if cid_number.is_empty()
        || cid_number.len() > primitives::core_const::CID_NUMBER_MAX_BYTES as usize
    {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            1001,
            "注册局机构 CID 格式错误",
        ));
    }
    Ok(cid_number)
}

fn encode_citizen_identity_call(
    identity_level: CitizenOnchainIdentityLevel,
    actor_cid_number: &str,
    actor_role_code: &str,
    payload_bytes: &[u8],
    expected_identity_version: u64,
    authorization_expires_at: u64,
    citizen_signature: &[u8; 64],
) -> Vec<u8> {
    let mut out = Vec::new();
    out.push(CITIZEN_IDENTITY_PALLET_INDEX);
    out.push(identity_level.call_index());
    out.extend(Compact(actor_cid_number.len() as u32).encode());
    out.extend_from_slice(actor_cid_number.as_bytes());
    out.extend(Compact(actor_role_code.len() as u32).encode());
    out.extend_from_slice(actor_role_code.as_bytes());
    out.extend_from_slice(payload_bytes);
    // 防重放两标量紧跟 payload，与链端 extrinsic 参数序一致。
    out.extend(expected_identity_version.to_le_bytes());
    out.extend(authorization_expires_at.to_le_bytes());
    out.extend(Compact(citizen_signature.len() as u32).encode());
    out.extend_from_slice(citizen_signature);
    out
}

pub(crate) fn validate_actor_role_code(value: &str) -> Result<String, axum::response::Response> {
    let value = value.trim();
    if value.is_empty() || value.len() > 64 {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            1001,
            "注册局岗位码不能为空且不得超过64字节",
        ));
    }
    Ok(value.to_string())
}
