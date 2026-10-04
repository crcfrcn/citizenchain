use super::call_data;
use crate::governance::signing::{
    build_signing_payloads, chain_action_code, fetch_genesis_hash, fetch_nonce,
    fetch_runtime_version, generate_request_id, now_secs, payload_b64, public_key_b64,
    remember_chain_sign_request_session, sha256_hash, signer_account_id_from_public_key,
    QrSignRequest, SignRequestBody, VoteSignRequestResult, DEFAULT_TTL_SECS,
    IMMORTAL_SIGN_BLOCK_NUMBER, QR_KIND_SIGN_REQUEST, QR_V1,
};

fn normalize_signer_public_key(signer_public_key: &str) -> Result<(String, Vec<u8>), String> {
    let signer_public_key = crate::shared::validation::normalize_public_key(signer_public_key)?;
    let signer_public_key_bytes = hex::decode(signer_public_key.trim_start_matches("0x"))
        .map_err(|e| format!("公钥解码失败: {e}"))?;
    Ok((signer_public_key, signer_public_key_bytes))
}

fn build_hashed_payload_request(
    request_prefix: &str,
    signer_public_key_clean: &str,
    signer_public_key_bytes: &[u8],
    call_data: &[u8],
) -> Result<VoteSignRequestResult, String> {
    let (spec_version, tx_version) = fetch_runtime_version()?;
    let genesis_hash = fetch_genesis_hash()?;
    let signer_account_id = signer_account_id_from_public_key(signer_public_key_clean)?;
    let nonce = fetch_nonce(&signer_account_id)?;

    let (full_payload, payload_for_qr) =
        build_signing_payloads(call_data, &genesis_hash, nonce, spec_version, tx_version)?;
    let request_id = generate_request_id(request_prefix);

    // Runtime WASM 交易 payload 远大于 QR 承载能力，是 QR_V1 唯一 hash-only 例外。
    // `full_payload` 只留在本地 session 校验；QR `b.d` 发送 `signing_bytes`，
    // 与 Substrate sr25519 实际签名输入完全一致，禁止 runtime-upgrade 另起哈希规则。
    // expected_payload_hash 必须对应 QR 中实际发送的 hash-only payload。
    let payload_hash = sha256_hash(&payload_for_qr);
    let payload_hash_hex = hex::encode(payload_hash);
    let full_payload_hash_hex = hex::encode(sha256_hash(&full_payload));

    let now = now_secs()?;
    let expires_at = now + DEFAULT_TTL_SECS;
    let request = QrSignRequest {
        proto: QR_V1.to_string(),
        kind: QR_KIND_SIGN_REQUEST,
        id: request_id.clone(),
        expires_at,
        body: SignRequestBody {
            action: chain_action_code(call_data)?,
            sig_alg: 1,
            signer_public_key: public_key_b64(signer_public_key_bytes)?,
            payload: payload_b64(&payload_for_qr),
        },
    };

    let request_json =
        serde_json::to_string(&request).map_err(|e| format!("序列化签名请求失败: {e}"))?;
    remember_chain_sign_request_session(
        &request_id,
        signer_public_key_clean,
        call_data,
        &full_payload_hash_hex,
        &payload_hash_hex,
        nonce,
        expires_at,
    )?;

    Ok(VoteSignRequestResult {
        request_json,
        call_data_hex: hex::encode(call_data),
        request_id,
        expected_payload_hash: format!("0x{}", payload_hash_hex),
        sign_nonce: nonce,
        sign_block_number: IMMORTAL_SIGN_BLOCK_NUMBER,
    })
}

/// 构建运行期协议升级提案签名请求。
pub(crate) fn build_propose_runtime_upgrade_sign_request(
    signer_public_key: &str,
    actor_cid_number: &str,
    wasm_path: &str,
    reason: &str,
    pow_params: pow_difficulty::PowDifficultyParams,
) -> Result<VoteSignRequestResult, String> {
    let (signer_public_key_clean, signer_public_key_bytes) =
        normalize_signer_public_key(signer_public_key)?;
    let (wasm_code, _wasm_size_mb) = call_data::read_wasm(wasm_path)?;
    let call_data =
        call_data::propose_runtime_upgrade(actor_cid_number, &wasm_code, reason, pow_params)?;

    build_hashed_payload_request(
        "upgrade",
        &signer_public_key_clean,
        &signer_public_key_bytes,
        &call_data,
    )
}
