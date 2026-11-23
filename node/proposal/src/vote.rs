//! 投票专用的冻结资格、原签名会话及恢复；不授予现任业务权限。
use crate::types::{FrozenVoteBinding, FrozenVoteView, FrozenVoter, ProposalVoteSubmitResult};
use codec::{DecodeAll, Encode};
use nodeclient::session::{remember_chain_sign_session, ChainSignSession};
use nodeclient::signing::*;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

#[derive(Clone)]
struct VoteContext {
    binding: FrozenVoteBinding,
    session: ChainSignSession,
    response: Option<String>,
    tx_hash: Option<String>,
    busy: bool,
}
static VOTE_CONTEXTS: OnceLock<Mutex<HashMap<String, VoteContext>>> = OnceLock::new();
fn contexts() -> &'static Mutex<HashMap<String, VoteContext>> {
    VOTE_CONTEXTS.get_or_init(|| Mutex::new(HashMap::new()))
}

pub(crate) fn frozen_ticket<'a>(
    view: &'a FrozenVoteView,
    binding: &FrozenVoteBinding,
) -> Result<&'a FrozenVoter, String> {
    if view.genesis_hash != binding.genesis_hash {
        return Err("原签名会话不属于当前链".into());
    }
    if let Some(error) = &view.scope_error {
        return Err(error.clone());
    }
    if view.kind == 0 && view.actor_cid_number.as_deref() != Some(&binding.cid_number) {
        return Err("机构身份CID号与原提案不匹配".into());
    }
    let role = view
        .roles
        .iter()
        .find(|r| r.cid_number == binding.cid_number && r.role_code == binding.role_code)
        .ok_or("原提案没有此岗位票据")?;
    if let Some(error) = &role.error {
        return Err(error.clone());
    }
    let voter = role
        .voters
        .iter()
        .find(|v| v.account_id == binding.account_id)
        .ok_or("此钱包不在原提案冻结名单，新钱包不能继承旧票")?;
    if voter.wallet_revision.as_deref() != Some(&binding.wallet_revision) {
        return Err("冻结钱包版本缺失或不匹配".into());
    }
    if let Some(error) = &voter.vote_error {
        return Err(error.clone());
    }
    Ok(voter)
}
pub(crate) fn ensure_open(view: &FrozenVoteView) -> Result<(), String> {
    if view.status != 0 || !matches!((view.kind, view.stage), (0, 0) | (1, 1)) {
        return Err("本提案当前不处于机构投票阶段".into());
    }
    let height = view
        .block_number
        .parse::<u64>()
        .map_err(|_| "最终块高度损坏")?;
    let end = view.end_block.parse::<u64>().map_err(|_| "投票期限损坏")?;
    if height > end {
        return Err("原提案投票已到期".into());
    }
    Ok(())
}
fn prepare_binding(
    pid: u128,
    wallet: &str,
    cid: Option<&str>,
    role: &str,
    kind: u8,
) -> Result<FrozenVoteBinding, String> {
    let view = crate::query::load_frozen_vote_view(pid)?;
    if view.kind != kind {
        return Err("投票类型与原提案不匹配".into());
    }
    let cid = cid
        .map(str::to_string)
        .or(view.actor_cid_number.clone())
        .ok_or("原机构身份CID号缺失")?;
    let binding = FrozenVoteBinding {
        genesis_hash: view.genesis_hash.clone(),
        proposal_id: pid.to_string(),
        cid_number: cid.clone(),
        role_code: role.into(),
        account_id: wallet.into(),
        wallet_revision: view
            .roles
            .iter()
            .find(|r| r.cid_number == cid && r.role_code == role)
            .and_then(|r| r.voters.iter().find(|v| v.account_id == wallet))
            .and_then(|v| v.wallet_revision.clone())
            .ok_or("冻结身份或版本资料缺失")?,
    };
    if frozen_ticket(&view, &binding)?.vote.is_some() {
        return Err("该冻结票据已经投票".into());
    }
    ensure_open(&view)?;
    Ok(binding)
}
fn build_request(
    pid: u128,
    wallet: &str,
    call_data: Vec<u8>,
    binding: Option<FrozenVoteBinding>,
) -> Result<VoteSignRequestResult, String> {
    let wallet = nodedesktop::validation::normalize_account_id(wallet)?;
    let account: [u8; 32] = hex::decode(wallet.trim_start_matches("0x"))
        .map_err(|_| "账户编码无效")?
        .try_into()
        .map_err(|_| "请选择32字节钱包账户ID，不能用PQ公钥充当票据账户")?;
    let (spec, tx) = fetch_runtime_version()?;
    let genesis = fetch_genesis_hash()?;
    let nonce = fetch_nonce(&wallet)?;
    if binding
        .as_ref()
        .is_some_and(|b| b.genesis_hash != format!("0x{}", hex::encode(genesis)))
    {
        return Err("准备期间链已切换".into());
    }
    let (payload, signing) =
        build_signing_payloads(&account, &call_data, &genesis, nonce, spec, tx)?;
    let request_id = generate_request_id("vote");
    let expires_at = now_secs()? + DEFAULT_TTL_SECS;
    let payload_hash_hex = hex::encode(sha256_hash(&payload));
    let session = ChainSignSession {
        expected_public_key: wallet.clone(),
        call_data_hex: hex::encode(&call_data),
        payload_hash_hex: payload_hash_hex.clone(),
        payload_hash: hex::encode(sha256_hash(&signing)),
        nonce,
        expires_at,
    };
    let request = QrSignRequest {
        proto: QR_V1.into(),
        kind: QR_KIND_SIGN_REQUEST,
        id: request_id.clone(),
        expires_at,
        body: SignRequestBody {
            action: chain_action_code(&call_data)?,
            sig_alg: 1,
            signer_public_key: public_key_b64(&account)?,
            payload: payload_b64(&payload),
        },
    };
    if let Some(binding) = binding {
        let mut items = contexts().lock().map_err(|_| "投票会话状态异常")?;
        let now = now_secs()?;
        items.retain(|_, c| c.response.is_some() || c.session.expires_at >= now);
        if items
            .values()
            .any(|c| c.binding == binding && c.response.is_some())
        {
            return Err("此票存在原签名会话，请先恢复，不得重复准备".into());
        }
        if items.len() >= 256 {
            return Err("待处理投票会话已达上限，请先恢复原会话".into());
        }
        items.insert(
            request_id.clone(),
            VoteContext {
                binding,
                session: session.clone(),
                response: None,
                tx_hash: None,
                busy: false,
            },
        );
    }
    remember_chain_sign_session(request_id.clone(), session)?;
    let _ = pid;
    Ok(VoteSignRequestResult {
        request_json: serde_json::to_string(&request).map_err(|e| e.to_string())?,
        call_data_hex: hex::encode(call_data),
        request_id,
        expected_payload_hash: format!("0x{payload_hash_hex}"),
        sign_nonce: nonce,
        sign_block_number: IMMORTAL_SIGN_BLOCK_NUMBER,
    })
}

pub(crate) fn build_vote_sign_request(
    pid: u128,
    wallet: &str,
    role: Option<&str>,
    approve: bool,
) -> Result<VoteSignRequestResult, String> {
    let wallet = nodedesktop::validation::normalize_account_id(wallet)?;
    let binding = match role {
        Some(role) => Some(prepare_binding(pid, &wallet, None, role, 0)?),
        None => {
            let meta = crate::query::fetch_proposal_meta(pid)?.ok_or("原提案不存在")?;
            if meta.actor_cid_number.is_some() {
                return Err("机构投票必须选择原岗位".into());
            }
            None
        }
    };
    let mut call = vec![20, 0];
    call.extend(pid.to_le_bytes());
    if let Some(role) = role {
        call.push(1);
        call.extend(role.as_bytes().to_vec().encode());
    } else {
        call.push(0);
    }
    call.push(u8::from(approve));
    build_request(pid, &wallet, call, binding)
}
pub(crate) fn build_joint_vote_sign_request(
    pid: u128,
    wallet: &str,
    cid: &str,
    role: &str,
    approve: bool,
) -> Result<VoteSignRequestResult, String> {
    let wallet = nodedesktop::validation::normalize_account_id(wallet)?;
    let binding = prepare_binding(pid, &wallet, Some(cid), role, 1)?;
    let mut call = vec![21, 0];
    call.extend(pid.to_le_bytes());
    call.extend(cid.as_bytes().to_vec().encode());
    call.extend(role.as_bytes().to_vec().encode());
    call.push(u8::from(approve));
    build_request(pid, &wallet, call, Some(binding))
}

/// 严格消费原投票调用，不能从投票入口提交其他签名交易。
pub(crate) fn decode_vote_call(
    call: &[u8],
) -> Result<(u128, Option<String>, Option<String>, bool), String> {
    if call.get(1) != Some(&0) {
        return Err("不是原投票调用".into());
    }
    match call.first() {
        Some(20) => {
            let (pid, claim, approve): (u128, VoteClaim, bool) =
                DecodeAll::decode_all(&mut &call[2..])
                    .map_err(|e| format!("原内部票据损坏: {e}"))?;
            Ok((
                pid,
                None,
                match claim {
                    VoteClaim::Personal => None,
                    VoteClaim::InstitutionRole(role) => {
                        Some(String::from_utf8(role).map_err(|_| "岗位标识损坏")?)
                    }
                },
                approve,
            ))
        }
        Some(21) => {
            let (pid, cid, role, approve): (u128, Vec<u8>, Vec<u8>, bool) =
                DecodeAll::decode_all(&mut &call[2..]).map_err(|e| e.to_string())?;
            Ok((
                pid,
                Some(String::from_utf8(cid).map_err(|_| "机构CID损坏")?),
                Some(String::from_utf8(role).map_err(|_| "岗位损坏")?),
                approve,
            ))
        }
        _ => Err("投票入口不接受其他业务交易".into()),
    }
}
#[derive(codec::Decode)]
enum VoteClaim {
    Personal,
    InstitutionRole(Vec<u8>),
}

pub(crate) fn submit_original_vote(
    request_id: &str,
    expected_wallet: &str,
    hash: &str,
    call: &[u8],
    nonce: u32,
    block: u64,
    response: &str,
    binding: Option<FrozenVoteBinding>,
) -> Result<ProposalVoteSubmitResult, String> {
    let (pid, cid, role, approve) = decode_vote_call(call)?;
    let wallet = nodedesktop::validation::normalize_account_id(expected_wallet)?;
    let Some(role) = role else {
        if binding.is_some() {
            return Err("个人票不接收机构资格".into());
        }
        return verify_and_submit(request_id, &wallet, hash, call, nonce, block, response).map(
            |v| ProposalVoteSubmitResult {
                tx_hash: Some(v.tx_hash),
                observed: false,
            },
        );
    };
    let binding = binding.ok_or("机构投票必须绑定原冻结身份")?;
    if binding.proposal_id != pid.to_string()
        || binding.account_id != wallet
        || binding.role_code != role
        || cid.as_ref().is_some_and(|c| c != &binding.cid_number)
    {
        return Err("提交调用与原冻结票据不一致".into());
    }
    let view = crate::query::load_frozen_vote_view(pid)?;
    let entry = frozen_ticket(&view, &binding)?;
    if let Some(recorded) = entry.vote {
        if recorded != approve {
            return Err("原票据已记录另一投票选择，不能冒充本次成功".into());
        }
        let tx = contexts()
            .lock()
            .map_err(|_| "投票会话异常")?
            .remove(request_id)
            .and_then(|v| v.tx_hash);
        return Ok(ProposalVoteSubmitResult {
            tx_hash: tx,
            observed: true,
        });
    }
    ensure_open(&view)?;
    let mut context = {
        let mut items = contexts().lock().map_err(|_| "投票会话异常")?;
        let item = items
            .get_mut(request_id)
            .ok_or("原签名会话不在本机；保留原记录，不自动重建")?;
        if item.binding != binding
            || item.session.expected_public_key != wallet
            || item.session.call_data_hex != hex::encode(call)
            || item.session.nonce != nonce
            || item.session.payload_hash_hex != hash.trim_start_matches("0x")
            || block != IMMORTAL_SIGN_BLOCK_NUMBER
        {
            return Err("提交参数与原签名会话不一致".into());
        }
        if item.busy {
            return Err("原投票会话正在处理，请等待确认".into());
        }
        if let Some(tx) = &item.tx_hash {
            return Ok(ProposalVoteSubmitResult {
                tx_hash: Some(tx.clone()),
                observed: false,
            });
        }
        let reply: QrSignResponse = serde_json::from_str(response).map_err(|e| e.to_string())?;
        if reply.proto != QR_V1
            || reply.kind != QR_KIND_SIGN_RESPONSE
            || reply.id != request_id
            || reply.body.signer_public_key != wallet
            || reply.expires_at != item.session.expires_at
        {
            return Err("响应不属于原钱包或原请求".into());
        }
        if item.response.as_deref().is_some_and(|r| r != response) {
            return Err("恢复必须使用原响应和原签名".into());
        }
        item.response = Some(response.into());
        item.busy = true;
        item.clone()
    };
    // 恢复只重新登记原材料，绝不取新nonce、换岗或重建已签调用。
    let result = remember_chain_sign_session(request_id.into(), context.session.clone())
        .and_then(|_| verify_and_submit(request_id, &wallet, hash, call, nonce, block, response));
    context.busy = false;
    if let Ok(v) = &result {
        context.tx_hash = Some(v.tx_hash.clone());
    }
    contexts()
        .lock()
        .map_err(|_| "投票会话异常")?
        .insert(request_id.into(), context);
    result.map(|v| ProposalVoteSubmitResult {
        tx_hash: Some(v.tx_hash),
        observed: false,
    })
}

/// 仅按最终块的同一票据和选择释放已确认本机会话，未知/反向记录不清理。
pub(crate) fn observe_completed_contexts(pid: u128, view: &FrozenVoteView) -> Result<(), String> {
    contexts()
        .lock()
        .map_err(|_| "投票会话状态异常")?
        .retain(|_, item| {
            if item.busy
                || item.binding.proposal_id != pid.to_string()
                || item.binding.genesis_hash != view.genesis_hash
            {
                return true;
            }
            let recorded = frozen_ticket(view, &item.binding).ok().and_then(|v| v.vote);
            let approve = hex::decode(&item.session.call_data_hex)
                .ok()
                .and_then(|c| decode_vote_call(&c).ok())
                .map(|v| v.3);
            recorded.is_none() || recorded != approve
        });
    Ok(())
}
