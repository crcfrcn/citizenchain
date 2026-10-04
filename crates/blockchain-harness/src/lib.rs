//! CitizenChain 区块链测试 harness。
//!
//! 本 crate 只服务真实验收、导入路径验证和后续恶意候选块构造，不能被生产节点、
//! runtime 或业务模块依赖。放在 `citizenchain/crates/` 下，是为了把测试专用
//! 能力沉淀为可复用工具，同时避免把坏块构造逻辑混入生产路径。

use sp_core::{sr25519, Pair, H256};

/// 完整导入态必须拒绝的 NodeGuard 永久规则坏样本。
///
/// harness 只定义“应当覆盖哪些坏样本”和“应由哪个守卫前缀拒绝”，具体 runtime
/// storage key 的构造仍留在 node 内部测试里，避免测试工具反向依赖生产节点私有实现。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ImportedStateBadCaseKind {
    /// 固定治理骨架缺失固定机构管理员账户。
    MissingGovernanceAdmin,
    /// 创世完整态里全节点发行计数不是零。
    NonZeroFullnodeIssued,
    /// 公民认证发行 pallet 下出现未知 storage key。
    UnknownCitizenIssuanceKey,
    /// 创世模块固定人口上限被改写。
    ChangedGenesisCitizenMax,
    /// 省储行创立发行质押本金账户缺失。
    MissingProvincialBankStake,
    /// 省储行固定发行 pallet 下出现未知 storage key。
    UnknownProvincialBankStorage,
}

/// NodeGuard 永久规则坏样本矩阵。
pub const IMPORTED_STATE_BAD_CASES: [ImportedStateBadCaseKind; 6] = [
    ImportedStateBadCaseKind::MissingGovernanceAdmin,
    ImportedStateBadCaseKind::NonZeroFullnodeIssued,
    ImportedStateBadCaseKind::UnknownCitizenIssuanceKey,
    ImportedStateBadCaseKind::ChangedGenesisCitizenMax,
    ImportedStateBadCaseKind::MissingProvincialBankStake,
    ImportedStateBadCaseKind::UnknownProvincialBankStorage,
];

impl ImportedStateBadCaseKind {
    /// 返回全部永久规则坏样本。
    pub fn all() -> &'static [Self] {
        &IMPORTED_STATE_BAD_CASES
    }

    /// 供日志、CLI 和文档复用的稳定标签。
    pub fn label(self) -> &'static str {
        match self {
            Self::MissingGovernanceAdmin => "missing_governance_admin",
            Self::NonZeroFullnodeIssued => "non_zero_fullnode_issued",
            Self::UnknownCitizenIssuanceKey => "unknown_citizen_issuance_key",
            Self::ChangedGenesisCitizenMax => "changed_genesis_citizen_max",
            Self::MissingProvincialBankStake => "missing_provincial_bank_stake",
            Self::UnknownProvincialBankStorage => "unknown_provincial_bank_storage",
        }
    }

    /// NodeGuard 拒绝该坏样本时必须命中的守卫类别前缀。
    pub fn expected_error_prefix(self) -> &'static str {
        match self {
            Self::MissingGovernanceAdmin => "固定治理骨架:",
            Self::NonZeroFullnodeIssued => "全节点发行:",
            Self::UnknownCitizenIssuanceKey => "公民认证发行:",
            Self::ChangedGenesisCitizenMax => "创世模块:",
            Self::MissingProvincialBankStake | Self::UnknownProvincialBankStorage => {
                "省储行固定发行:"
            }
        }
    }
}

/// `export-blocks` JSON 行格式的轻量摘要。
///
/// 这里只解析验收所需字段，不把该结构当成权威区块类型；真正导入仍交给
/// 节点 CLI `import-blocks` / import queue 执行。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportedBlockSummary {
    pub number_hex: String,
    pub parent_hash: String,
    pub state_root: String,
    pub extrinsics_root: String,
    pub extrinsics_len: usize,
    pub digest_logs_len: usize,
}

/// 构造 `System::remark` call data。
///
/// 当前 runtime 中 `RuntimeCall::System` 的 pallet index 为 0，`frame_system::remark`
/// 的 call index 为 0；该事实用于验收交易，不承载业务含义。
pub fn system_remark_call_data(remark: &[u8]) -> Vec<u8> {
    let mut call_data = Vec::with_capacity(2 + 5 + remark.len());
    call_data.push(0u8);
    call_data.push(0u8);
    call_data.extend_from_slice(&compact_u32(remark.len() as u32));
    call_data.extend_from_slice(remark);
    call_data
}

/// 使用标准开发测试账户 `//Alice` 构造 signed `System::remark` extrinsic。
///
/// 该函数用于本地验收链触发真实非空出块，避免每次验收都在 `/tmp` 重写一次性
/// 签名器。生产代码不得调用本函数。
pub fn alice_system_remark_extrinsic_hex(
    genesis_hash_hex: &str,
    nonce: u32,
    spec_version: u32,
    tx_version: u32,
    remark: &[u8],
) -> Result<String, String> {
    let genesis_hash = parse_h256(genesis_hash_hex)?;
    let call_data = system_remark_call_data(remark);
    let call = chain_signing::decode_runtime_call(&call_data)?;
    let pair = sr25519::Pair::from_string("//Alice", None)
        .map_err(|e| format!("构造 Alice 测试密钥失败: {e}"))?;
    let extrinsic = chain_signing::build_signed_extrinsic_with_pair(
        call,
        genesis_hash,
        nonce,
        spec_version,
        tx_version,
        &pair,
    );
    Ok(chain_signing::signed_extrinsic_hex(&extrinsic))
}

/// 解析 0x 前缀可选的 32 字节哈希。
pub fn parse_h256(value: &str) -> Result<H256, String> {
    let clean = value.strip_prefix("0x").unwrap_or(value);
    let raw = hex::decode(clean).map_err(|e| format!("哈希 hex 解码失败: {e}"))?;
    let bytes: [u8; 32] = raw
        .as_slice()
        .try_into()
        .map_err(|_| format!("哈希长度无效：期望 32 字节，实际 {}", raw.len()))?;
    Ok(H256::from(bytes))
}

/// 读取 `export-blocks` 默认 JSON 输出，返回每个区块的摘要。
///
/// Substrate `export-blocks` 默认输出 JSON lines，每行一个
/// `{ block, justifications }` 记录。本函数也容忍外层 JSON array，便于后续测试
/// 工具组合。解析失败必须显式报错，避免验收脚本把空文件或坏文件当成通过。
pub fn summarize_exported_blocks_json(input: &str) -> Result<Vec<ExportedBlockSummary>, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("导出块 JSON 为空".to_string());
    }

    if trimmed.starts_with('[') {
        let values: Vec<serde_json::Value> = serde_json::from_str(trimmed)
            .map_err(|e| format!("导出块 JSON array 解析失败: {e}"))?;
        values
            .iter()
            .enumerate()
            .map(|(idx, value)| summarize_exported_block_record(value, idx + 1))
            .collect()
    } else {
        trimmed
            .lines()
            .enumerate()
            .filter(|(_, line)| !line.trim().is_empty())
            .map(|(idx, line)| {
                let value: serde_json::Value = serde_json::from_str(line)
                    .map_err(|e| format!("第 {} 行导出块 JSON 解析失败: {e}", idx + 1))?;
                summarize_exported_block_record(&value, idx + 1)
            })
            .collect()
    }
}

/// 生成一个基础无效块文件：只篡改第一条记录的 header.stateRoot。
///
/// 该函数用于证明 `import-blocks` / import queue 能拒绝基础 root 不一致的坏文件；
/// 它不是 NodeGuard 永久规则坏块构造器，因为没有重算合法 state root，也没有生成
/// 结构完整但违反制度规则的状态转换。
pub fn tamper_first_state_root_json(
    input: &str,
    replacement_state_root: &str,
) -> Result<String, String> {
    let _ = parse_h256(replacement_state_root)?;
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("导出块 JSON 为空，无法篡改 stateRoot".to_string());
    }
    if trimmed.starts_with('[') {
        let mut values: Vec<serde_json::Value> = serde_json::from_str(trimmed)
            .map_err(|e| format!("导出块 JSON array 解析失败: {e}"))?;
        let first = values
            .first_mut()
            .ok_or_else(|| "导出块 JSON array 为空".to_string())?;
        set_state_root(first, replacement_state_root, 1)?;
        serde_json::to_string(&values).map_err(|e| format!("序列化篡改 JSON 失败: {e}"))
    } else {
        let mut out = Vec::new();
        let mut changed = false;
        for (idx, line) in trimmed.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            let mut value: serde_json::Value = serde_json::from_str(line)
                .map_err(|e| format!("第 {} 行导出块 JSON 解析失败: {e}", idx + 1))?;
            if !changed {
                set_state_root(&mut value, replacement_state_root, idx + 1)?;
                changed = true;
            }
            out.push(
                serde_json::to_string(&value)
                    .map_err(|e| format!("序列化第 {} 行篡改 JSON 失败: {e}", idx + 1))?,
            );
        }
        if !changed {
            return Err("没有可篡改的导出块记录".to_string());
        }
        Ok(format!("{}\n", out.join("\n")))
    }
}

fn compact_u32(value: u32) -> Vec<u8> {
    if value < 0x40 {
        vec![(value as u8) << 2]
    } else if value < 0x4000 {
        let encoded = ((value as u16) << 2) | 0x01;
        encoded.to_le_bytes().to_vec()
    } else if value < 0x4000_0000 {
        let encoded = (value << 2) | 0x02;
        encoded.to_le_bytes().to_vec()
    } else {
        let mut out = vec![0x03u8];
        out.extend_from_slice(&value.to_le_bytes());
        out
    }
}

fn summarize_exported_block_record(
    value: &serde_json::Value,
    record_no: usize,
) -> Result<ExportedBlockSummary, String> {
    let header = value
        .get("block")
        .and_then(|v| v.get("header"))
        .ok_or_else(|| format!("第 {record_no} 条记录缺少 block.header"))?;
    let extrinsics = value
        .get("block")
        .and_then(|v| v.get("extrinsics"))
        .and_then(|v| v.as_array())
        .ok_or_else(|| format!("第 {record_no} 条记录缺少 block.extrinsics 数组"))?;
    let digest_logs = header
        .get("digest")
        .and_then(|v| v.get("logs"))
        .and_then(|v| v.as_array())
        .ok_or_else(|| format!("第 {record_no} 条记录缺少 header.digest.logs 数组"))?;

    Ok(ExportedBlockSummary {
        number_hex: string_field(header, "number", record_no)?,
        parent_hash: string_field(header, "parentHash", record_no)?,
        state_root: string_field(header, "stateRoot", record_no)?,
        extrinsics_root: string_field(header, "extrinsicsRoot", record_no)?,
        extrinsics_len: extrinsics.len(),
        digest_logs_len: digest_logs.len(),
    })
}

fn string_field(
    value: &serde_json::Value,
    field: &str,
    record_no: usize,
) -> Result<String, String> {
    value
        .get(field)
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .ok_or_else(|| format!("第 {record_no} 条记录缺少字符串字段 {field}"))
}

fn set_state_root(
    value: &mut serde_json::Value,
    replacement_state_root: &str,
    record_no: usize,
) -> Result<(), String> {
    let header = value
        .get_mut("block")
        .and_then(|v| v.get_mut("header"))
        .ok_or_else(|| format!("第 {record_no} 条记录缺少 block.header"))?;
    let state_root = header
        .get_mut("stateRoot")
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("第 {record_no} 条记录缺少字符串字段 stateRoot"))?;
    if state_root.eq_ignore_ascii_case(replacement_state_root) {
        return Err("替换 stateRoot 与原值相同，无法形成篡改样本".to_string());
    }
    header["stateRoot"] = serde_json::Value::String(replacement_state_root.to_string());
    Ok(())
}

#[cfg(test)]
// 测试夹具构建失败必须立即中止，断言式解包仅限本测试模块。
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    const BLOCK0_JSON_LINE: &str = r#"{"block":{"header":{"parentHash":"0x0000000000000000000000000000000000000000000000000000000000000000","number":"0x0","stateRoot":"0x49fa5fee51926be6b92e79478e365f354a39c96b55f7378ee05b34d0633c9a53","extrinsicsRoot":"0x03170a2e7597b7b7e3d84c05391d139a62b157e78786d8c082f29dcf4c111314","digest":{"logs":[]}},"extrinsics":[]},"justifications":null}"#;

    #[test]
    fn system_remark_call_data_uses_runtime_system_remark_indices() {
        let call_data = system_remark_call_data(b"ok");

        assert_eq!(call_data, vec![0, 0, 8, b'o', b'k']);
        chain_signing::decode_runtime_call(&call_data)
            .expect("System::remark call data must decode with current runtime");
    }

    #[test]
    fn parse_h256_rejects_wrong_length() {
        let err = parse_h256("0x1234").expect_err("short hash must be rejected");

        assert!(err.contains("哈希长度无效"));
    }

    #[test]
    fn imported_state_bad_case_matrix_is_stable() {
        let labels = ImportedStateBadCaseKind::all()
            .iter()
            .map(|case| case.label())
            .collect::<std::collections::BTreeSet<_>>();

        assert_eq!(labels.len(), ImportedStateBadCaseKind::all().len());
        assert!(labels.contains("missing_provincial_bank_stake"));
        for case in ImportedStateBadCaseKind::all() {
            assert!(
                case.expected_error_prefix().ends_with(':'),
                "{} must expose a NodeGuard category prefix",
                case.label()
            );
        }
    }

    #[test]
    fn summarize_exported_blocks_json_reads_json_lines() {
        let summary = summarize_exported_blocks_json(BLOCK0_JSON_LINE)
            .expect("exported block line should summarize");

        assert_eq!(summary.len(), 1);
        assert_eq!(summary[0].number_hex, "0x0");
        assert_eq!(summary[0].extrinsics_len, 0);
        assert_eq!(summary[0].digest_logs_len, 0);
    }

    #[test]
    fn tamper_first_state_root_json_rewrites_only_state_root() {
        let replacement = "0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff0";
        let tampered = tamper_first_state_root_json(BLOCK0_JSON_LINE, replacement)
            .expect("state root should be tampered");
        let summary = summarize_exported_blocks_json(&tampered)
            .expect("tampered block should still be JSON-readable");

        assert_eq!(summary[0].state_root, replacement);
        assert_eq!(summary[0].number_hex, "0x0");
        assert_eq!(
            summary[0].extrinsics_root,
            "0x03170a2e7597b7b7e3d84c05391d139a62b157e78786d8c082f29dcf4c111314"
        );
    }

    #[test]
    fn tamper_first_state_root_json_rejects_same_root() {
        let err = tamper_first_state_root_json(
            BLOCK0_JSON_LINE,
            "0x49fa5fee51926be6b92e79478e365f354a39c96b55f7378ee05b34d0633c9a53",
        )
        .expect_err("same root must not be accepted as tampering");

        assert!(err.contains("相同"));
    }
}
