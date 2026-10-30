use super::*;

use crate::call::propose_runtime_upgrade;
use codec::DecodeAll;

#[test]
fn manual_call_encoding_matches_runtime_metadata_contract() {
    let code = vec![1u8, 2, 3];
    let params = difficulty::PowDifficultyParams::genesis_default();
    let reason = "升级参数";
    let proposal_call =
        propose_runtime_upgrade("LN001-NRC0G-944805165-2026", &code, reason, params)
            .expect("call data");
    let decoded = runtime::RuntimeCall::decode_all(&mut proposal_call.as_slice())
        .expect("治理升级 call_data 必须被真实 RuntimeCall 完整解码");
    assert_eq!(decoded.encode(), proposal_call);
}
