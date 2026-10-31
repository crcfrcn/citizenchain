//! 大屏专属链读:本机构活跃提案 ID 列表 + 某提案的逐席院内投票映射。
//!
//! 复用 chain_runtime 读链范式(subxt dynamic + 严格镜像 decode)。
//! - 活跃提案:点查 `VotingEngine::ActiveProposalsBySubject[InstitutionCid(cid_number)]`
//!   → `BoundedVec<u64>`(与 `Vec<u64>` 同编码)。
//! - 逐席投票：按 `proposal_id` 迭代 `RepresentativeVotesByTicket`，再按当前代表机构索引过滤。

use std::collections::HashMap;

use codec::{Decode, Encode};
use subxt::dynamic;

use onchina::chain_url;

#[derive(Decode, Encode)]
struct RoleSubjectMirror {
    cid_number: Vec<u8>,
    role_code: Vec<u8>,
}

#[derive(Decode, Encode)]
struct InstitutionVoteTicketMirror {
    role_subject: RoleSubjectMirror,
    voter_account_id: [u8; 32],
}

fn decode_representative_ticket_storage_key(
    key_bytes: &[u8],
) -> Result<(u32, InstitutionVoteTicketMirror), String> {
    // StorageDoubleMap 固定前缀 32；第一键 Blake2_128Concat 为 16+8；
    // 第二键哈希 16，之后才是 `(body_index, ticket)` 原始 SCALE 编码。
    let mut input = key_bytes
        .get(72..)
        .ok_or_else(|| "representative ticket storage key is too short".to_string())?;
    let decoded = <(u32, InstitutionVoteTicketMirror)>::decode(&mut input)
        .map_err(|e| format!("decode RepresentativeVotesByTicket key failed: {e}"))?;
    if !input.is_empty() {
        return Err("RepresentativeVotesByTicket key has trailing bytes".to_string());
    }
    Ok(decoded)
}

/// 读取某机构的活跃提案 ID 列表(点查 `ActiveProposalsBySubject`;键不存在=空)。
///
/// 值为 `BoundedVec<u64, MaxActiveProposals>`(ValueQuery),点查缺省返回空。
/// 该列表混合该机构所有种类活跃提案,是否为立法案由上层按 `LegProposalState.kind` 过滤。
pub(crate) async fn fetch_active_proposal_ids(cid_number: &str) -> Result<Vec<u64>, String> {
    let subject_key = proposal_subject_institution_cid_key(cid_number)?;
    let ws_url = chain_url::chain_ws_url()?;
    let client = chain_url::online_client(ws_url.as_str())
        .await
        .map_err(|e| format!("connect chain ws for active proposals failed: {e}"))?;
    let storage = client
        .storage()
        .at_latest()
        .await
        .map_err(|e| format!("get latest chain storage failed: {e}"))?;
    let address = dynamic::storage(
        "VotingEngine",
        "ActiveProposalsBySubject",
        vec![dynamic::Value::from_bytes(subject_key)],
    );
    let Some(thunk) = storage
        .fetch(&address)
        .await
        .map_err(|e| format!("fetch ActiveProposalsBySubject failed: {e}"))?
    else {
        return Ok(Vec::new());
    };
    let mut raw = thunk.encoded();
    Vec::<u64>::decode(&mut raw).map_err(|e| format!("decode ActiveProposalsBySubject failed: {e}"))
}

/// `ProposalSubject::InstitutionCid(CidNumber)` 的 SCALE 键编码。
///
/// 链端 enum 变体 0 = InstitutionCid,其后是 `BoundedVec<u8>` 的
/// Compact 长度 + UTF-8 CID 字节。不要把主账户 AccountId 当作提案归属键。
fn proposal_subject_institution_cid_key(cid_number: &str) -> Result<Vec<u8>, String> {
    let cid_number = cid_number.trim();
    if cid_number.is_empty() {
        return Err("institution cid_number is required".to_string());
    }
    let mut key = vec![0_u8];
    key.extend(cid_number.as_bytes().to_vec().encode());
    Ok(key)
}

/// 一张票的完整身份；同钱包不同岗位、机构、提案或院次绝不相互覆盖。
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub(crate) struct BallotKey {
    pub(crate) proposal_id: u64,
    pub(crate) body_index: u32,
    pub(crate) institution_cid_number: String,
    pub(crate) role_code: String,
    pub(crate) account_id: String,
}

impl BallotKey {
    pub(crate) fn seat_id(&self) -> String {
        // JSON 数组避免分隔符碰撞，仅用于 React 内部身份，不改变席位展示。
        serde_json::json!([
            self.proposal_id,
            self.body_index,
            self.institution_cid_number,
            self.role_code,
            self.account_id
        ])
        .to_string()
    }
}

/// 读取某提案当前代表机构的逐席投票。
///
/// 二级键是 `(body_index, InstitutionVoteTicket)`；同一钱包的多个岗位票据独立保留。
pub(crate) async fn fetch_representative_ballots(
    proposal_id: u64,
    current_body: u32,
) -> Result<HashMap<BallotKey, bool>, String> {
    let ws_url = chain_url::chain_ws_url()?;
    let client = chain_url::online_client(ws_url.as_str())
        .await
        .map_err(|e| format!("connect chain ws for representative ballots failed: {e}"))?;
    let storage = client
        .storage()
        .at_latest()
        .await
        .map_err(|e| format!("get latest chain storage failed: {e}"))?;
    // 部分键为 proposal_id，二级键保留院次与完整机构岗位票据。
    let query = dynamic::storage(
        "LegislationVote",
        "RepresentativeVotesByTicket",
        vec![dynamic::Value::u128(proposal_id as u128)],
    );
    let mut iter = storage
        .iter(query)
        .await
        .map_err(|e| format!("iterate RepresentativeVotesByTicket failed: {e}"))?;
    let mut ballots = HashMap::new();
    while let Some(entry) = iter.next().await {
        let kv = entry.map_err(|e| format!("read RepresentativeVotesByTicket failed: {e}"))?;
        let (body_index, ticket) = decode_representative_ticket_storage_key(&kv.key_bytes)?;
        if body_index != current_body {
            continue;
        }
        let mut raw = kv.value.encoded();
        let approve = bool::decode(&mut raw)
            .map_err(|e| format!("decode RepresentativeVotesByTicket value failed: {e}"))?;
        if !raw.is_empty() {
            return Err("representative ballot value has trailing bytes".into());
        }
        let key = BallotKey {
            proposal_id,
            body_index,
            institution_cid_number: String::from_utf8(ticket.role_subject.cid_number)
                .map_err(|_| "ballot CID is not UTF-8")?,
            role_code: String::from_utf8(ticket.role_subject.role_code)
                .map_err(|_| "ballot role is not UTF-8")?,
            account_id: format!("0x{}", hex::encode(ticket.voter_account_id)),
        };
        if ballots.insert(key, approve).is_some() {
            return Err("duplicate representative ballot ticket".into());
        }
    }
    Ok(ballots)
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
#[path = "../tests/display_chain_read.rs"]
mod tests;
