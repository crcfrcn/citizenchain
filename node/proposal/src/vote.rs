use nodeclient::session::remember_chain_sign_session;
use nodeclient::session::ChainSignSession;
use nodeclient::signing::build_signing_payloads;
use nodeclient::signing::chain_action_code;
use nodeclient::signing::encode_compact_u32;
use nodeclient::signing::fetch_genesis_hash;
use nodeclient::signing::fetch_nonce;
use nodeclient::signing::fetch_runtime_version;
use nodeclient::signing::generate_request_id;
use nodeclient::signing::now_secs;
use nodeclient::signing::payload_b64;
use nodeclient::signing::public_key_b64;
use nodeclient::signing::sha256_hash;
use nodeclient::signing::signer_account_id_from_public_key;
use nodeclient::signing::QrSignRequest;
use nodeclient::signing::SignRequestBody;
use nodeclient::signing::VoteSignRequestResult;
use nodeclient::signing::DEFAULT_TTL_SECS;
use nodeclient::signing::IMMORTAL_SIGN_BLOCK_NUMBER;
use nodeclient::signing::QR_KIND_SIGN_REQUEST;
use nodeclient::signing::QR_V1;

// ──── 公开函数 ────

/// 构建内部投票（`internal_vote`）签名请求。
///
/// 内部投票统一走 `InternalVote::cast`(pallet=20, call=0)；个人多签使用
/// `Personal` 票据声明，机构使用 `InstitutionRole(role_code)` 票据声明。
/// 由投票引擎按 ProposalData 前缀自动分派到对应 `InternalVoteExecutor`。
///
/// Call 编码: `[0x14][0x00][proposal_id:u64_le][ticket_claim][approve:bool]`。
///
/// 返回 QR 签名请求 JSON + 请求 ID + 预期审阅 payload hash。
pub(crate) fn build_vote_sign_request(
    proposal_id: u64,
    signer_public_key: &str,
    voter_role_code: Option<&str>,
    approve: bool,
) -> Result<VoteSignRequestResult, String> {
    // 验证公钥格式
    let signer_public_key = nodedesktop::validation::normalize_public_key(signer_public_key)?;
    let public_key_bytes = hex::decode(signer_public_key.trim_start_matches("0x"))
        .map_err(|e| format!("公钥解码失败: {e}"))?;
    let signer_account_id = signer_account_id_from_public_key(&signer_public_key)?;

    // 获取链上参数
    let (spec_version, tx_version) = fetch_runtime_version()?;
    let genesis_hash = fetch_genesis_hash()?;
    let nonce = fetch_nonce(&signer_account_id)?;

    // ticket_claim SCALE：Personal=0；InstitutionRole=1 + RoleCode(BoundedVec)。
    let mut call_data = Vec::with_capacity(12 + voter_role_code.map(str::len).unwrap_or_default());
    call_data.push(20u8); // InternalVote sub-pallet index
    call_data.push(0u8); // cast call index
    call_data.extend_from_slice(&proposal_id.to_le_bytes());
    match voter_role_code {
        Some(role_code) => {
            if role_code.is_empty()
                || role_code.len() > entitybase::INSTITUTION_ROLE_CODE_MAX_BYTES as usize
            {
                return Err("voter_role_code 长度超出链上岗位码范围".to_string());
            }
            call_data.push(1u8);
            call_data.extend_from_slice(&encode_compact_u32(role_code.len() as u32));
            call_data.extend_from_slice(role_code.as_bytes());
        }
        None => call_data.push(0u8),
    }
    call_data.push(if approve { 1u8 } else { 0u8 });

    // 链交易签名材料只能由 runtime 类型构造，避免 node 冷签与热钱包
    // 交易路径在 TxExtension 或 additional_signed 字节上分叉。
    let (payload, signing_bytes) =
        build_signing_payloads(&call_data, &genesis_hash, nonce, spec_version, tx_version)?;

    // 计算审阅 payload hash 与实际签名字节 hash，分别用于 QR 会话和提交校验。
    let payload_hash = sha256_hash(&payload);
    let payload_hash_hex = hex::encode(payload_hash);
    let payload_hash = hex::encode(sha256_hash(&signing_bytes));

    // 生成请求 ID
    let request_id = generate_request_id("vote");

    let now = now_secs()?;
    let expires_at = now + DEFAULT_TTL_SECS;
    let request = QrSignRequest {
        proto: QR_V1.to_string(),
        kind: QR_KIND_SIGN_REQUEST,
        id: request_id.clone(),
        expires_at,
        body: SignRequestBody {
            action: chain_action_code(&call_data)?,
            sig_alg: 1,
            signer_public_key: public_key_b64(&public_key_bytes)?,
            payload: payload_b64(&payload),
        },
    };

    let request_json =
        serde_json::to_string(&request).map_err(|e| format!("序列化签名请求失败: {e}"))?;
    remember_chain_sign_session(
        request_id.clone(),
        ChainSignSession {
            expected_public_key: signer_public_key,
            call_data_hex: hex::encode(&call_data),
            payload_hash_hex: payload_hash_hex.clone(),
            payload_hash,
            nonce,
            expires_at,
        },
    )?;

    Ok(VoteSignRequestResult {
        request_json,
        call_data_hex: hex::encode(&call_data),
        request_id,
        expected_payload_hash: format!("0x{}", payload_hash_hex),
        sign_nonce: nonce,
        sign_block_number: IMMORTAL_SIGN_BLOCK_NUMBER,
    })
}

/// 构建 joint_vote 签名请求（联合投票内部投票阶段：pallet=21, call=0）。
///
/// JointVote pallet:`cast_admin` 在 21.0,
/// `cast_referendum` 在 21.1(联合公投阶段需双层凭证,本函数不覆盖)。
///
/// `actor_cid_number` 是联合投票机构身份的唯一主键，不派生或附带主账户。
pub(crate) fn build_joint_vote_sign_request(
    proposal_id: u64,
    signer_public_key: &str,
    actor_cid_number: &str,
    voter_role_code: &str,
    approve: bool,
) -> Result<VoteSignRequestResult, String> {
    let signer_public_key = nodedesktop::validation::normalize_public_key(signer_public_key)?;
    let public_key_bytes = hex::decode(signer_public_key.trim_start_matches("0x"))
        .map_err(|e| format!("公钥解码失败: {e}"))?;
    let signer_account_id = signer_account_id_from_public_key(&signer_public_key)?;

    if actor_cid_number.is_empty()
        || actor_cid_number.len() > primitives::core_const::CID_NUMBER_MAX_BYTES as usize
    {
        return Err("actor_cid_number 长度必须在链上 CID_NUMBER_MAX_BYTES 范围内".to_string());
    }
    if voter_role_code.is_empty()
        || voter_role_code.len() > entitybase::INSTITUTION_ROLE_CODE_MAX_BYTES as usize
    {
        return Err("voter_role_code 长度超出链上岗位码范围".to_string());
    }

    let (spec_version, tx_version) = fetch_runtime_version()?;
    let genesis_hash = fetch_genesis_hash()?;
    let nonce = fetch_nonce(&signer_account_id)?;

    // call data: [21][0][proposal_id][cid_number][voter_role_code][approve]
    let mut call_data = Vec::with_capacity(14 + actor_cid_number.len() + voter_role_code.len());
    call_data.push(21u8); // JointVote sub-pallet index
    call_data.push(0u8); // cast_admin call index
    call_data.extend_from_slice(&proposal_id.to_le_bytes());
    call_data.extend_from_slice(&encode_compact_u32(actor_cid_number.len() as u32));
    call_data.extend_from_slice(actor_cid_number.as_bytes());
    call_data.extend_from_slice(&encode_compact_u32(voter_role_code.len() as u32));
    call_data.extend_from_slice(voter_role_code.as_bytes());
    call_data.push(if approve { 1u8 } else { 0u8 });

    let (payload, signing_bytes) =
        build_signing_payloads(&call_data, &genesis_hash, nonce, spec_version, tx_version)?;
    let payload_hash = sha256_hash(&payload);
    let payload_hash_hex = hex::encode(payload_hash);
    let payload_hash = hex::encode(sha256_hash(&signing_bytes));
    let request_id = generate_request_id("jvote");

    let now = now_secs()?;
    let expires_at = now + DEFAULT_TTL_SECS;
    let request = QrSignRequest {
        proto: QR_V1.to_string(),
        kind: QR_KIND_SIGN_REQUEST,
        id: request_id.clone(),
        expires_at,
        body: SignRequestBody {
            action: chain_action_code(&call_data)?,
            sig_alg: 1,
            signer_public_key: public_key_b64(&public_key_bytes)?,
            payload: payload_b64(&payload),
        },
    };

    let request_json =
        serde_json::to_string(&request).map_err(|e| format!("序列化签名请求失败: {e}"))?;
    remember_chain_sign_session(
        request_id.clone(),
        ChainSignSession {
            expected_public_key: signer_public_key,
            call_data_hex: hex::encode(&call_data),
            payload_hash_hex: payload_hash_hex.clone(),
            payload_hash,
            nonce,
            expires_at,
        },
    )?;

    Ok(VoteSignRequestResult {
        request_json,
        call_data_hex: hex::encode(&call_data),
        request_id,
        expected_payload_hash: format!("0x{}", payload_hash_hex),
        sign_nonce: nonce,
        sign_block_number: IMMORTAL_SIGN_BLOCK_NUMBER,
    })
}
