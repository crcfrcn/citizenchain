//! Golden fixtures 契约测试。
//!
//! `citizenchain/crates/qr-protocol/tests/fixtures/*.json` 是 QR_V1 五种码型的
//! 唯一样例真源,各端注释都指向它。但在本测试出现之前,**全仓没有任何测试加载过
//! 它们** —— 全部引用都只是注释。结果是 fixtures 与实现各自演化:2026-08-06 审计
//! 发现 `user_contact`/`user_transfer` 还停在旧长字段、`sign_request` 的动作码
//! `515`(0x0203)根本不在 registry 里,即唯一的签名请求样例是不可签的。
//!
//! 本测试把 fixtures 钉死为可回归的契约:
//! - 五种码型齐备,文件名与 `k` 值一一对应
//! - 信封字段集精确(固定码禁带 `i`/`e`,临时码必带)
//! - body 键集精确等于 spec 规定的单字母集合
//! - `account_id`(`n`)必须是小写 `0x` + 64 hex
//! - 签名请求的动作码必须在 registry 在册(否则签名端按铁律必须拒签)

// 金标夹具读取失败必须立即中止测试，断言式解包仅限本测试目标。
#![allow(clippy::expect_used, clippy::unwrap_used)]

use serde_json::Value;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn load(name: &str) -> Value {
    let path = fixtures_dir().join(name);
    let text =
        fs::read_to_string(&path).unwrap_or_else(|e| panic!("读取 {} 失败: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{} 不是合法 JSON: {e}", path.display()))
}

fn keys(value: &Value) -> BTreeSet<String> {
    value
        .as_object()
        .expect("必须是 JSON 对象")
        .keys()
        .cloned()
        .collect()
}

fn expect_keys(value: &Value, expected: &[&str], what: &str) {
    let actual = keys(value);
    let want: BTreeSet<String> = expected.iter().map(|s| (*s).to_string()).collect();
    assert_eq!(
        actual, want,
        "{what} 键集不符;实际 {actual:?},期望 {want:?}"
    );
}

const ACCOUNT_ID_LEN: usize = 66; // "0x" + 64 hex

fn expect_account_id(body: &Value, key: &str, what: &str) {
    let v = body[key]
        .as_str()
        .unwrap_or_else(|| panic!("{what}.{key} 必须是字符串"));
    assert_eq!(
        v.len(),
        ACCOUNT_ID_LEN,
        "{what}.{key} 必须是 0x + 64 hex,实际 {v}"
    );
    assert!(v.starts_with("0x"), "{what}.{key} 必须以小写 0x 开头:{v}");
    assert!(
        v[2..]
            .chars()
            .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c)),
        "{what}.{key} 只允许小写十六进制:{v}"
    );
}

/// 五个 fixture 文件齐备,且 `p`/`k` 与文件名对应。
#[test]
fn fixtures_cover_all_five_kinds_with_matching_k() {
    for (name, k) in [
        ("sign_request.json", 1),
        ("sign_response.json", 2),
        ("user_contact.json", 3),
        ("user_transfer.json", 4),
        ("account_id_code.json", 5),
    ] {
        let env = load(name);
        assert_eq!(env["p"], "QR_V1", "{name} 的 p 必须是 QR_V1");
        assert_eq!(env["k"], k, "{name} 的 k 必须是 {k}");
        assert!(env["k"].is_u64(), "{name} 的 k 必须是整数,不能是字符串");
    }
}

/// 固定码(k=3/k=5)禁带 `i`/`e`;临时码(k=1/2/4)必带。
#[test]
fn fixture_envelopes_respect_fixed_and_temporary_shape() {
    for name in ["user_contact.json", "account_id_code.json"] {
        expect_keys(&load(name), &["p", "k", "b"], name);
    }
    for name in [
        "sign_request.json",
        "sign_response.json",
        "user_transfer.json",
    ] {
        expect_keys(&load(name), &["p", "k", "i", "e", "b"], name);
    }
}

/// body 键集必须精确等于 spec 规定的单字母集合。
#[test]
fn fixture_bodies_use_exact_single_letter_keys() {
    expect_keys(
        &load("sign_request.json")["b"],
        &["a", "g", "u", "d"],
        "sign_request.b",
    );
    expect_keys(
        &load("sign_response.json")["b"],
        &["u", "s"],
        "sign_response.b",
    );
    expect_keys(
        &load("user_contact.json")["b"],
        &["c", "n"],
        "user_contact.b",
    );
    expect_keys(
        &load("user_transfer.json")["b"],
        &["n", "v", "t", "m", "l"],
        "user_transfer.b",
    );
    expect_keys(
        &load("account_id_code.json")["b"],
        &["n"],
        "account_id_code.b",
    );
}

/// 旧长字段一律不得复活(spec 铁律:遇旧字段必须报错)。
#[test]
fn fixtures_contain_no_legacy_long_field_names() {
    const LEGACY: [&str; 5] = [
        "cid_number",
        "ss58_address",
        "display_name",
        "recipient_name",
        "account_id",
    ];
    for name in [
        "sign_request.json",
        "sign_response.json",
        "user_contact.json",
        "user_transfer.json",
        "account_id_code.json",
    ] {
        let body = load(name)["b"].clone();
        let actual = keys(&body);
        for legacy in LEGACY {
            assert!(
                !actual.contains(legacy),
                "{name} 的 body 出现已废弃字段 {legacy};单字母键统一后旧长键必须彻底消失"
            );
        }
    }
}

/// 所有携带 `n` 的码型,其 `account_id` 必须是规范小写 `0x` + 64 hex。
#[test]
fn fixture_account_ids_are_canonical() {
    for name in [
        "user_contact.json",
        "user_transfer.json",
        "account_id_code.json",
    ] {
        expect_account_id(&load(name)["b"], "n", name);
    }
}

/// 签名请求 fixture 的动作码必须在 registry 在册。
///
/// 否则签名端查不到中文动作名,按铁律必须红色拒签 —— 一个不可签的样例当金标是自欺。
#[test]
fn sign_request_fixture_action_is_registered() {
    let env = load("sign_request.json");
    let code = env["b"]["a"].as_u64().expect("b.a 必须是整数");
    let code = u16::try_from(code).expect("动作码必须落在 u16");
    let entry = qr_protocol::registry::action_by_code(code)
        .unwrap_or_else(|e| panic!("sign_request fixture 的动作码 {code} 未在 registry 登记: {e}"));
    assert_eq!(
        entry.qr_kind, "sign_request",
        "该动作码的 qr_kind 必须是 sign_request,实际 {}",
        entry.qr_kind
    );
    assert!(
        !entry.action_label_zh.trim().is_empty(),
        "在册动作必须有中文标签,否则签名端无法展示"
    );
    assert_eq!(env["b"]["g"], 1, "g 必须为 1(sr25519)");
}
