//! 从同一最终块读取原机构票据；未投、反对及读取失败必须分别处理。
use crate::types::{JointVoteTally, ReferendumVoteTally, UserVoteStatus, VoteTally};
use codec::Encode;
use nodeclient::codec::decode_hex_storage;

fn count_at(pid: u128, pallet: &str, item: &str, at: &str) -> Result<(u32, u32), String> {
    let key = nodeclient::query::map_key(pallet, item, &pid.to_le_bytes());
    let Some(raw) = nodeclient::query::fetch_storage_at(&key, at)? else {
        return Ok((0, 0));
    };
    let bytes = decode_hex_storage(&raw)?;
    if bytes.len() != 8 {
        return Err(format!("{item}计票数据损坏"));
    }
    Ok((
        u32::from_le_bytes(bytes[..4].try_into().unwrap()),
        u32::from_le_bytes(bytes[4..].try_into().unwrap()),
    ))
}
pub(crate) fn fetch_internal_tally_at(pid: u128, at: &str) -> Result<VoteTally, String> {
    let (yes, no) = count_at(pid, "InternalVote", "InternalTallies", at)?;
    Ok(VoteTally { yes, no })
}
pub(crate) fn fetch_joint_tally_at(pid: u128, at: &str) -> Result<JointVoteTally, String> {
    let (yes, no) = count_at(pid, "JointVote", "JointTallies", at)?;
    Ok(JointVoteTally { yes, no })
}
pub(crate) fn fetch_referendum_tally_at(
    pid: u128,
    at: &str,
) -> Result<ReferendumVoteTally, String> {
    let key = nodeclient::query::map_key("JointVote", "ReferendumTallies", &pid.to_le_bytes());
    let Some(raw) = nodeclient::query::fetch_storage_at(&key, at)? else {
        return Ok(ReferendumVoteTally { yes: 0, no: 0 });
    };
    let bytes = decode_hex_storage(&raw)?;
    if bytes.len() != 16 {
        return Err("公投计票数据损坏".into());
    }
    Ok(ReferendumVoteTally {
        yes: u64::from_le_bytes(bytes[..8].try_into().unwrap()),
        no: u64::from_le_bytes(bytes[8..].try_into().unwrap()),
    })
}
pub(crate) fn decode_option_vote(raw: Option<&str>) -> Result<Option<bool>, String> {
    match raw {
        None => Ok(None),
        Some(raw) => match decode_hex_storage(raw)?.as_slice() {
            [0] => Ok(Some(false)),
            [1] => Ok(Some(true)),
            _ => Err("原投票记录损坏，不能当作未投票".into()),
        },
    }
}
pub(crate) fn institution_vote_key(
    pid: u128,
    kind: u8,
    cid: &str,
    role: &str,
    person_cid: &[u8],
) -> Result<String, String> {
    let role_subject = engingcore::types::RoleSubject {
        cid_number: cid
            .as_bytes()
            .to_vec()
            .try_into()
            .map_err(|_| "机构身份CID号超限")?,
        role_code: role
            .as_bytes()
            .to_vec()
            .try_into()
            .map_err(|_| "岗位标识超限")?,
    };
    let ticket = engingcore::types::InstitutionVoteTicket {
        role_subject,
        person_cid: person_cid.to_vec().try_into().map_err(|_| "人员CID超限")?,
    };
    let (mut key, pallet, item) = match kind {
        0 => (vec![2], "InternalVote", "InternalVotesByTicket"),
        1 => (vec![], "JointVote", "JointVotesByTicket"),
        _ => return Err("节点机构治理投票类型不支持".into()),
    };
    key.extend(ticket.encode());
    Ok(nodeclient::query::double_map_key(
        pallet,
        item,
        &pid.to_le_bytes(),
        &key,
    ))
}

/// 输入是32字节账户ID；PQ公钥不属于票据键。机构只查询原快照，个人分支保持原键。
pub(crate) fn fetch_user_vote_status(
    pid: u128,
    wallet: &str,
    cid: Option<&str>,
    role: Option<&str>,
) -> Result<UserVoteStatus, String> {
    let wallet = nodedesktop::validation::normalize_account_id(wallet)?;
    let at = nodeclient::query::fetch_finalized_head()?;
    let meta = crate::query::fetch_proposal_meta_at(pid, &at)?.ok_or("提案不存在")?;
    let vote = match (cid, role) {
        (Some(cid), Some(role)) => {
            let view = crate::query::load_frozen_vote_view_at(pid, &at)?;
            let entry = view
                .roles
                .iter()
                .find(|r| r.cid_number == cid && r.role_code == role)
                .and_then(|r| {
                    r.voters
                        .iter()
                        .find(|v| v.account_id.as_deref() == Some(wallet.as_str()))
                })
                .ok_or("此钱包不在原岗位快照")?;
            if let Some(e) = &entry.vote_error {
                return Err(e.clone());
            }
            entry.vote
        }
        (None, None) => {
            let mut ticket = vec![0];
            ticket.extend(decode_hex_storage(&wallet)?);
            let key = nodeclient::query::double_map_key(
                "InternalVote",
                "InternalVotesByTicket",
                &pid.to_le_bytes(),
                &ticket,
            );
            decode_option_vote(nodeclient::query::fetch_storage_at(&key, &at)?.as_deref())?
        }
        _ => return Err("必须同时指定完整机构身份CID号和原岗位".into()),
    };
    Ok(UserVoteStatus {
        proposal_id: pid,
        kind: meta.kind,
        stage: meta.stage,
        voter_role_code: role.map(str::to_string),
        internal_vote: if meta.kind == 0 { vote } else { None },
        joint_vote: if meta.kind == 1 { vote } else { None },
    })
}
