//! 立法投票 `cast_representative_vote` 等裸 SCALE call-data 编码器（pallet 26）。
//!
//! 代表机构表决携带 `voter_role_code`；其余签署调用仍为 `(proposal_id, approve)`。
//! 复用「构造裸 call data → CitizenWallet 一次签名并显示响应二维码
//! → OnChina 回扫后统一提交」通道。
//!
//! 特别案人口快照由投票引擎按 `actor_cid_number` 推导作用域并随提案原子生成，本文件不承载快照调用。
//!
//! `cast_representative_vote` 已接入 handler；`cast_referendum_vote`/`executive_sign`/`override_sign`/
//! `guard_vote` 及其 call index 为公投/行政签署/护宪终审流预留(本轮读展示 + 另线程),暂无生产消费方。

// 公投/行政签署/护宪终审流的 call index 与 encode 函数为预留(本轮读展示 + 另线程),尚无生产消费方。
#![allow(dead_code)]

use crate::core::institution_call::{chain_action_code, ChainCall};
use codec::Encode;

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

/// 编码 `(proposal_id: u64 小端, approve: bool 0x01/0x00)` + `[26, call_index]` 前缀。
fn encode_vote(call_index: u8, proposal_id: u64, approve: bool) -> ChainCall {
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
    proposal_id: u64,
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
pub fn encode_cast_referendum_vote(proposal_id: u64, approve: bool) -> ChainCall {
    encode_vote(CAST_REFERENDUM_VOTE_CALL_INDEX, proposal_id, approve)
}

/// 行政签署/否决(法定代表人:市长/省长/总统)。
pub fn encode_executive_sign(proposal_id: u64, approve: bool) -> ChainCall {
    encode_vote(EXECUTIVE_SIGN_CALL_INDEX, proposal_id, approve)
}

/// 三人会签救济(院长 + 参议长 + 众议长)。
pub fn encode_override_sign(proposal_id: u64, approve: bool) -> ChainCall {
    encode_vote(OVERRIDE_SIGN_CALL_INDEX, proposal_id, approve)
}

/// 护宪大法官终审(修宪)。
pub fn encode_guard_vote(proposal_id: u64, approve: bool) -> ChainCall {
    encode_vote(GUARD_VOTE_CALL_INDEX, proposal_id, approve)
}

#[cfg(test)]
// 投票调用编码夹具为固定输入，断言式解包仅用于测试回归定位。
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;
    use codec::Encode;

    /// 代表机构表决编码 = `[26,1]` + `(u64 小端, bool)`；动作码 0x1A01。
    #[test]
    fn cast_representative_vote_matches_codec_golden() {
        let chain = encode_cast_representative_vote(42, "REPRESENTATIVE", true);
        assert_eq!(&chain.call_data[..2], &[26, 1]);
        assert_eq!(chain.action, 0x1A01);

        let mut golden = Vec::new();
        golden.extend(42u64.encode());
        golden.extend(b"REPRESENTATIVE".to_vec().encode());
        golden.extend(true.encode());
        assert_eq!(
            &chain.call_data[2..],
            &golden[..],
            "cast_representative_vote SCALE 漂移"
        );
    }

    /// 五个表决/签署 call 共用 `(u64, bool)` 形态;approve=false → 末字节 0x00,前缀按各自 call index。
    #[test]
    fn all_vote_calls_share_shape_and_call_index() {
        let cases = [
            (
                encode_cast_referendum_vote(2, false),
                CAST_REFERENDUM_VOTE_CALL_INDEX,
            ),
            (encode_executive_sign(3, false), EXECUTIVE_SIGN_CALL_INDEX),
            (encode_override_sign(4, false), OVERRIDE_SIGN_CALL_INDEX),
            (encode_guard_vote(5, false), GUARD_VOTE_CALL_INDEX),
        ];
        for (chain, call_index) in cases {
            assert_eq!(chain.call_data[0], LEGISLATION_VOTE_PALLET_INDEX);
            assert_eq!(chain.call_data[1], call_index);
            assert_eq!(chain.call_data.len(), 2 + 8 + 1); // 前缀 + u64 + bool
            assert_eq!(*chain.call_data.last().unwrap(), 0x00); // approve=false
        }
        let representative = encode_cast_representative_vote(1, "REPRESENTATIVE", false);
        assert_eq!(representative.call_data[0], LEGISLATION_VOTE_PALLET_INDEX);
        assert_eq!(
            representative.call_data[1],
            CAST_REPRESENTATIVE_VOTE_CALL_INDEX
        );
        assert_eq!(*representative.call_data.last().unwrap(), 0x00);
    }
}
