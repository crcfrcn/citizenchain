use nodeclient::codec::decode_hex_storage;

use crate::query::cid_number_key;
use crate::query::encode_compact_u32;
use crate::query::fetch_proposal_meta;
use crate::types::JointVoteTally;
use crate::types::ReferendumVoteTally;
use crate::types::UserVoteStatus;
use crate::types::VoteTally;

pub(crate) fn fetch_internal_tally(proposal_id: u64) -> Result<VoteTally, String> {
    let key = nodeclient::query::map_key(
        "InternalVote",
        "InternalTallies",
        &proposal_id.to_le_bytes(),
    );
    // (ADR-017):投票计数按 finalized 口径读取,禁止 best。
    match nodeclient::query::fetch_finalized_storage(&key)? {
        None => Ok(VoteTally { yes: 0, no: 0 }),
        Some(hex_data) => {
            let data = decode_hex_storage(&hex_data)?;
            if data.len() < 8 {
                return Ok(VoteTally { yes: 0, no: 0 });
            }
            let yes = u32::from_le_bytes(
                data[0..4]
                    .try_into()
                    .map_err(|_| "SCALE 数据长度不足".to_string())?,
            );
            let no = u32::from_le_bytes(
                data[4..8]
                    .try_into()
                    .map_err(|_| "SCALE 数据长度不足".to_string())?,
            );
            Ok(VoteTally { yes, no })
        }
    }
}

pub(crate) fn fetch_joint_tally(proposal_id: u64) -> Result<JointVoteTally, String> {
    let key = nodeclient::query::map_key("JointVote", "JointTallies", &proposal_id.to_le_bytes());
    // (ADR-017):投票计数按 finalized 口径读取,禁止 best。
    match nodeclient::query::fetch_finalized_storage(&key)? {
        None => Ok(JointVoteTally { yes: 0, no: 0 }),
        Some(hex_data) => {
            let data = decode_hex_storage(&hex_data)?;
            if data.len() < 8 {
                return Ok(JointVoteTally { yes: 0, no: 0 });
            }
            let yes = u32::from_le_bytes(
                data[0..4]
                    .try_into()
                    .map_err(|_| "SCALE 数据长度不足".to_string())?,
            );
            let no = u32::from_le_bytes(
                data[4..8]
                    .try_into()
                    .map_err(|_| "SCALE 数据长度不足".to_string())?,
            );
            Ok(JointVoteTally { yes, no })
        }
    }
}

pub(crate) fn fetch_referendum_tally(proposal_id: u64) -> Result<ReferendumVoteTally, String> {
    let key =
        nodeclient::query::map_key("JointVote", "ReferendumTallies", &proposal_id.to_le_bytes());
    // (ADR-017):投票计数按 finalized 口径读取,禁止 best。
    match nodeclient::query::fetch_finalized_storage(&key)? {
        None => Ok(ReferendumVoteTally { yes: 0, no: 0 }),
        Some(hex_data) => {
            let data = decode_hex_storage(&hex_data)?;
            if data.len() < 16 {
                return Ok(ReferendumVoteTally { yes: 0, no: 0 });
            }
            let yes = u64::from_le_bytes(
                data[0..8]
                    .try_into()
                    .map_err(|_| "SCALE 数据长度不足".to_string())?,
            );
            let no = u64::from_le_bytes(
                data[8..16]
                    .try_into()
                    .map_err(|_| "SCALE 数据长度不足".to_string())?,
            );
            Ok(ReferendumVoteTally { yes, no })
        }
    }
}

/// 查询指定用户（管理员公钥）对某提案的投票状态。
pub(crate) fn fetch_user_vote_status(
    proposal_id: u64,
    signer_public_key: &str,
    cid_number: Option<&str>,
    voter_role_code: Option<&str>,
) -> Result<UserVoteStatus, String> {
    let meta =
        fetch_proposal_meta(proposal_id)?.ok_or_else(|| format!("提案 {proposal_id} 不存在"))?;

    let signer_public_key_bytes =
        hex::decode(signer_public_key).map_err(|e| format!("公钥解码失败: {e}"))?;

    // 查询内部投票状态（个人账户票或 CID + 岗位码 + 钱包票据）。
    let internal_vote = {
        let ticket_key = match (cid_number, voter_role_code) {
            (Some(cid), Some(role_code)) => {
                let mut key = vec![1u8]; // InternalVoteTicket::Institution
                key.extend_from_slice(&cid_number_key(cid)?);
                key.extend_from_slice(&encode_compact_u32(role_code.len() as u32));
                key.extend_from_slice(role_code.as_bytes());
                key.extend_from_slice(&signer_public_key_bytes);
                key
            }
            (None, None) => {
                let mut key = vec![0u8]; // InternalVoteTicket::Personal
                key.extend_from_slice(&signer_public_key_bytes);
                key
            }
            _ => return Err("机构投票状态查询必须同时提供 cid_number 和 voter_role_code".into()),
        };
        let key = nodeclient::query::double_map_key(
            "InternalVote",
            "InternalVotesByTicket",
            &proposal_id.to_le_bytes(),
            &ticket_key,
        );
        fetch_option_bool(&key)?
    };

    // 查询联合岗位票据状态（CID + 岗位码 + 钱包）。
    let joint_vote = if let (1, Some(cid_number), Some(voter_role_code)) =
        (meta.kind, cid_number, voter_role_code)
    {
        let mut composite_key = cid_number_key(cid_number)?;
        composite_key.extend_from_slice(&encode_compact_u32(voter_role_code.len() as u32));
        composite_key.extend_from_slice(voter_role_code.as_bytes());
        composite_key.extend_from_slice(&signer_public_key_bytes);
        let key = nodeclient::query::double_map_key(
            "JointVote",
            "JointVotesByTicket",
            &proposal_id.to_le_bytes(),
            &composite_key,
        );
        fetch_option_bool(&key)?
    } else {
        None
    };

    Ok(UserVoteStatus {
        proposal_id,
        kind: meta.kind,
        stage: meta.stage,
        voter_role_code: voter_role_code.map(str::to_string),
        internal_vote,
        joint_vote,
    })
}

/// 查询链上 Option<bool> 存储值。
fn fetch_option_bool(storage_key: &str) -> Result<Option<bool>, String> {
    // (ADR-017):投票状态按 finalized 口径读取,禁止 best。
    match nodeclient::query::fetch_finalized_storage(storage_key)? {
        None => Ok(None),
        Some(hex_data) => {
            let data = decode_hex_storage(&hex_data)?;
            if data.is_empty() {
                Ok(None)
            } else {
                Ok(Some(data[0] == 1))
            }
        }
    }
}
