//! 通过真实共享包接口核验导出块，金标只归本包tests。
#![allow(clippy::expect_used, clippy::unwrap_used)]
use blockchain_harness::*;
const BLOCK0_JSON_LINE: &str = include_str!("block.json");

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

#[test]
fn tampering_changes_only_the_first_header_and_preserves_other_records() {
    let first: serde_json::Value = serde_json::from_str(BLOCK0_JSON_LINE).unwrap();
    let mut second = first.clone();
    second["block"]["header"]["number"] = "0x1".into();
    let replacement = format!("0x{}", "ef".repeat(32));
    for input in [
        serde_json::to_string(&vec![first.clone(), second.clone()]).unwrap(),
        format!("\n{}\n\n{}\n", first, second),
    ] {
        let result = tamper_first_state_root_json(&input, &replacement).unwrap();
        let values: Vec<serde_json::Value> = if result.starts_with('[') {
            serde_json::from_str(&result).unwrap()
        } else {
            result
                .lines()
                .map(|line| serde_json::from_str(line).unwrap())
                .collect()
        };
        let mut expected = first.clone();
        expected["block"]["header"]["stateRoot"] = replacement.clone().into();
        assert_eq!(values, vec![expected, second.clone()]);
    }
}

#[test]
fn malformed_tamper_inputs_never_return_a_success_sample() {
    let root = format!("0x{}", "ef".repeat(32));
    for input in ["", " ", "[]", "[{}]", "not-json", "{}"] {
        assert!(tamper_first_state_root_json(input, &root).is_err());
    }
    for root in ["", "0x1234", "中文"] {
        assert!(tamper_first_state_root_json(BLOCK0_JSON_LINE, root).is_err());
    }
}
