//! 验证升级调用与Runtime metadata一致，并保持准备提交的原因字节。

use super::*;

use crate::call::propose_runtime_upgrade;
use codec::DecodeAll;

#[test]
fn manual_call_encoding_matches_runtime_metadata_contract() {
    let code = vec![1u8, 2, 3];
    let params = difficulty::PowDifficultyParams {
        params_version: 1,
        algorithm_version: 1,
        target_block_time_ms: 360_000,
        adjustment_interval: 600,
        max_adjust_up_factor: 4,
        max_adjust_down_divisor: 4,
    };
    let reason = "升级参数";
    let proposal_call =
        propose_runtime_upgrade("LN001-NRC0G-944805165-2026", &code, reason, params)
            .expect("call data");
    // 冻结正常输入的原调用字节，规范化与前置校验不得改变签名载荷结构。
    assert_eq!(hex::encode(&proposal_call), "0c00684c4e3030312d4e524330472d3934343830353136352d3230323640434f4d4d49545445455f4d454d42455230e58d87e7baa7e58f82e695b00c010203010000000100407e0500000000005802000004000000000000000400000000000000");
    let decoded = runtime::RuntimeCall::decode_all(&mut proposal_call.as_slice())
        .expect("治理升级 call_data 必须被真实 RuntimeCall 完整解码");
    assert_eq!(decoded.encode(), proposal_call);
}

#[test]
fn canonical_reason_keeps_build_and_submit_call_bytes_identical() {
    let cid = "LN001-NRC0G-944805165-2026";
    let params = difficulty::PowDifficultyParams::genesis_default();
    let raw = " \n 升级参数 \t";
    let build = propose_runtime_upgrade(cid, b"candidate", raw.trim(), params).unwrap();
    let submit = propose_runtime_upgrade(cid, b"candidate", raw, params).unwrap();
    assert_eq!(build, submit);
    let decoded = runtime::RuntimeCall::decode_all(&mut submit.as_slice()).unwrap();
    assert_eq!(decoded.encode(), submit);
}

#[test]
fn reason_boundary_matches_the_actual_runtime_call() {
    let cid = "LN001-NRC0G-944805165-2026";
    let params = difficulty::PowDifficultyParams::genesis_default();
    let maximum = "中".repeat(341) + "a";
    let call = propose_runtime_upgrade(cid, b"candidate", &maximum, params).unwrap();
    let decoded = runtime::RuntimeCall::decode_all(&mut call.as_slice()).unwrap();
    assert_eq!(decoded.encode(), call);
    assert!(propose_runtime_upgrade(cid, b"candidate", &"中".repeat(342), params).is_err());
}
