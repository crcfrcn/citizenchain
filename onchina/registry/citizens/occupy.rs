use crate::core::chain_submit::parse_hex32;
use crate::authorization::rules::ensure_registry_admin;
//! 注册局公民 CID 占号、换绑与吊销业务。
//!
//! 本文件保留原有业务校验、调用编码、公民账户回签和管理员签名准备。
//! 通用冷签会话由平台 transactions/session.rs 持有，最终提交、确认与恢复由
//! 平台 transactions/handler.rs 执行，再调用本模块的原有身份目标校验及业务投影。

use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use chrono::{Duration, Utc};
use codec::{Compact, Encode};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::authentication::actions::require_admin_security_grant;
use crate::authentication::login::parse_account_id_bytes;
use crate::authentication::operation_auth::AdminActionType;
use crate::core::chain_citizen_identity::{
    read_citizen_identity_at, read_finalized_citizen_identity as read_finalized_cid_binding,
    FinalizedCidStatus, FinalizedCitizenIdentity as FinalizedCidBinding,
};
use crate::core::chain_submit;
use crate::transactions::handler::delete_session_best_effort;
use crate::transactions::session::{ChainSignSession, SESSION_TTL_SECS};
use crate::crypto::pubkey::{normalize_account_id, same_account_id};
use crate::registry::citizens::admin_entry::{
    cid_seed, generate_citizen_cid_candidate, validate_citizen_input, AdminCreateCitizenInput,
};
use crate::registry::citizens::chain_identity::{
    active_registry_cid_number, validate_actor_role_code,
};
use crate::*;
use sp_core::{sr25519, Pair};
use sp_crypto_hashing::blake2_256;

pub(crate) const CITIZEN_IDENTITY_PALLET_INDEX: u8 = 10;
pub(crate) const OCCUPY_CID_CALL_INDEX: u8 = 6;
const ADMIN_REBIND_CID_CALL_INDEX: u8 = 7;
const REVOKE_CID_CALL_INDEX: u8 = 8;
/// 发号碰撞重试上限(对齐 n9 桶 1000 次重试死规则)。
const CID_GENERATE_MAX_RETRY: u32 = 1000;
/// 用户账户授权按链上 Timestamp.Now 签发 300 秒，给服务端/链出块时钟偏差留余量。
const AUTHORIZATION_TTL_SECS: u64 = 300;
/// 首次绑定授权只接受 revision=0；成功占号后链上写入 revision=1。
const OCCUPY_EXPECTED_BINDING_REVISION: u64 = 0;

pub(crate) const PURPOSE_CITIZEN_OCCUPY: &str = "CITIZEN_OCCUPY";
/// 占号 pending:发号后、用户占号签名收集前的占位会话(call_data 尚未构建)。
pub(crate) const PURPOSE_CITIZEN_OCCUPY_PENDING: &str = "CITIZEN_OCCUPY_PENDING";
/// 注册局按匿名/实名辖区规则代 CID 换绑钱包账户。
pub(crate) const PURPOSE_CITIZEN_ADMIN_REBIND: &str = "CITIZEN_ADMIN_REBIND";
/// 换绑 pending:发号无关(号已有),新账户签名收集前的占位会话。
pub(crate) const PURPOSE_CITIZEN_ADMIN_REBIND_PENDING: &str = "CITIZEN_ADMIN_REBIND_PENDING";
pub(crate) const PURPOSE_CITIZEN_REVOKE: &str = "CITIZEN_REVOKE";
pub(crate) const PURPOSE_CITIZEN_IDENTITY_PUSH: &str = "CITIZEN_IDENTITY_PUSH";

impl Db {
    /// 吊销落库:本地档案状态置 REVOKED(墓碑语义,档案保留)。
    pub(crate) fn mark_citizen_revoked_conn(
        conn: &mut impl postgres::GenericClient,
        cid_number: &str,
        account_id: &str,
        onchain_tx_hash: &str,
        binding_revision: u64,
        binding_finalized_block_number: u32,
        binding_finalized_block_hash: &str,
    ) -> Result<u64, String> {
        let cid_number = cid_number.to_string();
        let account_id = account_id.to_string();
        let onchain_tx_hash = onchain_tx_hash.to_string();
        let binding_revision = i64::try_from(binding_revision)
            .map_err(|_| "citizen binding revision exceeds i64".to_string())?;
        let binding_finalized_block_number = i64::from(binding_finalized_block_number);
        let binding_finalized_block_hash = binding_finalized_block_hash.to_string();
        Db::lock_citizen_conn(conn, &cid_number)?;
        conn.execute(
                "UPDATE citizens
                 SET account_id = NULL, citizen_status = 'REVOKED',
                     binding_revision = $2, binding_finalized_block_number = $3,
                     binding_finalized_block_hash = $4,
                     status_updated_at = extract(epoch from now())::bigint,
                     onchain_tx_hash = $5, onchain_at = now(), updater_account_id = $6, updated_at = now()
                 WHERE cid_number = $1
                   AND (
                       binding_revision < $2
                       OR (binding_revision = $2 AND citizen_status = 'REVOKED')
                   )",
                &[
                    &cid_number,
                    &binding_revision,
                    &binding_finalized_block_number,
                    &binding_finalized_block_hash,
                    &onchain_tx_hash,
                    &account_id,
                ],
            )
            .map_err(|e| format!("mark citizen revoked failed: {e}"))
    }

    /// 链上身份推送成功回写(D8:提交路径同步回写,精确到交易哈希与块高)。
    ///
    /// 出生日期 `citizen_birth_date` 是新增公民时必填、写入后不可修改的字段,
    /// 任何编辑/回写路径都不得进入其 SET 子句(与链端 `BirthDateImmutable` 对齐)。
    pub(crate) fn confirm_citizen_identity_onchain_conn(
        conn: &mut impl postgres::GenericClient,
        cid_number: &str,
        registrar_account_id: &str,
        onchain_tx_hash: &str,
        finalized: &FinalizedCidBinding,
    ) -> Result<u64, String> {
        let (citizen_account_id, binding_revision) = finalized
            .active_binding()
            .ok_or_else(|| "finalized CID binding is not active".to_string())?;
        let cid_number = cid_number.to_string();
        let citizen_account_id = format!("0x{}", hex::encode(citizen_account_id));
        let registrar_account_id = registrar_account_id.to_string();
        let onchain_tx_hash = onchain_tx_hash.to_string();
        let block = Some(i64::from(finalized.finalized_block_number));
        let binding_revision = i64::try_from(binding_revision)
            .map_err(|_| "citizen binding revision exceeds i64".to_string())?;
        let binding_finalized_block_hash =
            format!("0x{}", hex::encode(finalized.finalized_block_hash));
        Db::lock_citizen_conn(conn, &cid_number)?;
        conn.execute(
                "UPDATE citizens
                 SET account_id = $2, binding_revision = $3,
                     binding_finalized_block_number = $4,
                     binding_finalized_block_hash = $5, onchain_tx_hash = $6,
                     onchain_block_number = $4, onchain_at = now(),
                     updater_account_id = $7, updated_at = now()
                 WHERE cid_number = $1
                   AND (
                       binding_revision < $3
                       OR (
                           binding_revision = $3
                           AND (account_id IS NULL OR account_id = $2)
                       )
                   )",
                &[
                    &cid_number,
                    &citizen_account_id,
                    &binding_revision,
                    &block,
                    &binding_finalized_block_hash,
                    &onchain_tx_hash,
                    &registrar_account_id,
                ],
            )
            .map_err(|e| format!("confirm citizen identity onchain failed: {e}"))
    }
}

// ──── SCALE 调用编码(citizen-identity pallet)────

fn append_bounded(out: &mut Vec<u8>, bytes: &[u8]) {
    out.extend(Compact(bytes.len() as u32).encode());
    out.extend_from_slice(bytes);
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FinalizedBindingExpectation {
    genesis_hash: [u8; 32],
    /// 首次绑定为 None；换绑为交易提交前的绑定账户/revision。
    current_binding: Option<([u8; 32], u64)>,
    target_account_id: [u8; 32],
    target_binding_revision: u64,
    /// 首次占号必须精确归因到本次注册局和账户承诺；换绑不改 CidRecord，故为 None。
    occupy_record: Option<(Vec<u8>, [u8; 32])>,
    authorization_expires_at: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FinalizedTargetState {
    Pending,
    Confirmed,
    Conflict,
}

fn authorization_expires_at(chain_now_seconds: u64) -> Option<u64> {
    chain_now_seconds.checked_add(AUTHORIZATION_TTL_SECS)
}

fn authorization_is_live(chain_now_seconds: u64, expires_at: u64) -> bool {
    expires_at > chain_now_seconds
        && expires_at <= chain_now_seconds.saturating_add(SESSION_TTL_SECS as u64)
}

pub(crate) fn finalized_target_state(
    snapshot: &FinalizedCidBinding,
    expectation: &FinalizedBindingExpectation,
) -> FinalizedTargetState {
    if snapshot.genesis_hash != expectation.genesis_hash {
        return FinalizedTargetState::Conflict;
    }
    let occupy_record_matches = match expectation.occupy_record.as_ref() {
        None => true,
        Some((registrar_cid_number, commitment)) => {
            snapshot.registrar_cid_number.as_ref() == Some(registrar_cid_number)
                && snapshot.commitment == Some(*commitment)
        }
    };
    if snapshot.cid_status == FinalizedCidStatus::Active
        && snapshot.active_binding()
            == Some((
                expectation.target_account_id,
                expectation.target_binding_revision,
            ))
        && occupy_record_matches
    {
        return FinalizedTargetState::Confirmed;
    }
    let still_current = match expectation.current_binding {
        None => snapshot.is_unoccupied(),
        Some(current) => {
            snapshot.cid_status == FinalizedCidStatus::Active
                && snapshot.active_binding() == Some(current)
        }
    };
    if still_current
        && authorization_is_live(
            snapshot.chain_now_seconds,
            expectation.authorization_expires_at,
        )
    {
        FinalizedTargetState::Pending
    } else {
        FinalizedTargetState::Conflict
    }
}

pub(crate) async fn verify_finalized_binding_at(
    cid_number: &str,
    expectation: &FinalizedBindingExpectation,
    block_hash: [u8; 32],
) -> Result<FinalizedCidBinding, String> {
    let snapshot = read_citizen_identity_at(cid_number, Some(block_hash)).await?;
    if finalized_target_state(&snapshot, expectation) == FinalizedTargetState::Confirmed {
        Ok(snapshot)
    } else {
        Err(format!(
            "CID {cid_number} 在本次 extrinsic 的 finalized block 未达到精确目标状态"
        ))
    }
}

fn encode_cid_occupy_authorization(
    genesis_hash: &[u8; 32],
    cid_number: &str,
    account_id: &[u8; 32],
    expires_at: u64,
) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(genesis_hash);
    append_bounded(&mut out, cid_number.as_bytes());
    out.extend_from_slice(account_id);
    out.extend_from_slice(&OCCUPY_EXPECTED_BINDING_REVISION.to_le_bytes());
    out.extend_from_slice(&expires_at.to_le_bytes());
    out
}

fn encode_cid_rebind_authorization(
    genesis_hash: &[u8; 32],
    cid_number: &str,
    current_account_id: &[u8; 32],
    new_account_id: &[u8; 32],
    expected_binding_revision: u64,
    expires_at: u64,
) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(genesis_hash);
    append_bounded(&mut out, cid_number.as_bytes());
    out.extend_from_slice(current_account_id);
    out.extend_from_slice(new_account_id);
    out.extend_from_slice(&expected_binding_revision.to_le_bytes());
    out.extend_from_slice(&expires_at.to_le_bytes());
    out
}

/// occupy_cid(actor_cid_number, actor_role_code, cid_number, account_id, expires_at, citizen_signature)
///
/// 占即绑:居住地/承诺哈希不再是 call 参数(commitment 链上算 blake2_256(account_id),
/// 居住地去地域)。`account_id` = AccountId32,32 裸字节(无长度前缀);
/// `occupy_signature` = 用户对 `CidOccupyAuthorization` 的签名,BoundedVec。
fn encode_occupy_cid_call(
    actor_cid_number: &str,
    actor_role_code: &str,
    cid_number: &str,
    account_id: &[u8; 32],
    expires_at: u64,
    occupy_signature: &[u8],
) -> Vec<u8> {
    let mut out = Vec::new();
    out.push(CITIZEN_IDENTITY_PALLET_INDEX);
    out.push(OCCUPY_CID_CALL_INDEX);
    append_bounded(&mut out, actor_cid_number.as_bytes());
    append_bounded(&mut out, actor_role_code.as_bytes());
    append_bounded(&mut out, cid_number.as_bytes());
    out.extend_from_slice(account_id);
    out.extend_from_slice(&expires_at.to_le_bytes());
    append_bounded(&mut out, occupy_signature);
    out
}

/// 验用户首次绑定签名；authorization 必须由当前链创世哈希、revision=0 和过期时间构造。
fn verify_occupy_signature(account_id: &str, authorization: &[u8], signature_hex: &str) -> bool {
    let Some(account_id_bytes) = parse_account_id_bytes(account_id) else {
        return false;
    };
    let Some(signature) = parse_signature_bytes(signature_hex) else {
        return false;
    };
    let message =
        primitives::sign::signing_message(primitives::sign::OP_SIGN_CID_OCCUPY, authorization);
    let public = sr25519::Public::from_raw(account_id_bytes);
    let signature = sr25519::Signature::from_raw(signature);
    sr25519::Pair::verify(&signature, message, &public)
}

/// admin_rebind_cid_account_id(actor_cid_number, actor_role_code, cid_number, new_account_id,
/// expected_binding_revision, expires_at, new_account_signature)
///
/// `new_account_signature` = 新账户对 `CidRebindAuthorization` 的签名,BoundedVec。
fn encode_admin_rebind_cid_account_id_call(
    actor_cid_number: &str,
    actor_role_code: &str,
    cid_number: &str,
    new_account_id: &[u8; 32],
    expected_binding_revision: u64,
    expires_at: u64,
    new_account_signature: &[u8],
) -> Vec<u8> {
    let mut out = Vec::new();
    out.push(CITIZEN_IDENTITY_PALLET_INDEX);
    out.push(ADMIN_REBIND_CID_CALL_INDEX);
    append_bounded(&mut out, actor_cid_number.as_bytes());
    append_bounded(&mut out, actor_role_code.as_bytes());
    append_bounded(&mut out, cid_number.as_bytes());
    out.extend_from_slice(new_account_id);
    out.extend_from_slice(&expected_binding_revision.to_le_bytes());
    out.extend_from_slice(&expires_at.to_le_bytes());
    append_bounded(&mut out, new_account_signature);
    out
}

/// 验新账户换绑签名；authorization 锁定链、旧/新账户、revision 与过期时间。
fn verify_admin_rebind_signature(
    account_id: &str,
    authorization: &[u8],
    signature_hex: &str,
) -> bool {
    let Some(account_id_bytes) = parse_account_id_bytes(account_id) else {
        return false;
    };
    let Some(signature) = parse_signature_bytes(signature_hex) else {
        return false;
    };
    let message = primitives::sign::signing_message(
        primitives::sign::OP_SIGN_CID_ADMIN_REBIND,
        authorization,
    );
    let public = sr25519::Public::from_raw(account_id_bytes);
    let signature = sr25519::Signature::from_raw(signature);
    sr25519::Pair::verify(&signature, message, &public)
}

/// 验当前账户对同一换绑授权载荷的签名；仅证明客户端可继承此前私有密文，不是注册局
/// admin_rebind 的授权前提。当前账户不可用时注册局权限链仍可独立完成控制权换绑。
fn verify_current_account_signature(
    account_id: &str,
    authorization: &[u8],
    signature_hex: &str,
) -> bool {
    let Some(account_id_bytes) = parse_account_id_bytes(account_id) else {
        return false;
    };
    let Some(signature) = parse_signature_bytes(signature_hex) else {
        return false;
    };
    let message =
        primitives::sign::signing_message(primitives::sign::OP_SIGN_CID_REBIND, authorization);
    let public = sr25519::Public::from_raw(account_id_bytes);
    let signature = sr25519::Signature::from_raw(signature);
    sr25519::Pair::verify(&signature, message, &public)
}

fn parse_signature_bytes(signature_hex: &str) -> Option<[u8; 64]> {
    let raw = hex::decode(signature_hex.trim_start_matches("0x")).ok()?;
    raw.try_into().ok()
}



pub(crate) fn finalized_binding_expectation(
    session: &ChainSignSession,
) -> Result<Option<(String, FinalizedBindingExpectation)>, String> {
    if !matches!(
        session.purpose.as_str(),
        PURPOSE_CITIZEN_OCCUPY | PURPOSE_CITIZEN_ADMIN_REBIND
    ) {
        return Ok(None);
    }
    let cid_number = session
        .context
        .get("cid_number")
        .and_then(|value| value.as_str())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "finalized binding context missing cid_number".to_string())?
        .to_string();
    let genesis_hash = session
        .context
        .get("genesis_hash")
        .and_then(|value| value.as_str())
        .and_then(parse_hex32)
        .ok_or_else(|| "finalized binding context missing genesis_hash".to_string())?;
    let authorization_expires_at = session
        .context
        .get("authorization_expires_at")
        .and_then(|value| value.as_u64())
        .ok_or_else(|| "finalized binding context missing authorization_expires_at".to_string())?;

    let expectation = if session.purpose == PURPOSE_CITIZEN_OCCUPY {
        let target_account_id = session
            .context
            .get("citizen_account_id")
            .and_then(|value| value.as_str())
            .and_then(parse_hex32)
            .ok_or_else(|| "finalized occupy context missing citizen_account_id".to_string())?;
        let actor_cid_number = session
            .context
            .get("actor_cid_number")
            .and_then(|value| value.as_str())
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "finalized occupy context missing actor_cid_number".to_string())?;
        FinalizedBindingExpectation {
            genesis_hash,
            current_binding: None,
            target_account_id,
            target_binding_revision: 1,
            occupy_record: Some((
                actor_cid_number.as_bytes().to_vec(),
                blake2_256(&target_account_id),
            )),
            authorization_expires_at,
        }
    } else {
        let current_account_id = session
            .context
            .get("current_account_id")
            .and_then(|value| value.as_str())
            .and_then(parse_hex32)
            .ok_or_else(|| "finalized rebind context missing current_account_id".to_string())?;
        let current_binding_revision = session
            .context
            .get("expected_binding_revision")
            .and_then(|value| value.as_u64())
            .filter(|value| *value > 0)
            .ok_or_else(|| {
                "finalized rebind context missing expected_binding_revision".to_string()
            })?;
        let target_account_id = session
            .context
            .get("new_account_id")
            .and_then(|value| value.as_str())
            .and_then(parse_hex32)
            .ok_or_else(|| "finalized rebind context missing new_account_id".to_string())?;
        let target_binding_revision = current_binding_revision
            .checked_add(1)
            .ok_or_else(|| "finalized rebind revision overflow".to_string())?;
        FinalizedBindingExpectation {
            genesis_hash,
            current_binding: Some((current_account_id, current_binding_revision)),
            target_account_id,
            target_binding_revision,
            occupy_record: None,
            authorization_expires_at,
        }
    };
    Ok(Some((cid_number, expectation)))
}

/// revoke_cid(actor_cid_number, actor_role_code, cid_number)
fn encode_revoke_cid_call(
    actor_cid_number: &str,
    actor_role_code: &str,
    cid_number: &str,
) -> Vec<u8> {
    let mut out = Vec::new();
    out.push(CITIZEN_IDENTITY_PALLET_INDEX);
    out.push(REVOKE_CID_CALL_INDEX);
    append_bounded(&mut out, actor_cid_number.as_bytes());
    append_bounded(&mut out, actor_role_code.as_bytes());
    append_bounded(&mut out, cid_number.as_bytes());
    out
}

// ──── DTO ────

#[derive(Serialize)]
pub(crate) struct PrepareCitizenOccupyOutput {
    pub(crate) request_id: String,
    pub(crate) cid_number: String,
    /// 公民钱包域签名请求二维码；b.d 锁定创世哈希、CID、revision=0 和过期时间，
    /// account_id 槽留零，钱包选定本账户后填入并签名。
    pub(crate) citizen_sign_request: String,
    pub(crate) expires_at: i64,
}

/// 提交用户占号签名(第二段):管理员回扫用户已签名的响应二维码后回传。
#[derive(Deserialize)]
pub(crate) struct SubmitCitizenOccupyInput {
    pub(crate) request_id: String,
    /// 用户钱包账户(0x 小写 hex),占即绑主键;由用户签名响应二维码带回。
    pub(crate) account_id: String,
    /// 用户对 CidOccupyAuthorization 的占号授权签名(域 OP_SIGN_CID_OCCUPY)。
    pub(crate) occupy_signature: String,
}

/// 提交用户占号签名返回:管理员冷签请求二维码(第三段冷签用)。
#[derive(Serialize)]
pub(crate) struct SubmitCitizenOccupyOutput {
    pub(crate) request_id: String,
    pub(crate) cid_number: String,
    pub(crate) sign_request: String,
    pub(crate) expires_at: i64,
}

/// 换绑第一段:注册局按链上身份类型和辖区规则为 CID 发起换绑钱包账户。
#[derive(Deserialize)]
pub(crate) struct PrepareCitizenRebindInput {
    pub(crate) actor_role_code: String,
    pub(crate) cid_number: String,
}

#[derive(Serialize)]
pub(crate) struct PrepareCitizenRebindOutput {
    pub(crate) request_id: String,
    pub(crate) cid_number: String,
    /// 新钱包域签名请求二维码；b.d 锁定链、CID、当前账户、revision 和过期时间，
    /// new_account_id 槽留零，钱包选定本账户后填入并签名。
    pub(crate) new_wallet_sign_request: String,
    pub(crate) expires_at: i64,
}

/// 换绑第二段:管理员回扫新钱包已签名的响应二维码后回传。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SubmitCitizenRebindInput {
    pub(crate) request_id: String,
    /// 新钱包账户(0x 小写 hex),换绑后 CID 绑定的新 account_id。
    pub(crate) new_account_id: String,
    /// 新账户对 CidRebindAuthorization 的换绑授权签名(域 OP_SIGN_CID_ADMIN_REBIND)。
    pub(crate) new_account_signature: String,
    /// 可选当前账户签名对；同时提供才表示客户端已完成此前私有数据交接预演。
    pub(crate) current_account_id: Option<String>,
    pub(crate) current_account_signature: Option<String>,
}

#[derive(Serialize)]
pub(crate) struct SubmitCitizenRebindOutput {
    pub(crate) request_id: String,
    pub(crate) cid_number: String,
    pub(crate) sign_request: String,
    pub(crate) expires_at: i64,
}

#[derive(Serialize)]
pub(crate) struct PrepareCitizenRevokeOutput {
    pub(crate) request_id: String,
    pub(crate) cid_number: String,
    pub(crate) sign_request: String,
    pub(crate) expires_at: i64,
}

#[derive(Deserialize)]
pub(crate) struct PrepareCitizenRevokeInput {
    pub(crate) actor_role_code: String,
}

// ──── handlers ────

/// 建档占号 prepare(第一段):校验 + onchina 服务端 `cid_seed` 发号;不建 call、不落档案。
/// 返回 cid_number,供前端向用户展示带链、revision=0 和过期时间约束的占号授权二维码。
pub(crate) async fn prepare_citizen_occupy(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<AdminCreateCitizenInput>,
) -> impl IntoResponse {
    let ctx = match require_admin_any(&state, &headers) {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    if let Err(resp) = ensure_registry_admin(&ctx) {
        return resp;
    }
    let actor_role_code = match validate_actor_role_code(input.actor_role_code.as_str()) {
        Ok(value) => value,
        Err(resp) => return resp,
    };
    let validated = match validate_citizen_input(&ctx, &input) {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    // 链上写(occupy_cid extrinsic)硬规则 = passkey + 冷签:此处强制 passkey 断言(冷签在段3)。
    let occupy_grant_payload = serde_json::json!({
        "cid_type": validated.cid_type,
        "actor_role_code": actor_role_code,
        "op": "occupy",
    });
    let passkey = match require_admin_security_grant(
        &state,
        &headers,
        &ctx,
        AdminActionType::CitizenOnchainPush,
        validated.cid_type.as_str(),
        Some(&occupy_grant_payload),
    ) {
        Ok(proof) => proof,
        Err(resp) => return resp,
    };
    let seed = cid_seed(&validated);

    // 发号:本地投影查重后，从同一 finalized 快照确认登记、绑定和 revision 都不存在。
    // 链上首次绑定不提供旧证明兼容或同账户续用；任何已登记状态都必须换候选号。
    let mut chosen: Option<(String, [u8; 32], u64)> = None;
    for nonce in 0..CID_GENERATE_MAX_RETRY {
        let candidate = match generate_citizen_cid_candidate(&validated, &seed, nonce) {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        match state.db.find_citizen_by_cid(candidate.as_str()) {
            Ok(Some(_)) => continue,
            Ok(None) => {}
            Err(err) => {
                tracing::error!(error = %err, "cid local pre-check failed");
                return api_error(StatusCode::INTERNAL_SERVER_ERROR, 1004, "发号本地查重失败");
            }
        }
        match read_finalized_cid_binding(candidate.as_str()).await {
            Ok(snapshot) if snapshot.is_unoccupied() => {
                let Some(expires_at) = authorization_expires_at(snapshot.chain_now_seconds) else {
                    return api_error(
                        StatusCode::BAD_GATEWAY,
                        1004,
                        "链上时间异常,无法签发占号授权",
                    );
                };
                chosen = Some((candidate, snapshot.genesis_hash, expires_at));
                break;
            }
            Ok(_) => continue,
            Err(err) => {
                tracing::error!(error = %err, "cid finalized binding pre-check failed");
                return api_error(
                    StatusCode::BAD_GATEWAY,
                    1004,
                    "发号链上绑定状态查重失败(链不可用)",
                );
            }
        }
    }
    let Some((cid_number, genesis_hash, authorization_expires_at)) = chosen else {
        return api_error(StatusCode::INTERNAL_SERVER_ERROR, 1004, "发号重试耗尽");
    };

    let issued_at = Utc::now();
    let expires_at = issued_at + Duration::seconds(SESSION_TTL_SECS);
    let request_id = format!("citizen-occupy-{}", Uuid::new_v4());
    // pending 会话:call_data/nonce/signing_hash 占位,待用户签名回来后 promote 回填。
    let session = ChainSignSession {
        request_id: request_id.clone(),
        purpose: PURPOSE_CITIZEN_OCCUPY_PENDING.to_string(),
        account_id: ctx.account_id.clone(),
        call_data: Vec::new(),
        nonce: 0,
        signing_hash: String::new(),
        context: serde_json::json!({
            "validated": validated,
            "cid_number": cid_number,
            "actor_role_code": actor_role_code,
            "genesis_hash": format!("0x{}", hex::encode(genesis_hash)),
            "expected_binding_revision": OCCUPY_EXPECTED_BINDING_REVISION,
            "authorization_expires_at": authorization_expires_at,
        }),
        expires_at,
        consumed_at: None,
    };
    if let Err(err) = state.db.insert_chain_sign_session(&session, &passkey, &ctx.institution_cid_number) {
        tracing::error!(error = %err, "insert occupy pending session failed");
        return api_error(StatusCode::INTERNAL_SERVER_ERROR, 1004, "占号会话落库失败");
    }

    // account_id 槽以 32 字节零占位；钱包必须解析完整模板并在原槽位填入所选账户。
    let authorization_template = encode_cid_occupy_authorization(
        &genesis_hash,
        cid_number.as_str(),
        &[0u8; 32],
        authorization_expires_at,
    );
    let citizen_sign_request = match crate::core::qr::build_domain_sign_request_bytes(
        request_id.as_str(),
        authorization_expires_at as i64,
        &authorization_template,
        crate::core::qr::action_occupy(),
    ) {
        Ok(v) => v,
        Err(resp) => return resp,
    };

    crate::core::runtime_ops::append_audit_log(
        &state,
        "CITIZEN_OCCUPY_PREPARE",
        &ctx.account_id,
        Some(cid_number.clone()),
        serde_json::json!({
            "cid_number": cid_number,
            "request_id": request_id,
            "genesis_hash": format!("0x{}", hex::encode(genesis_hash)),
            "expected_binding_revision": OCCUPY_EXPECTED_BINDING_REVISION,
            "authorization_expires_at": authorization_expires_at,
            "actor_ip": actor_ip_from_headers(&headers),
        }),
    );

    Json(ApiResponse {
        code: 0,
        message: "ok".to_string(),
        data: PrepareCitizenOccupyOutput {
            request_id,
            cid_number,
            citizen_sign_request,
            expires_at: authorization_expires_at as i64,
        },
    })
    .into_response()
}

/// 提交用户占号签名(第二段):复核 finalized 未登记状态 → 验用户签名 →
/// 构造 occupy_cid call(占即绑 account_id)→
/// prepare_signing → 会话转正 → 返回管理员冷签请求二维码。
pub(crate) async fn submit_citizen_occupy(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<SubmitCitizenOccupyInput>,
) -> impl IntoResponse {
    let ctx = match require_admin_any(&state, &headers) {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    if let Err(resp) = ensure_registry_admin(&ctx) {
        return resp;
    }
    let session = match state.db.find_chain_sign_session(input.request_id.as_str()) {
        Ok(Some(v)) => v,
        Ok(None) => return api_error(StatusCode::NOT_FOUND, 1004, "占号会话不存在"),
        Err(err) => {
            tracing::error!(error = %err, "query occupy pending session failed");
            return api_error(StatusCode::INTERNAL_SERVER_ERROR, 1004, "占号会话查询失败");
        }
    };
    if session.purpose != PURPOSE_CITIZEN_OCCUPY_PENDING {
        return api_error(StatusCode::CONFLICT, 1005, "占号会话状态不正确");
    }
    if session.consumed_at.is_some() {
        delete_session_best_effort(&state, session.request_id.as_str(), "consumed pending");
        return api_error(StatusCode::CONFLICT, 1005, "占号会话已被消费");
    }
    if session.expires_at < Utc::now() {
        delete_session_best_effort(&state, session.request_id.as_str(), "pending expired");
        return api_error(StatusCode::GONE, 1005, "占号会话已过期,请重新发起");
    }
    if !same_account_id(session.account_id.as_str(), ctx.account_id.as_str()) {
        return api_error(StatusCode::FORBIDDEN, 1003, "只有发起管理员可以提交本会话");
    }

    let cid_number = session
        .context
        .get("cid_number")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let actor_role_code = session
        .context
        .get("actor_role_code")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let expected_genesis_hash = session
        .context
        .get("genesis_hash")
        .and_then(|v| v.as_str())
        .and_then(parse_hex32);
    let expected_binding_revision = session
        .context
        .get("expected_binding_revision")
        .and_then(|v| v.as_u64());
    let authorization_expires_at = session
        .context
        .get("authorization_expires_at")
        .and_then(|v| v.as_u64());
    let (Some(expected_genesis_hash), Some(authorization_expires_at)) =
        (expected_genesis_hash, authorization_expires_at)
    else {
        delete_session_best_effort(
            &state,
            session.request_id.as_str(),
            "pending context invalid",
        );
        return api_error(StatusCode::INTERNAL_SERVER_ERROR, 1004, "占号会话数据损坏");
    };
    if cid_number.is_empty()
        || actor_role_code.is_empty()
        || expected_binding_revision != Some(OCCUPY_EXPECTED_BINDING_REVISION)
    {
        delete_session_best_effort(
            &state,
            session.request_id.as_str(),
            "pending context invalid",
        );
        return api_error(StatusCode::INTERNAL_SERVER_ERROR, 1004, "占号会话数据损坏");
    }

    // 用户钱包账户(0x 小写 hex,占即绑主键)。
    let Some(account_id_hex) = normalize_account_id(input.account_id.as_str()) else {
        return api_error(StatusCode::BAD_REQUEST, 1001, "用户钱包账户格式错误");
    };
    let finalized = match read_finalized_cid_binding(cid_number.as_str()).await {
        Ok(snapshot) => snapshot,
        Err(err) => {
            tracing::error!(error = %err, "recheck finalized occupy binding failed");
            return api_error(
                StatusCode::BAD_GATEWAY,
                1004,
                "占号链上绑定状态复核失败(链不可用)",
            );
        }
    };
    if finalized.genesis_hash != expected_genesis_hash || !finalized.is_unoccupied() {
        delete_session_best_effort(&state, session.request_id.as_str(), "occupy state changed");
        return api_error(
            StatusCode::CONFLICT,
            1005,
            "CID 链上状态已变化,请重新发起占号",
        );
    }
    if !authorization_is_live(finalized.chain_now_seconds, authorization_expires_at) {
        delete_session_best_effort(
            &state,
            session.request_id.as_str(),
            "occupy authorization expired",
        );
        return api_error(StatusCode::GONE, 1005, "占号授权已过期,请重新发起");
    }
    let Some(account_id_bytes) = parse_account_id_bytes(account_id_hex.as_str()) else {
        return api_error(StatusCode::BAD_REQUEST, 1001, "用户钱包账户格式错误");
    };
    let authorization = encode_cid_occupy_authorization(
        &expected_genesis_hash,
        cid_number.as_str(),
        &account_id_bytes,
        authorization_expires_at,
    );
    // 链、CID、账户、revision=0、过期时间任一变化都会导致验签失败。
    if !verify_occupy_signature(
        account_id_hex.as_str(),
        &authorization,
        input.occupy_signature.as_str(),
    ) {
        return api_error(StatusCode::BAD_REQUEST, 1003, "用户占号签名验证失败");
    }
    let Some(occupy_signature_bytes) = parse_signature_bytes(input.occupy_signature.as_str())
    else {
        return api_error(StatusCode::BAD_REQUEST, 1001, "用户占号签名格式错误");
    };

    let actor_cid_number = match active_registry_cid_number(&state) {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let call = encode_occupy_cid_call(
        &actor_cid_number,
        actor_role_code.as_str(),
        cid_number.as_str(),
        &account_id_bytes,
        authorization_expires_at,
        &occupy_signature_bytes,
    );
    let prepared = match chain_submit::prepare_signing(&call, ctx.account_id.as_str()).await {
        Ok(v) => v,
        Err(err) => {
            tracing::error!(error = %err, "prepare occupy signing failed");
            return api_error(
                StatusCode::BAD_GATEWAY,
                1004,
                "链签名载荷准备失败(链不可用)",
            );
        }
    };

    let issued_at = Utc::now();
    let action = crate::core::institution_call::chain_action_code(
        CITIZEN_IDENTITY_PALLET_INDEX,
        OCCUPY_CID_CALL_INDEX,
    );
    let sign_request = match crate::core::qr::build_sign_request_bytes(
        session.request_id.as_str(),
        issued_at.timestamp(),
        authorization_expires_at as i64,
        ctx.account_id.as_str(),
        &prepared.payload,
        action,
    ) {
        Ok(v) => v,
        Err(resp) => return resp,
    };

    let mut context = session.context.clone();
    if let Some(map) = context.as_object_mut() {
        map.insert(
            "citizen_account_id".to_string(),
            serde_json::Value::String(account_id_hex.clone()),
        );
        map.insert(
            "actor_cid_number".to_string(),
            serde_json::Value::String(actor_cid_number),
        );
    }
    if let Err(err) = state.db.promote_chain_sign_session(
        session.request_id.as_str(),
        PURPOSE_CITIZEN_OCCUPY,
        &call,
        prepared.nonce,
        prepared.signing_hash_hex.as_str(),
        &context,
    ) {
        tracing::error!(error = %err, "promote occupy session failed");
        return api_error(StatusCode::INTERNAL_SERVER_ERROR, 1004, "占号会话转正失败");
    }

    Json(ApiResponse {
        code: 0,
        message: "ok".to_string(),
        data: SubmitCitizenOccupyOutput {
            request_id: session.request_id,
            cid_number,
            sign_request,
            expires_at: authorization_expires_at as i64,
        },
    })
    .into_response()
}

/// 换绑 prepare(第一段):直接读取链上 finalized 绑定真源，不以本局投影限制办理入口。
///
/// 匿名 CID 可由任一在册 CREG/FRG 办理；civic CID 的本市 CREG/对应省 FRG 边界由
/// Runtime 统一鉴权。返回同一份完整授权模板：新钱包必须签名；当前钱包签名可选，
/// 仅用于证明客户端可以在换绑生效时完成历史私有密文交接，不改变注册局强制换绑权限。
pub(crate) async fn prepare_citizen_rebind(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<PrepareCitizenRebindInput>,
) -> impl IntoResponse {
    let ctx = match require_admin_any(&state, &headers) {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    if let Err(resp) = ensure_registry_admin(&ctx) {
        return resp;
    }
    let actor_role_code = match validate_actor_role_code(input.actor_role_code.as_str()) {
        Ok(value) => value,
        Err(resp) => return resp,
    };
    let cid_number = input.cid_number.trim().to_string();
    if cid_number.is_empty() {
        return api_error(StatusCode::BAD_REQUEST, 1001, "cid_number 不能为空");
    }
    // 链上写(admin_rebind_cid_account_id extrinsic)硬规则 = passkey + 冷签:强制 passkey 断言(冷签在段3)。
    // 注册局强制换绑不以当前账户签名为权限前提；passkey 是注册局侧操作者身份加固，不可省。
    let rebind_grant_payload = serde_json::json!({
        "cid_number": cid_number,
        "actor_role_code": actor_role_code,
        "op": "admin_rebind",
    });
    let passkey = match require_admin_security_grant(
        &state,
        &headers,
        &ctx,
        AdminActionType::CitizenOnchainPush,
        cid_number.as_str(),
        Some(&rebind_grant_payload),
    ) {
        Ok(proof) => proof,
        Err(resp) => return resp,
    };

    let issued_at = Utc::now();
    let expires_at = issued_at + Duration::seconds(SESSION_TTL_SECS);
    let finalized = match read_finalized_cid_binding(cid_number.as_str()).await {
        Ok(snapshot) => snapshot,
        Err(err) => {
            tracing::error!(error = %err, "read finalized rebind state failed");
            return api_error(
                StatusCode::BAD_GATEWAY,
                1004,
                "换绑链上绑定状态读取失败(链不可用)",
            );
        }
    };
    let (current_account_id, expected_binding_revision) = match finalized.cid_status {
        FinalizedCidStatus::Missing => {
            return api_error(
                StatusCode::NOT_FOUND,
                1004,
                "链上 CID 不存在或未绑定钱包账户",
            );
        }
        FinalizedCidStatus::Revoked => {
            return api_error(StatusCode::CONFLICT, 1005, "链上 CID 已吊销,禁止换绑");
        }
        FinalizedCidStatus::Active => match finalized.active_binding() {
            Some(binding) => binding,
            None => {
                tracing::error!(
                    cid_number = %cid_number,
                    has_account_id = finalized.account_id.is_some(),
                    binding_revision = ?finalized.binding_revision,
                    "inconsistent active finalized CID binding state"
                );
                return api_error(
                    StatusCode::BAD_GATEWAY,
                    1004,
                    "链上 CID 绑定状态不一致,已拒绝换绑",
                );
            }
        },
    };
    let Some(authorization_expires_at) = authorization_expires_at(finalized.chain_now_seconds)
    else {
        return api_error(
            StatusCode::BAD_GATEWAY,
            1004,
            "链上时间异常,无法签发换绑授权",
        );
    };
    let request_id = format!("citizen-rebind-{}", Uuid::new_v4());
    let session = ChainSignSession {
        request_id: request_id.clone(),
        purpose: PURPOSE_CITIZEN_ADMIN_REBIND_PENDING.to_string(),
        account_id: ctx.account_id.clone(),
        call_data: Vec::new(),
        nonce: 0,
        signing_hash: String::new(),
        context: serde_json::json!({
            "cid_number": cid_number,
            "actor_role_code": actor_role_code,
            "genesis_hash": format!("0x{}", hex::encode(finalized.genesis_hash)),
            "current_account_id": format!("0x{}", hex::encode(current_account_id)),
            "expected_binding_revision": expected_binding_revision,
            "authorization_expires_at": authorization_expires_at,
        }),
        expires_at,
        consumed_at: None,
    };
    if let Err(err) = state.db.insert_chain_sign_session(&session, &passkey, &ctx.institution_cid_number) {
        tracing::error!(error = %err, "insert rebind pending session failed");
        return api_error(StatusCode::INTERNAL_SERVER_ERROR, 1004, "换绑会话落库失败");
    }
    // new_account_id 槽以 32 字节零占位；钱包必须在精确槽位填入所选新账户后签名。
    let authorization_template = encode_cid_rebind_authorization(
        &finalized.genesis_hash,
        cid_number.as_str(),
        &current_account_id,
        &[0u8; 32],
        expected_binding_revision,
        authorization_expires_at,
    );
    let new_wallet_sign_request = match crate::core::qr::build_domain_sign_request_bytes(
        request_id.as_str(),
        authorization_expires_at as i64,
        &authorization_template,
        crate::core::qr::action_rebind(),
    ) {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    crate::core::runtime_ops::append_audit_log(
        &state,
        "CITIZEN_ADMIN_REBIND_PREPARE",
        &ctx.account_id,
        Some(cid_number.clone()),
        serde_json::json!({
            "cid_number": cid_number,
            "request_id": request_id,
            "genesis_hash": format!("0x{}", hex::encode(finalized.genesis_hash)),
            "current_account_id": format!("0x{}", hex::encode(current_account_id)),
            "expected_binding_revision": expected_binding_revision,
            "authorization_expires_at": authorization_expires_at,
            "actor_ip": actor_ip_from_headers(&headers),
        }),
    );
    Json(ApiResponse {
        code: 0,
        message: "ok".to_string(),
        data: PrepareCitizenRebindOutput {
            request_id,
            cid_number,
            new_wallet_sign_request,
            expires_at: authorization_expires_at as i64,
        },
    })
    .into_response()
}

/// 换绑第二段:复核 finalized 当前账户/revision 未变化 → 验新账户换绑签名 →
/// 构造 admin_rebind_cid_account_id call → prepare_signing → 会话转正。
pub(crate) async fn submit_citizen_rebind(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<SubmitCitizenRebindInput>,
) -> impl IntoResponse {
    let ctx = match require_admin_any(&state, &headers) {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    if let Err(resp) = ensure_registry_admin(&ctx) {
        return resp;
    }
    let session = match state.db.find_chain_sign_session(input.request_id.as_str()) {
        Ok(Some(v)) => v,
        Ok(None) => return api_error(StatusCode::NOT_FOUND, 1004, "换绑会话不存在"),
        Err(err) => {
            tracing::error!(error = %err, "query rebind pending session failed");
            return api_error(StatusCode::INTERNAL_SERVER_ERROR, 1004, "换绑会话查询失败");
        }
    };
    if session.purpose != PURPOSE_CITIZEN_ADMIN_REBIND_PENDING {
        return api_error(StatusCode::CONFLICT, 1005, "换绑会话状态不正确");
    }
    if session.consumed_at.is_some() {
        delete_session_best_effort(&state, session.request_id.as_str(), "consumed pending");
        return api_error(StatusCode::CONFLICT, 1005, "换绑会话已被消费");
    }
    if session.expires_at < Utc::now() {
        delete_session_best_effort(&state, session.request_id.as_str(), "pending expired");
        return api_error(StatusCode::GONE, 1005, "换绑会话已过期,请重新发起");
    }
    if !same_account_id(session.account_id.as_str(), ctx.account_id.as_str()) {
        return api_error(StatusCode::FORBIDDEN, 1003, "只有发起管理员可以提交本会话");
    }

    let cid_number = session
        .context
        .get("cid_number")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let actor_role_code = session
        .context
        .get("actor_role_code")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let expected_genesis_hash = session
        .context
        .get("genesis_hash")
        .and_then(|v| v.as_str())
        .and_then(parse_hex32);
    let current_account_id = session
        .context
        .get("current_account_id")
        .and_then(|v| v.as_str())
        .and_then(parse_hex32);
    let expected_binding_revision = session
        .context
        .get("expected_binding_revision")
        .and_then(|v| v.as_u64());
    let authorization_expires_at = session
        .context
        .get("authorization_expires_at")
        .and_then(|v| v.as_u64());
    let (
        Some(expected_genesis_hash),
        Some(current_account_id),
        Some(expected_binding_revision),
        Some(authorization_expires_at),
    ) = (
        expected_genesis_hash,
        current_account_id,
        expected_binding_revision,
        authorization_expires_at,
    )
    else {
        delete_session_best_effort(
            &state,
            session.request_id.as_str(),
            "pending context invalid",
        );
        return api_error(StatusCode::INTERNAL_SERVER_ERROR, 1004, "换绑会话数据损坏");
    };
    if cid_number.is_empty() || actor_role_code.is_empty() || expected_binding_revision == 0 {
        delete_session_best_effort(
            &state,
            session.request_id.as_str(),
            "pending context invalid",
        );
        return api_error(StatusCode::INTERNAL_SERVER_ERROR, 1004, "换绑会话数据损坏");
    }

    // 新钱包账户(0x 小写 hex,换绑后 CID 绑定的新 account_id)。
    let Some(account_id_hex) = normalize_account_id(input.new_account_id.as_str()) else {
        return api_error(StatusCode::BAD_REQUEST, 1001, "新钱包账户格式错误");
    };
    let Some(account_id_bytes) = parse_account_id_bytes(account_id_hex.as_str()) else {
        return api_error(StatusCode::BAD_REQUEST, 1001, "新钱包账户格式错误");
    };
    if account_id_bytes == current_account_id {
        return api_error(
            StatusCode::BAD_REQUEST,
            1001,
            "新钱包账户不能与当前绑定账户相同",
        );
    }
    // submit 前再次读取同一 finalized 快照；当前账户/revision 任一变化都使挑战失效。
    let finalized = match read_finalized_cid_binding(cid_number.as_str()).await {
        Ok(snapshot) => snapshot,
        Err(err) => {
            tracing::error!(error = %err, "recheck finalized rebind state failed");
            return api_error(
                StatusCode::BAD_GATEWAY,
                1004,
                "换绑链上绑定状态复核失败(链不可用)",
            );
        }
    };
    if finalized.genesis_hash != expected_genesis_hash
        || finalized.cid_status != FinalizedCidStatus::Active
        || finalized.active_binding() != Some((current_account_id, expected_binding_revision))
    {
        delete_session_best_effort(&state, session.request_id.as_str(), "rebind state changed");
        return api_error(
            StatusCode::CONFLICT,
            1005,
            "CID 当前账户或绑定 revision 已变化,请重新发起换绑",
        );
    }
    if !authorization_is_live(finalized.chain_now_seconds, authorization_expires_at) {
        delete_session_best_effort(
            &state,
            session.request_id.as_str(),
            "rebind authorization expired",
        );
        return api_error(StatusCode::GONE, 1005, "换绑授权已过期,请重新发起");
    }
    let authorization = encode_cid_rebind_authorization(
        &expected_genesis_hash,
        cid_number.as_str(),
        &current_account_id,
        &account_id_bytes,
        expected_binding_revision,
        authorization_expires_at,
    );
    let data_handover_authorized = match (
        input.current_account_id.as_deref(),
        input.current_account_signature.as_deref(),
    ) {
        (None, None) => false,
        (Some(submitted_current_account_id), Some(current_account_signature)) => {
            let current_account_id_hex = format!("0x{}", hex::encode(current_account_id));
            let Some(normalized_current_account_id) =
                normalize_account_id(submitted_current_account_id)
            else {
                return api_error(StatusCode::BAD_REQUEST, 1001, "当前钱包账户格式错误");
            };
            if normalized_current_account_id != current_account_id_hex {
                return api_error(
                    StatusCode::BAD_REQUEST,
                    1003,
                    "当前钱包签名账户不是 CID 当前绑定账户",
                );
            }
            if !verify_current_account_signature(
                normalized_current_account_id.as_str(),
                &authorization,
                current_account_signature,
            ) {
                return api_error(StatusCode::BAD_REQUEST, 1003, "当前账户换绑签名验证失败");
            }
            true
        }
        _ => {
            return api_error(
                StatusCode::BAD_REQUEST,
                1001,
                "current_account_id 与 current_account_signature 必须同时提供或同时省略",
            );
        }
    };
    // 新钱包签名锁定链、当前/新账户、revision 与过期时间；当前账户签名只决定数据是否可交接。
    if !verify_admin_rebind_signature(
        account_id_hex.as_str(),
        &authorization,
        input.new_account_signature.as_str(),
    ) {
        return api_error(StatusCode::BAD_REQUEST, 1003, "新账户换绑签名验证失败");
    }
    let Some(new_account_signature_bytes) =
        parse_signature_bytes(input.new_account_signature.as_str())
    else {
        return api_error(StatusCode::BAD_REQUEST, 1001, "换绑签名格式错误");
    };
    tracing::info!(
        cid_number = %cid_number,
        data_handover_authorized,
        "citizen rebind signatures verified"
    );

    let actor_cid_number = match active_registry_cid_number(&state) {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let call = encode_admin_rebind_cid_account_id_call(
        &actor_cid_number,
        actor_role_code.as_str(),
        cid_number.as_str(),
        &account_id_bytes,
        expected_binding_revision,
        authorization_expires_at,
        &new_account_signature_bytes,
    );
    let prepared = match chain_submit::prepare_signing(&call, ctx.account_id.as_str()).await {
        Ok(v) => v,
        Err(err) => {
            tracing::error!(error = %err, "prepare rebind signing failed");
            return api_error(
                StatusCode::BAD_GATEWAY,
                1004,
                "链签名载荷准备失败(链不可用)",
            );
        }
    };

    let issued_at = Utc::now();
    let action = crate::core::institution_call::chain_action_code(
        CITIZEN_IDENTITY_PALLET_INDEX,
        ADMIN_REBIND_CID_CALL_INDEX,
    );
    let sign_request = match crate::core::qr::build_sign_request_bytes(
        session.request_id.as_str(),
        issued_at.timestamp(),
        authorization_expires_at as i64,
        ctx.account_id.as_str(),
        &prepared.payload,
        action,
    ) {
        Ok(v) => v,
        Err(resp) => return resp,
    };

    let mut context = session.context.clone();
    if let Some(map) = context.as_object_mut() {
        map.insert(
            "new_account_id".to_string(),
            serde_json::Value::String(account_id_hex.clone()),
        );
    }
    if let Err(err) = state.db.promote_chain_sign_session(
        session.request_id.as_str(),
        PURPOSE_CITIZEN_ADMIN_REBIND,
        &call,
        prepared.nonce,
        prepared.signing_hash_hex.as_str(),
        &context,
    ) {
        tracing::error!(error = %err, "promote rebind session failed");
        return api_error(StatusCode::INTERNAL_SERVER_ERROR, 1004, "换绑会话转正失败");
    }

    Json(ApiResponse {
        code: 0,
        message: "ok".to_string(),
        data: SubmitCitizenRebindOutput {
            request_id: session.request_id,
            cid_number,
            sign_request,
            expires_at: authorization_expires_at as i64,
        },
    })
    .into_response()
}

/// 吊销 prepare:登记表墓碑(最严档 PasskeyColdSign grant,与身份上链同档)。
pub(crate) async fn prepare_citizen_revoke(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(cid_number): Path<String>,
    Json(input): Json<PrepareCitizenRevokeInput>,
) -> impl IntoResponse {
    let ctx = match require_admin_any(&state, &headers) {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    if let Err(resp) = ensure_registry_admin(&ctx) {
        return resp;
    }
    let actor_role_code = match validate_actor_role_code(input.actor_role_code.as_str()) {
        Ok(value) => value,
        Err(resp) => return resp,
    };
    let grant_payload = serde_json::json!({
        "cid_number": cid_number,
        "actor_role_code": actor_role_code,
        "op": "revoke",
    });
    let passkey = match require_admin_security_grant(
        &state,
        &headers,
        &ctx,
        AdminActionType::CitizenOnchainPush,
        cid_number.as_str(),
        Some(&grant_payload),
    ) {
        Ok(proof) => proof,
        Err(resp) => return resp,
    };
    match state.db.find_citizen_by_cid(cid_number.as_str()) {
        Ok(Some(_)) => {}
        Ok(None) => return api_error(StatusCode::NOT_FOUND, 1004, "公民档案不存在"),
        Err(err) => {
            tracing::error!(error = %err, "query citizen by cid failed");
            return api_error(StatusCode::INTERNAL_SERVER_ERROR, 1004, "公民档案查询失败");
        }
    }
    let actor_cid_number = match active_registry_cid_number(&state) {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let call = encode_revoke_cid_call(
        &actor_cid_number,
        actor_role_code.as_str(),
        cid_number.as_str(),
    );
    let prepared = match chain_submit::prepare_signing(&call, ctx.account_id.as_str()).await {
        Ok(v) => v,
        Err(err) => {
            tracing::error!(error = %err, "prepare revoke signing failed");
            return api_error(
                StatusCode::BAD_GATEWAY,
                1004,
                "链签名载荷准备失败(链不可用)",
            );
        }
    };
    let issued_at = Utc::now();
    let expires_at = issued_at + Duration::seconds(SESSION_TTL_SECS);
    let request_id = format!("citizen-revoke-{}", Uuid::new_v4());
    let action = crate::core::institution_call::chain_action_code(
        CITIZEN_IDENTITY_PALLET_INDEX,
        REVOKE_CID_CALL_INDEX,
    );
    let sign_request = match crate::core::qr::build_sign_request_bytes(
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
    let session = ChainSignSession {
        request_id: request_id.clone(),
        purpose: PURPOSE_CITIZEN_REVOKE.to_string(),
        account_id: ctx.account_id.clone(),
        call_data: call,
        nonce: prepared.nonce,
        signing_hash: prepared.signing_hash_hex.clone(),
        context: serde_json::json!({
            "cid_number": cid_number,
            "actor_role_code": actor_role_code,
        }),
        expires_at,
        consumed_at: None,
    };
    if let Err(err) = state.db.insert_chain_sign_session(&session, &passkey, &ctx.institution_cid_number) {
        tracing::error!(error = %err, "insert revoke session failed");
        return api_error(StatusCode::INTERNAL_SERVER_ERROR, 1004, "吊销会话落库失败");
    }
    crate::core::runtime_ops::append_audit_log(
        &state,
        "CITIZEN_REVOKE_PREPARE",
        &ctx.account_id,
        Some(cid_number.clone()),
        serde_json::json!({
            "cid_number": cid_number,
            "request_id": request_id,
            "actor_ip": actor_ip_from_headers(&headers),
        }),
    );
    Json(ApiResponse {
        code: 0,
        message: "ok".to_string(),
        data: PrepareCitizenRevokeOutput {
            request_id,
            cid_number,
            sign_request,
            expires_at: expires_at.timestamp(),
        },
    })
    .into_response()
}

#[cfg(test)]
// 测试需要在前置条件失效时立即失败，断言式解包仅限本测试模块。
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn submit_rebind_input_uses_exact_current_and_new_account_fields() {
        let current_account_id = format!("0x{}", "11".repeat(32));
        let new_account_id = format!("0x{}", "22".repeat(32));
        let input: SubmitCitizenRebindInput = serde_json::from_value(serde_json::json!({
            "request_id": "request-1",
            "current_account_id": current_account_id,
            "current_account_signature": format!("0x{}", "33".repeat(64)),
            "new_account_id": new_account_id,
            "new_account_signature": format!("0x{}", "44".repeat(64)),
        }))
        .expect("official account field names must deserialize");
        assert_eq!(
            input.current_account_id.as_deref(),
            Some(current_account_id.as_str())
        );
        assert_eq!(input.new_account_id, new_account_id);

        let alias_result = serde_json::from_value::<SubmitCitizenRebindInput>(serde_json::json!({
            "request_id": "request-1",
            "current_account_id": current_account_id,
            "current_account_signature": format!("0x{}", "33".repeat(64)),
            "new_account_id": format!("0x{}", "22".repeat(32)),
            "new_account_signature": format!("0x{}", "44".repeat(64)),
            "account_id": format!("0x{}", "22".repeat(32)),
            "rebind_signature": format!("0x{}", "44".repeat(64)),
        }));
        assert!(alias_result.is_err(), "废弃泛化字段不得作为兼容输入");
    }









    fn finalized_snapshot(
        cid_status: FinalizedCidStatus,
        account_id: Option<[u8; 32]>,
        binding_revision: Option<u64>,
        chain_now_seconds: u64,
    ) -> FinalizedCidBinding {
        FinalizedCidBinding {
            genesis_hash: [0xaau8; 32],
            finalized_block_hash: [0xbbu8; 32],
            finalized_block_number: 9,
            chain_now_seconds,
            cid_status,
            registrar_cid_number: (cid_status != FinalizedCidStatus::Missing)
                .then(|| b"CN001-CREG-0001".to_vec()),
            commitment: (cid_status != FinalizedCidStatus::Missing).then_some([0x55u8; 32]),
            residence_province_code: b"GD".to_vec(),
            residence_city_code: b"001".to_vec(),
            account_id,
            binding_revision,
            identity_version: 0,
            voting: None,
            candidate: None,
        }
    }

    #[test]
    fn authorization_expiry_uses_chain_time_and_keeps_runtime_margin() {
        // 服务端时钟不参与授权到期值；只以同一 finalized 快照中的链秒+300 签发。
        assert_eq!(authorization_expires_at(1_000), Some(1_300));
        assert!(authorization_is_live(1_000, 1_300));
        assert!(authorization_is_live(1_299, 1_300));
        assert!(!authorization_is_live(1_300, 1_300));
        // 即使传入未来超过 Runtime 最大窗口的时间也必须 fail-closed。
        assert!(!authorization_is_live(1_000, 1_601));
        assert_eq!(authorization_expires_at(u64::MAX), None);
    }

    #[test]
    fn revoked_or_inconsistent_cid_never_becomes_rebind_source() {
        let account_id = [0x11u8; 32];
        assert_eq!(
            finalized_snapshot(
                FinalizedCidStatus::Revoked,
                Some(account_id),
                Some(4),
                1_000
            )
            .active_binding(),
            None
        );
        assert_eq!(
            finalized_snapshot(FinalizedCidStatus::Active, Some(account_id), None, 1_000)
                .active_binding(),
            None
        );
        assert!(finalized_snapshot(FinalizedCidStatus::Missing, None, None, 1_000).is_unoccupied());
    }

    #[test]
    fn finalized_target_rejects_pending_failed_and_accepts_only_exact_target() {
        let current_account_id = [0x11u8; 32];
        let new_account_id = [0x22u8; 32];
        let expectation = FinalizedBindingExpectation {
            genesis_hash: [0xaau8; 32],
            current_binding: Some((current_account_id, 7)),
            target_account_id: new_account_id,
            target_binding_revision: 8,
            occupy_record: None,
            authorization_expires_at: 1_300,
        };
        // nonce 即使已被 txpool 推进，只要 finalized storage 仍是交易前绑定就只能等待。
        assert_eq!(
            finalized_target_state(
                &finalized_snapshot(
                    FinalizedCidStatus::Active,
                    Some(current_account_id),
                    Some(7),
                    1_100,
                ),
                &expectation,
            ),
            FinalizedTargetState::Pending
        );
        assert_eq!(
            finalized_target_state(
                &finalized_snapshot(
                    FinalizedCidStatus::Active,
                    Some(new_account_id),
                    Some(8),
                    1_200,
                ),
                &expectation,
            ),
            FinalizedTargetState::Confirmed
        );
        // 失败交易、并发换绑或确认超时都不得冒充成功。
        assert_eq!(
            finalized_target_state(
                &finalized_snapshot(
                    FinalizedCidStatus::Active,
                    Some(current_account_id),
                    Some(7),
                    1_300,
                ),
                &expectation,
            ),
            FinalizedTargetState::Conflict
        );
        assert_eq!(
            finalized_target_state(
                &finalized_snapshot(
                    FinalizedCidStatus::Active,
                    Some([0x33u8; 32]),
                    Some(8),
                    1_200,
                ),
                &expectation,
            ),
            FinalizedTargetState::Conflict
        );
    }

    #[test]
    fn finalized_occupy_target_requires_same_registrar_and_account_commitment() {
        let account_id = [0x22u8; 32];
        let registrar = b"CN001-CREG-0001".to_vec();
        let commitment = blake2_256(&account_id);
        let expectation = FinalizedBindingExpectation {
            genesis_hash: [0xaau8; 32],
            current_binding: None,
            target_account_id: account_id,
            target_binding_revision: 1,
            occupy_record: Some((registrar.clone(), commitment)),
            authorization_expires_at: 1_300,
        };
        let mut snapshot =
            finalized_snapshot(FinalizedCidStatus::Active, Some(account_id), Some(1), 1_200);
        snapshot.registrar_cid_number = Some(registrar);
        snapshot.commitment = Some(commitment);
        assert_eq!(
            finalized_target_state(&snapshot, &expectation),
            FinalizedTargetState::Confirmed
        );

        snapshot.registrar_cid_number = Some(b"CN999-FRG-0001".to_vec());
        assert_eq!(
            finalized_target_state(&snapshot, &expectation),
            FinalizedTargetState::Conflict
        );
        snapshot.registrar_cid_number = Some(b"CN001-CREG-0001".to_vec());
        snapshot.commitment = Some([0x99u8; 32]);
        assert_eq!(
            finalized_target_state(&snapshot, &expectation),
            FinalizedTargetState::Conflict
        );
    }

    /// 占号调用字节 golden:锁死链↔onchina 字节契约(pallet 10 / call 6 占即绑新签名)。
    /// 布局 = [10][6] Compact(len)+actor_cid ‖ Compact(len)+actor_role ‖ Compact(len)+cid
    ///        ‖ account_id(32 裸字节) ‖ expires_at(u64 LE) ‖ Compact(len)+occupy_signature。
    #[test]
    fn encode_occupy_cid_call_byte_golden() {
        let account_id = [0x11u8; 32];
        let occupy_signature = [0x22u8; 4];
        let out = encode_occupy_cid_call(
            "A",
            "B",
            "C",
            &account_id,
            0x0102_0304_0506_0708,
            &occupy_signature,
        );
        // 0a06 | 04 41 | 04 42 | 04 43 | 11*32 | 0807060504030201 | 10 | 22*4
        let expected = concat!(
            "0a06",
            "0441",
            "0442",
            "0443",
            "1111111111111111111111111111111111111111111111111111111111111111",
            "0807060504030201",
            "10",
            "22222222",
        );
        assert_eq!(hex::encode(out), expected);
    }

    /// 换绑调用字节 golden:pallet 10 / call 7,锁定 revision/expires 的 LE 顺序。
    #[test]
    fn encode_admin_rebind_cid_account_id_call_byte_golden() {
        let new_account_id = [0x11u8; 32];
        let new_account_signature = [0x22u8; 4];
        let out = encode_admin_rebind_cid_account_id_call(
            "A",
            "B",
            "C",
            &new_account_id,
            0x1112_1314_1516_1718,
            0x0102_0304_0506_0708,
            &new_account_signature,
        );
        // ... | new_account | revision LE | expires LE | Compact(sig len) | sig
        let expected = concat!(
            "0a07",
            "0441",
            "0442",
            "0443",
            "1111111111111111111111111111111111111111111111111111111111111111",
            "1817161514131211",
            "0807060504030201",
            "10",
            "22222222",
        );
        assert_eq!(hex::encode(out), expected);
    }

    /// 占号验签往返:真签名通过、篡改 cid 拒、换绑域验签器验占号签名拒(域分离防重放)。
    #[test]
    fn verify_occupy_signature_roundtrip_and_domain_separation() {
        let pair = sr25519::Pair::from_seed(&[7u8; 32]);
        let account = pair.public().0;
        let account_hex = format!("0x{}", hex::encode(account));
        let cid = "CN220-CTZN2-198805200-2026";
        let genesis_hash = [0xabu8; 32];
        let expires_at = 1_800_000_000;
        let payload = encode_cid_occupy_authorization(&genesis_hash, cid, &account, expires_at);
        let msg = primitives::sign::signing_message(primitives::sign::OP_SIGN_CID_OCCUPY, &payload);
        let sig_hex = format!("0x{}", hex::encode(pair.sign(&msg).0));

        assert!(verify_occupy_signature(
            account_hex.as_str(),
            &payload,
            sig_hex.as_str()
        ));
        // 篡改 CID、创世哈希、过期时间均拒。
        let changed_cid = encode_cid_occupy_authorization(
            &genesis_hash,
            "CN220-CTZN2-000000000-2026",
            &account,
            expires_at,
        );
        assert!(!verify_occupy_signature(
            account_hex.as_str(),
            &changed_cid,
            sig_hex.as_str()
        ));
        let changed_genesis =
            encode_cid_occupy_authorization(&[0xacu8; 32], cid, &account, expires_at);
        assert!(!verify_occupy_signature(
            account_hex.as_str(),
            &changed_genesis,
            sig_hex.as_str()
        ));
        let changed_expiry =
            encode_cid_occupy_authorization(&genesis_hash, cid, &account, expires_at + 1);
        assert!(!verify_occupy_signature(
            account_hex.as_str(),
            &changed_expiry,
            sig_hex.as_str()
        ));
        // 占号签名拿去过换绑域验签器 → 拒(0x12 与 0x1F 分离)。
        assert!(!verify_admin_rebind_signature(
            account_hex.as_str(),
            &payload,
            sig_hex.as_str()
        ));
    }

    /// 换绑验签往返 + 反向域分离。
    #[test]
    fn verify_admin_rebind_signature_roundtrip_and_domain_separation() {
        let pair = sr25519::Pair::from_seed(&[9u8; 32]);
        let account = pair.public().0;
        let account_hex = format!("0x{}", hex::encode(account));
        let cid = "CN330-NATP3-111111111-2026";
        let genesis_hash = [0xcdu8; 32];
        let current_account = [0x44u8; 32];
        let expected_binding_revision = 7;
        let expires_at = 1_800_000_000;
        let payload = encode_cid_rebind_authorization(
            &genesis_hash,
            cid,
            &current_account,
            &account,
            expected_binding_revision,
            expires_at,
        );
        let msg =
            primitives::sign::signing_message(primitives::sign::OP_SIGN_CID_ADMIN_REBIND, &payload);
        let sig_hex = format!("0x{}", hex::encode(pair.sign(&msg).0));

        assert!(verify_admin_rebind_signature(
            account_hex.as_str(),
            &payload,
            sig_hex.as_str()
        ));
        for changed in [
            encode_cid_rebind_authorization(
                &[0xceu8; 32],
                cid,
                &current_account,
                &account,
                expected_binding_revision,
                expires_at,
            ),
            encode_cid_rebind_authorization(
                &genesis_hash,
                cid,
                &[0x45u8; 32],
                &account,
                expected_binding_revision,
                expires_at,
            ),
            encode_cid_rebind_authorization(
                &genesis_hash,
                cid,
                &current_account,
                &account,
                expected_binding_revision + 1,
                expires_at,
            ),
            encode_cid_rebind_authorization(
                &genesis_hash,
                cid,
                &current_account,
                &account,
                expected_binding_revision,
                expires_at + 1,
            ),
        ] {
            assert!(!verify_admin_rebind_signature(
                account_hex.as_str(),
                &changed,
                sig_hex.as_str()
            ));
        }
        // 换绑签名拿去过占号域验签器 → 拒。
        assert!(!verify_occupy_signature(
            account_hex.as_str(),
            &payload,
            sig_hex.as_str()
        ));
    }

    #[test]
    fn verify_current_account_signature_requires_chain_current_account_and_own_domain() {
        let current_pair = sr25519::Pair::from_seed(&[7u8; 32]);
        let new_pair = sr25519::Pair::from_seed(&[8u8; 32]);
        let current_account = current_pair.public().0;
        let new_account = new_pair.public().0;
        let current_account_hex = format!("0x{}", hex::encode(current_account));
        let payload = encode_cid_rebind_authorization(
            &[0xaau8; 32],
            "CN330-NATP3-111111111-2026",
            &current_account,
            &new_account,
            7,
            1_800_000_000,
        );
        let current_message =
            primitives::sign::signing_message(primitives::sign::OP_SIGN_CID_REBIND, &payload);
        let current_account_signature =
            format!("0x{}", hex::encode(current_pair.sign(&current_message).0));

        assert!(verify_current_account_signature(
            current_account_hex.as_str(),
            &payload,
            current_account_signature.as_str()
        ));
        assert!(!verify_current_account_signature(
            format!("0x{}", hex::encode(new_account)).as_str(),
            &payload,
            current_account_signature.as_str()
        ));
        assert!(!verify_admin_rebind_signature(
            current_account_hex.as_str(),
            &payload,
            current_account_signature.as_str()
        ));
    }
}

/// 安全重提继续用原CID绑定期望核对；只有原目标仍待办理才允许重提。
pub(crate) async fn safe_module_resubmit(session: &ChainSignSession) -> bool {
    match finalized_binding_expectation(session) {
        Ok(Some((cid, expectation))) => match crate::core::chain_citizen_identity::read_finalized_citizen_identity(&cid).await {
            Ok(snapshot) => finalized_target_state(&snapshot, &expectation) == FinalizedTargetState::Pending,
            Err(error) => { tracing::error!(error = %error, "read CID state for resubmit failed"); false }
        },
        Ok(None) => true, Err(_) => false,
    }
}
/// 使用原绑定核验函数，错误响应及链上确认条件保持。
pub(crate) async fn verify_module_finalized_binding(session: &ChainSignSession, finalized: &crate::core::chain_submit::FinalizedChainSubmit) -> Result<Option<crate::core::chain_citizen_identity::FinalizedCitizenIdentity>, axum::response::Response> {
    let expectation = finalized_binding_expectation(session).map_err(|_| api_error(StatusCode::INTERNAL_SERVER_ERROR, 1004, "CID 绑定会话数据损坏"))?;
    let Some((cid, expectation)) = expectation else { return Ok(None); };
    verify_finalized_binding_at(&cid, &expectation, finalized.block_hash).await.map(Some).map_err(|error| {
        tracing::error!(error = %error, tx_hash = %finalized.tx_hash, "wait finalized CID binding failed");
        api_error(StatusCode::BAD_GATEWAY, 2004, &format!("交易已提交({})但 CID 绑定未 finalized: {error}", finalized.tx_hash))
    })
}
