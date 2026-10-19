//! 随所属功能迁移的原有回归，输入与断言保持不变。
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
