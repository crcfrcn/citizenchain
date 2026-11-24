//! 校验升级原因、PoW参数及WASM输入，构造运行时升级提案。

use codec::Encode;
use nodeclient::signing::encode_compact_u32;

const RUNTIME_UPGRADE_PALLET_INDEX: u8 = 12;

const PROPOSE_RUNTIME_UPGRADE_CALL_INDEX: u8 = 0;

const MAX_WASM_BYTES: usize = 5 * 1_048_576;
const MAX_REASON_BYTES: usize = 1024;

const COMMITTEE_ROLE_CODE: &[u8] = b"COMMITTEE_MEMBER";

/// 与页面构建、提交使用相同的规范化；字节上限保持当前链上规则。
pub(crate) fn normalize_reason(reason: &str) -> Result<&str, String> {
    let reason = reason.trim();
    if reason.is_empty() {
        return Err("升级理由不能为空".to_string());
    }
    if reason.len() > MAX_REASON_BYTES {
        return Err(format!(
            "升级理由不能超过 {MAX_REASON_BYTES} 字节，当前 {} 字节",
            reason.len()
        ));
    }
    Ok(reason)
}

/// 提前提示 difficulty::stage_params 的既有拒绝条件；链上校验仍是最终依据。
pub(crate) fn validate_pow_params(
    proposed: difficulty::PowDifficultyParams,
    active: difficulty::PowDifficultyParams,
    has_pending: bool,
) -> Result<(), String> {
    proposed.validate().map_err(str::to_string)?;
    if proposed.algorithm_version != primitives::pow_const::POW_ALGORITHM_VERSION {
        return Err("当前 Runtime 不支持该 PoW 算法版本".to_string());
    }
    if has_pending {
        return Err("已有待生效 PoW 参数，请生效后重新读取参数并生成签名".to_string());
    }
    let mut values = proposed;
    values.params_version = active.params_version;
    if values == active {
        if proposed.params_version != active.params_version {
            return Err("PoW 参数未变时，参数版本必须保持不变".to_string());
        }
    } else if proposed.params_version != active.params_version.saturating_add(1) {
        return Err("修改 PoW 参数时，参数版本必须在链上当前版本基础上精确加一".to_string());
    }
    Ok(())
}

/// 读取并校验 Runtime WASM 文件。
pub(crate) fn read_wasm(wasm_path: &str) -> Result<(Vec<u8>, f64), String> {
    let wasm_code =
        std::fs::read(wasm_path.trim()).map_err(|e| format!("读取 WASM 文件失败: {e}"))?;
    if wasm_code.is_empty() {
        return Err("WASM 文件为空".to_string());
    }
    let wasm_size_mb = wasm_code.len() as f64 / 1_048_576.0;
    if wasm_code.len() > MAX_WASM_BYTES {
        return Err(format!("WASM 文件超过 5MB 上限，当前 {wasm_size_mb:.2} MB"));
    }
    Ok((wasm_code, wasm_size_mb))
}

/// 构建运行期协议升级提案 call_data: Upgrade.propose_runtime_upgrade(...)。
pub(crate) fn propose_runtime_upgrade(
    actor_cid_number: &str,
    wasm_code: &[u8],
    reason: &str,
    pow_params: difficulty::PowDifficultyParams,
) -> Result<Vec<u8>, String> {
    if actor_cid_number.is_empty()
        || actor_cid_number.len() > primitives::core_const::CID_NUMBER_MAX_BYTES as usize
    {
        return Err("actor_cid_number 超出链上协议范围".to_string());
    }
    let reason_bytes = normalize_reason(reason)?.as_bytes();

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
    pow_params: difficulty::PowDifficultyParams,
) -> Result<Vec<u8>, String> {
    let (wasm_code, _) = read_wasm(wasm_path)?;
    propose_runtime_upgrade(actor_cid_number, &wasm_code, reason, pow_params)
}
#[cfg(test)]
#[path = "../tests/integration.rs"]
mod tests;

#[cfg(test)]
#[path = "../tests/unit.rs"]
mod unit_tests;
