//! 读取本机构活跃立法提案、代表席位和逐席投票映射。

use std::collections::HashMap;

use codec::{Decode, DecodeAll, Encode};
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
    person_cid: Vec<u8>,
}

fn decode_representative_ticket_storage_key(
    key_bytes: &[u8],
) -> Result<(u32, InstitutionVoteTicketMirror), String> {
    // StorageDoubleMap 固定前缀 32；第一键 Blake2_128Concat 为 16+16(u128)；
    // 第二键哈希 16，之后才是 `(body_index, ticket)` 原始 SCALE 编码。
    let mut input = key_bytes
        .get(80..)
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
/// 值为 `BoundedVec<u128, MaxActiveProposals>`(ValueQuery),点查缺省返回空。
/// 该列表混合该机构所有种类活跃提案,是否为立法案由上层按 `LegProposalState.kind` 过滤。
pub(crate) async fn fetch_active_proposal_ids(
    cid_number: &str,
    at: [u8; 32],
) -> Result<Vec<u128>, String> {
    let subject_key = proposal_subject_institution_cid_key(cid_number)?;
    let ws_url = chain_url::chain_ws_url()?;
    let client = chain_url::online_client(ws_url.as_str())
        .await
        .map_err(|e| format!("connect chain ws for active proposals failed: {e}"))?;
    let storage = client.storage().at(subxt::utils::H256(at));
    let address = dynamic::storage(
        "VotingEngine",
        "ActiveProposalsBySubject",
        vec![subject_key],
    );
    let Some(thunk) = storage
        .fetch(&address)
        .await
        .map_err(|e| format!("fetch ActiveProposalsBySubject failed: {e}"))?
    else {
        return Ok(Vec::new());
    };
    decode_active_proposal_ids(thunk.encoded())
}

fn decode_active_proposal_ids(bytes: &[u8]) -> Result<Vec<u128>, String> {
    Vec::<u128>::decode_all(&mut &bytes[..])
        .map_err(|e| format!("decode ActiveProposalsBySubject failed: {e}"))
}

/// `ProposalSubject::InstitutionCid(CidNumber)` 的 SCALE 键编码。
///
/// 链端 enum 变体 0 = InstitutionCid,其后是 `BoundedVec<u8>` 的
/// Compact 长度 + UTF-8 CID 字节。不要把主账户 AccountId 当作提案归属键。
fn proposal_subject_institution_cid_key(cid_number: &str) -> Result<dynamic::Value, String> {
    if cid_number.is_empty() || cid_number.trim() != cid_number {
        return Err("institution CID is required".into());
    }
    Ok(dynamic::Value::unnamed_variant(
        "InstitutionCid",
        vec![dynamic::Value::from_bytes(cid_number.as_bytes())],
    ))
}

/// 一张票的完整身份；同钱包不同岗位、机构、提案或院次绝不相互覆盖。
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub(crate) struct BallotKey {
    pub(crate) proposal_id: u128,
    pub(crate) body_index: u32,
    pub(crate) institution_cid_number: String,
    pub(crate) role_code: String,
    pub(crate) person_cid: String,
}

impl BallotKey {
    pub(crate) fn seat_id(&self) -> String {
        // JSON 数组避免分隔符碰撞，仅用于 React 内部身份，不改变席位展示。
        serde_json::json!([
            self.proposal_id,
            self.body_index,
            self.institution_cid_number,
            self.role_code,
            self.person_cid
        ])
        .to_string()
    }
}

/// 读取某提案当前代表机构的逐席投票。
///
/// 二级键是 `(body_index, InstitutionVoteTicket)`；同一钱包的多个岗位票据独立保留。
pub(crate) async fn fetch_representative_ballots(
    proposal_id: u128,
    current_body: u32,
    at: [u8; 32],
) -> Result<HashMap<BallotKey, bool>, String> {
    let ws_url = chain_url::chain_ws_url()?;
    let client = chain_url::online_client(ws_url.as_str())
        .await
        .map_err(|e| format!("connect chain ws for representative ballots failed: {e}"))?;
    let storage = client.storage().at(subxt::utils::H256(at));
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
            person_cid: String::from_utf8(ticket.person_cid).map_err(|_| "投票人员CID非UTF-8")?,
        };
        if ballots.insert(key, approve).is_some() {
            return Err("duplicate representative ballot ticket".into());
        }
    }
    Ok(ballots)
}

#[derive(Debug, Clone)]
pub(crate) struct FrozenSeat {
    pub body_index: u32,
    pub person_cid: String,
    pub role_code: String,
    pub role_name: String,
    pub account_id: Option<String>,
    pub name: String,
}
/// 一张提案的指定代表岗位冻结人员；人员失去现任或当前绑定不删除其原票据。
pub(crate) async fn fetch_frozen_seats(
    pid: u128,
    body: u32,
    cid: &str,
    role: &str,
    at: [u8; 32],
) -> Result<Vec<FrozenSeat>, String> {
    let client = chain_url::online_client(&chain_url::chain_ws_url()?)
        .await
        .map_err(|e| e.to_string())?;
    let storage = client.storage().at(subxt::utils::H256(at));
    let subject = entitybase::RoleSubject {
        cid_number: cid.as_bytes().to_vec(),
        role_code: role.as_bytes().to_vec(),
    };
    let raw = storage
        .fetch(&dynamic::storage(
            "VotingEngine",
            "RoleVoters",
            vec![
                dynamic::Value::u128(pid),
                dynamic::Value::named_composite([
                    (
                        "cid_number",
                        dynamic::Value::from_bytes(&subject.cid_number),
                    ),
                    ("role_code", dynamic::Value::from_bytes(&subject.role_code)),
                ]),
            ],
        ))
        .await
        .map_err(|e| e.to_string())?
        .ok_or("立法代表岗位冻结人员缺失")?;
    let persons = Vec::<Vec<u8>>::decode_all(&mut raw.encoded()).map_err(|e| e.to_string())?;
    if persons.is_empty() || persons.windows(2).any(|p| p[0] >= p[1]) {
        return Err("冻结人员为空、重复或未排序".into());
    }
    let role_name = storage
        .fetch(&dynamic::storage(
            "PublicEntity",
            "InstitutionRoles",
            vec![
                dynamic::Value::from_bytes(cid.as_bytes()),
                dynamic::Value::from_bytes(role.as_bytes()),
            ],
        ))
        .await
        .map_err(|e| e.to_string())?
        .map(|v| {
            entitybase::InstitutionRole::<Vec<u8>, Vec<u8>, Vec<u8>>::decode_all(&mut v.encoded())
                .map(|r| String::from_utf8_lossy(&r.role_name).into_owned())
                .map_err(|e| e.to_string())
        })
        .transpose()?
        .unwrap_or_else(|| role.to_string());
    let mut seats = Vec::new();
    for person in persons {
        if !primitives::cid::number::parse_cid_number_parts_bytes(&person)
            .is_ok_and(|p| p.institution == *b"CTZN")
        {
            return Err("代表冻结人员不是规范公民CID".into());
        }
        let person = String::from_utf8(person).map_err(|_| "冻结人员CID非UTF-8")?;
        let identity = onchina::chain_identity::read_citizen_identity_at(&person, Some(at)).await?;
        let account_id = identity
            .active_binding()
            .map(|(a, _)| format!("0x{}", hex::encode(a)));
        let name = identity
            .candidate
            .map(|p| {
                format!(
                    "{}{}",
                    String::from_utf8_lossy(&p.family_name),
                    String::from_utf8_lossy(&p.given_name)
                )
            })
            .unwrap_or_default();
        seats.push(FrozenSeat {
            body_index: body,
            person_cid: person,
            role_code: role.into(),
            role_name: role_name.clone(),
            account_id,
            name,
        });
    }
    Ok(seats)
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
#[path = "../tests/display_chain_read.rs"]
mod tests;
