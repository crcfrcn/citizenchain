//! 编码代表投票、公投及行政签署的SCALE调用。

#![allow(dead_code)]

use codec::Encode;
use onchina::institutions::{chain_action_code, ChainCall};

/// LegislationVote pallet 在 construct_runtime 的索引。
pub const LEGISLATION_VOTE_PALLET_INDEX: u8 = 26;
/// `cast_representative_vote` call index（代表机构表决）。
pub const CAST_REPRESENTATIVE_VOTE_CALL_INDEX: u8 = 1;
/// `cast_referendum_vote` call index(特别案公投)。
pub const CAST_REFERENDUM_VOTE_CALL_INDEX: u8 = 2;
/// `executive_sign` call index(行政签署/否决)。
pub const EXECUTIVE_SIGN_CALL_INDEX: u8 = 3;
/// `override_sign` call index(三人会签救济)。
pub const OVERRIDE_SIGN_CALL_INDEX: u8 = 4;
/// `guard_vote` call index(护宪大法官终审)。
pub const GUARD_VOTE_CALL_INDEX: u8 = 5;

/// 编码 `(proposal_id: u128 小端, approve: bool 0x01/0x00)` + `[26, call_index]` 前缀。
fn encode_vote(call_index: u8, proposal_id: u128, approve: bool) -> ChainCall {
    let mut out = vec![LEGISLATION_VOTE_PALLET_INDEX, call_index];
    out.extend(proposal_id.to_le_bytes());
    out.push(if approve { 0x01 } else { 0x00 });
    ChainCall {
        action: chain_action_code(LEGISLATION_VOTE_PALLET_INDEX, call_index),
        call_data: out,
    }
}

/// 当前代表机构的管理员按其机构席位投票。
pub fn encode_cast_representative_vote(
    proposal_id: u128,
    voter_role_code: &str,
    approve: bool,
) -> ChainCall {
    let role_code = voter_role_code.trim().as_bytes();
    assert!(!role_code.is_empty() && role_code.len() <= 64);
    let mut out = vec![
        LEGISLATION_VOTE_PALLET_INDEX,
        CAST_REPRESENTATIVE_VOTE_CALL_INDEX,
    ];
    out.extend(proposal_id.to_le_bytes());
    out.extend(role_code.to_vec().encode());
    out.push(if approve { 0x01 } else { 0x00 });
    ChainCall {
        action: chain_action_code(
            LEGISLATION_VOTE_PALLET_INDEX,
            CAST_REPRESENTATIVE_VOTE_CALL_INDEX,
        ),
        call_data: out,
    }
}

/// 特别案立法公投。
pub fn encode_cast_referendum_vote(proposal_id: u128, approve: bool) -> ChainCall {
    encode_vote(CAST_REFERENDUM_VOTE_CALL_INDEX, proposal_id, approve)
}

/// 行政签署/否决(法定代表人:市长/省长/总统)。
pub fn encode_executive_sign(proposal_id: u128, approve: bool) -> ChainCall {
    encode_vote(EXECUTIVE_SIGN_CALL_INDEX, proposal_id, approve)
}

/// 三人会签救济(院长 + 参议长 + 众议长)。
pub fn encode_override_sign(proposal_id: u128, approve: bool) -> ChainCall {
    encode_vote(OVERRIDE_SIGN_CALL_INDEX, proposal_id, approve)
}

/// 护宪大法官终审(修宪)。
pub fn encode_guard_vote(proposal_id: u128, approve: bool) -> ChainCall {
    encode_vote(GUARD_VOTE_CALL_INDEX, proposal_id, approve)
}

#[cfg(test)]
// 投票调用编码夹具为固定输入，断言式解包仅用于测试回归定位。
#[allow(clippy::expect_used, clippy::unwrap_used)]
#[path = "../tests/voting.rs"]
mod tests;
