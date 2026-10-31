//! 通过真实共享包接口核验导出块，金标只归本包tests。
#![allow(clippy::expect_used, clippy::unwrap_used)]
use blockchain_harness::*;
const BLOCK0_JSON_LINE: &str = include_str!("block.json");

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
fn json_array_and_lines_preserve_record_order_and_counts() {
    let first: serde_json::Value = serde_json::from_str(BLOCK0_JSON_LINE).unwrap();
    let mut second = first.clone();
    second["block"]["header"]["number"] = "0x1".into();
    second["block"]["extrinsics"] = serde_json::json!(["0x01", "0x02"]);
    second["block"]["header"]["digest"]["logs"] = serde_json::json!(["0xaa"]);
    let array = serde_json::to_string(&vec![first.clone(), second.clone()]).unwrap();
    let lines = format!("\n{}\n\n{}\n", first, second);
    let actual = summarize_exported_blocks_json(&array).unwrap();
    assert_eq!(actual, summarize_exported_blocks_json(&lines).unwrap());
    assert_eq!(actual[1].number_hex, "0x1");
    assert_eq!(actual[1].extrinsics_len, 2);
    assert_eq!(actual[1].digest_logs_len, 1);
    for input in ["", " ", "not-json", "{}", "[{}]"] {
        assert!(summarize_exported_blocks_json(input).is_err());
    }
}

#[test]
fn malformed_export_fields_report_the_affected_record() {
    let base: serde_json::Value = serde_json::from_str(BLOCK0_JSON_LINE).unwrap();
    for pointer in [
        "/block/header",
        "/block/extrinsics",
        "/block/header/digest/logs",
        "/block/header/number",
        "/block/header/parentHash",
        "/block/header/stateRoot",
        "/block/header/extrinsicsRoot",
    ] {
        let mut bad = base.clone();
        *bad.pointer_mut(pointer).unwrap() = serde_json::Value::Null;
        let error = summarize_exported_blocks_json(&format!("{}\n{}", base, bad)).unwrap_err();
        assert!(error.contains('2'), "{pointer}: {error}");
    }
}

#[test]
fn remark_compact_length_boundaries_decode_as_current_runtime_calls() {
    for (length, prefix) in [
        (0, vec![0]),
        (63, vec![0xfc]),
        (64, vec![1, 1]),
        (16383, vec![0xfd, 0xff]),
        (16384, vec![2, 0, 1, 0]),
    ] {
        let remark = vec![0x55; length];
        let encoded = system_remark_call_data(&remark);
        assert_eq!(&encoded[..2], &[0, 0]);
        assert_eq!(&encoded[2..2 + prefix.len()], prefix);
        assert_eq!(&encoded[2 + prefix.len()..], remark);
        assert!(chain_signing::decode_runtime_call(&encoded).is_ok());
    }
    assert!(parse_h256(&"ab".repeat(32)).is_ok());
    assert!(parse_h256(&format!("0x{}", "ab".repeat(32))).is_ok());
    for value in ["", "0x", "0xGG", "中文"] {
        assert!(parse_h256(value).is_err());
    }
}
