use crate::governance::signing::encode_compact_u32;
use codec::Encode;

const RUNTIME_UPGRADE_PALLET_INDEX: u8 = 12;
const PROPOSE_RUNTIME_UPGRADE_CALL_INDEX: u8 = 0;
const MAX_WASM_BYTES: usize = 5 * 1_048_576;
const COMMITTEE_ROLE_CODE: &[u8] = b"COMMITTEE_MEMBER";

/// 读取并校验 Runtime WASM 文件。
pub(crate) fn read_wasm(wasm_path: &str) -> Result<(Vec<u8>, f64), String> {
    let wasm_code = std::fs::read(wasm_path).map_err(|e| format!("读取 WASM 文件失败: {e}"))?;
    if wasm_code.is_empty() {
        return Err("WASM 文件为空".to_string());
    }
    let wasm_size_mb = wasm_code.len() as f64 / 1_048_576.0;
    if wasm_code.len() > MAX_WASM_BYTES {
        return Err(format!("WASM 文件超过 5MB 上限，当前 {wasm_size_mb:.2} MB"));
    }
    Ok((wasm_code, wasm_size_mb))
}

/// 构建运行期协议升级提案 call_data: RuntimeUpgrade.propose_runtime_upgrade(...)。
pub(crate) fn propose_runtime_upgrade(
    actor_cid_number: &str,
    wasm_code: &[u8],
    reason: &str,
    pow_params: pow_difficulty::PowDifficultyParams,
) -> Result<Vec<u8>, String> {
    if actor_cid_number.is_empty()
        || actor_cid_number.len() > primitives::core_const::CID_NUMBER_MAX_BYTES as usize
    {
        return Err("actor_cid_number 超出链上协议范围".to_string());
    }
    let reason_bytes = reason.as_bytes();
    if reason_bytes.is_empty() {
        return Err("升级理由不能为空".to_string());
    }

    let reason_compact = encode_compact_u32(reason_bytes.len() as u32);
    let wasm_compact = encode_compact_u32(wasm_code.len() as u32);

    let mut call_data = Vec::with_capacity(
        2 + reason_compact.len() + reason_bytes.len() + wasm_compact.len() + wasm_code.len(),
    );
    call_data.push(RUNTIME_UPGRADE_PALLET_INDEX);
    call_data.push(PROPOSE_RUNTIME_UPGRADE_CALL_INDEX);
    call_data.extend_from_slice(&encode_compact_u32(actor_cid_number.len() as u32));
    call_data.extend_from_slice(actor_cid_number.as_bytes());
    call_data.extend_from_slice(&encode_compact_u32(COMMITTEE_ROLE_CODE.len() as u32));
    call_data.extend_from_slice(COMMITTEE_ROLE_CODE);
    call_data.extend_from_slice(&reason_compact);
    call_data.extend_from_slice(reason_bytes);
    call_data.extend_from_slice(&wasm_compact);
    call_data.extend_from_slice(wasm_code);
    call_data.extend_from_slice(&pow_params.encode());
    Ok(call_data)
}

/// 从文件重建运行期协议升级提案 call_data。
pub(crate) fn propose_runtime_upgrade_from_file(
    actor_cid_number: &str,
    wasm_path: &str,
    reason: &str,
    pow_params: pow_difficulty::PowDifficultyParams,
) -> Result<Vec<u8>, String> {
    let (wasm_code, _) = read_wasm(wasm_path)?;
    propose_runtime_upgrade(actor_cid_number, &wasm_code, reason, pow_params)
}

#[cfg(test)]
mod tests {
    use super::*;
    use codec::DecodeAll;

    #[test]
    fn manual_call_encoding_matches_runtime_metadata_contract() {
        let code = vec![1u8, 2, 3];
        let params = pow_difficulty::PowDifficultyParams::genesis_default();
        let reason = "升级参数";
        let proposal_call =
            propose_runtime_upgrade("LN001-NRC0G-944805165-2026", &code, reason, params)
                .expect("call data");
        let decoded = citizenchain::RuntimeCall::decode_all(&mut proposal_call.as_slice())
            .expect("治理升级 call_data 必须被真实 RuntimeCall 完整解码");
        assert_eq!(decoded.encode(), proposal_call);
    }
}
