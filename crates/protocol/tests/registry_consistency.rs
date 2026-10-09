// 协议 fixture 缺失或不一致必须立即中止测试，断言式解包仅限本测试目标。
#![allow(clippy::expect_used, clippy::unwrap_used)]

use qr_protocol::export::{
    export_qr_bodies_dart, export_qr_bodies_rust, export_qr_bodies_typescript, export_registry_dart,
};
use qr_protocol::registry::{actions, fields, kinds, reject_reasons, SigningCategory};
use std::collections::HashSet;
use std::fs;
use std::path::Path;

/// decoder 按条件发射、但不进任何 action `required_fields` 的字段。
///
/// 这些 key 由 `citizenwallet/lib/signer/payload_decoder.dart` 在 Option 字段命中时才发射,
/// Rust 侧无法从 registry 推出,只能在此显式登记。新增前必须先确认 decoder 真的发射该 key,
/// 不得拿本表豁免死标签。
const DECODER_ONLY_FIELDS: &[&str] = &[
    "birth_date",
    "executive_cid_number",
    "legislature_cid_number",
    "personal_account_id",
    "spec_version",
    "transaction_version",
];

const REMOVED_AMBIGUOUS_ACCOUNT_FIELDS: &[&str] = &[
    "wallet_account",
    "admin_account",
    "owner_account",
    "signer_pubkey",
    "credential_signer_pubkey",
    "actor_pubkey",
    "admin_pubkey",
    "operator_account",
    "target_account",
    "institution_account",
    "personal_account",
    "execution_account",
    "funding_account",
    "operation_fee_payer",
    "execution_fee_payer",
    "fee_payer",
    "beneficiary",
    "from",
    "to",
    "who",
    "bank_main",
    "new_bank",
    "account",
    "address",
];

#[test]
fn actions_have_unique_keys_codes_and_chinese_labels() {
    let actions = actions().expect("actions.yaml 必须可解析");
    let mut keys = HashSet::new();
    let mut codes = HashSet::new();

    for action in actions {
        assert!(
            keys.insert(action.action_key.clone()),
            "action_key 重复: {}",
            action.action_key
        );
        assert!(
            codes.insert(action.action_code),
            "action_code 重复: 0x{:04x}",
            action.action_code
        );
        assert!(
            !action.action_label_zh.trim().is_empty(),
            "{} 缺少中文动作名",
            action.action_key
        );
        assert!(
            !action.decoder.trim().is_empty(),
            "{} 缺少 decoder",
            action.action_key
        );
        assert_eq!(
            action.qr_kind, "sign_request",
            "{} 不能新增登录专用或业务专用 QR kind",
            action.action_key
        );
    }
}

/// 自助占号必须是完整链交易，并与现有Runtime call5保持唯一动作登记。
#[test]
fn self_occupy_cid_is_a_full_chain_transaction() {
    let entries = actions().expect("动作登记可解析");
    let action = entries
        .iter()
        .find(|a| a.action_code == 0x0a05)
        .expect("自助占号已登记");
    assert_eq!(action.action_key, "self_occupy_cid");
    assert_eq!(action.call.as_deref(), Some("self_occupy_cid"));
    assert_eq!(action.kind, qr_protocol::registry::ActionKind::ChainCall);
    assert!(!action.hash_only_allowed);
    assert_eq!(action.required_fields, vec!["cid_number"]);
}

#[test]
fn required_fields_all_have_chinese_labels() {
    let actions = actions().expect("actions.yaml 必须可解析");
    let fields = fields().expect("fields.yaml 必须可解析");
    let field_keys: HashSet<_> = fields
        .iter()
        .map(|field| field.field_key.as_str())
        .collect();

    for action in actions {
        for field_key in action.required_fields {
            assert!(
                field_keys.contains(field_key.as_str()),
                "{} required_fields 缺少中文字段登记: {}",
                action.action_key,
                field_key
            );
        }
    }
}

#[test]
fn field_and_reject_reason_keys_are_unique_and_chinese() {
    let fields = fields().expect("fields.yaml 必须可解析");
    let mut field_keys = HashSet::new();
    for field in fields {
        assert!(
            field_keys.insert(field.field_key.clone()),
            "field_key 重复: {}",
            field.field_key
        );
        assert!(
            !field.field_label_zh.trim().is_empty(),
            "{} 缺少中文字段名",
            field.field_key
        );
    }

    let reasons = reject_reasons().expect("reject_reasons.yaml 必须可解析");
    let mut reason_keys = HashSet::new();
    for reason in reasons {
        assert!(
            reason_keys.insert(reason.reject_reason_key.clone()),
            "reject_reason_key 重复: {}",
            reason.reject_reason_key
        );
        assert!(
            !reason.reject_reason_zh.trim().is_empty(),
            "{} 缺少中文拒绝原因",
            reason.reject_reason_key
        );
    }
}

#[test]
fn removed_ambiguous_account_fields_cannot_return() {
    let actions = actions().expect("actions.yaml 必须可解析");
    for action in actions {
        for field_key in action.required_fields {
            assert!(
                !REMOVED_AMBIGUOUS_ACCOUNT_FIELDS.contains(&field_key.as_str()),
                "{} required_fields 恢复了已删除的含糊账户字段: {}",
                action.action_key,
                field_key
            );
        }
    }

    let fields = fields().expect("fields.yaml 必须可解析");
    for field in fields {
        assert!(
            !REMOVED_AMBIGUOUS_ACCOUNT_FIELDS.contains(&field.field_key.as_str()),
            "fields.yaml 恢复了已删除的含糊账户字段: {}",
            field.field_key
        );
    }
}

#[test]
fn hash_only_is_limited_to_runtime_upgrade() {
    let actions = actions().expect("actions.yaml 必须可解析");
    for action in actions {
        if action.hash_only_allowed {
            assert_eq!(
                action.signing_category,
                SigningCategory::RuntimeUpgrade,
                "{} 只有 Runtime 升级允许 hash-only",
                action.action_key
            );
            assert!(
                action.action_key.contains("runtime_upgrade")
                    || action.action_key == "developer_direct_upgrade",
                "{} 不是 Runtime 升级动作,不能 hash-only",
                action.action_key
            );
        }
    }
}

/// 反向校验:fields.yaml 不得积累无人引用的孤儿中文名。
///
/// `required_fields_all_have_chinese_labels` 只做 actions → fields 单向校验,
/// 缺这条反向校验正是历史孤儿标签堆积的成因。
#[test]
fn fields_yaml_has_no_orphan_entries() {
    let actions = actions().expect("actions.yaml 必须可解析");
    let mut referenced = HashSet::new();
    for action in actions {
        for field_key in action.required_fields {
            referenced.insert(field_key);
        }
    }

    let fields = fields().expect("fields.yaml 必须可解析");
    for field in fields {
        assert!(
            referenced.contains(&field.field_key)
                || DECODER_ONLY_FIELDS.contains(&field.field_key.as_str()),
            "fields.yaml 存在孤儿字段中文名: {} — 没有任何 action 的 required_fields 引用它,\
             也不在 DECODER_ONLY_FIELDS 登记表内。删掉它,或先确认 decoder 确实发射后再登记。",
            field.field_key
        );
    }
}

// 中文注释：生成器金标固定当前注册表正式导出的完整字节，不读取未发布邻仓。
fn generated_sha256(source: &str) -> Result<String, Box<dyn std::error::Error>> {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let mut child = Command::new("node")
        .args(["-e", "const fs=require('node:fs'),crypto=require('node:crypto');process.stdout.write(crypto.createHash('sha256').update(fs.readFileSync(0)).digest('hex')); "])
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::inherit()).spawn()?;
    child
        .stdin
        .take()
        .ok_or("金标输入通道缺失")?
        .write_all(source.as_bytes())?;
    let output = child.wait_with_output()?;
    let value = String::from_utf8(output.stdout)?;
    if !output.status.success()
        || value.len() != 64
        || !value.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Err("QR生成器金标摘要执行失败".into());
    }
    Ok(value)
}

#[test]
fn generated_dart_registries_are_current() {
    let expected = export_registry_dart().expect("Dart registry 必须可生成");
    assert_eq!(
        generated_sha256(&expected).expect("Dart原消费者金标摘要"),
        "c7ce2f501a02a52739438b39668c1d06a07772ef1c9b3f82a173482e360122e6",
        "Dart registry 与保留的原消费者完整字节不一致"
    );
}

#[test]
fn qr_kinds_are_unique_and_constraints_are_closed() {
    let kinds = kinds().expect("kinds.yaml 必须可解析");
    let actions = actions().expect("actions.yaml 必须可解析");
    let action_keys = actions
        .iter()
        .map(|action| action.action_key.as_str())
        .collect::<HashSet<_>>();
    let mut kind_keys = HashSet::new();
    let mut kind_codes = HashSet::new();
    for kind in kinds {
        assert!(kind_keys.insert(kind.kind_key.clone()), "kind_key 重复");
        assert!(kind_codes.insert(kind.kind_code), "kind_code 重复");
        let field_keys = kind
            .fields
            .iter()
            .map(|field| field.wire_key.as_str())
            .collect::<HashSet<_>>();
        assert_eq!(field_keys.len(), kind.fields.len(), "body wire_key 重复");
        for field in &kind.fields {
            for action_key in &field.empty_for_actions {
                assert!(
                    action_keys.contains(action_key.as_str()),
                    "{} 引用了未登记动作 {}",
                    field.wire_key,
                    action_key
                );
            }
        }
        for pair in kind.optional_pairs {
            assert!(field_keys.contains(pair[0].as_str()));
            assert!(field_keys.contains(pair[1].as_str()));
        }
    }
    assert_eq!(kind_codes, HashSet::from([1, 2, 3, 4, 5]));
}

/// MLS登记只保留动作13与同一public_key；未登记动作和码型必须拒绝。
#[test]
fn mls_device_bind_is_the_only_public_key_registration() {
    let entries = actions().expect("动作登记可解析");
    let action = entries
        .iter()
        .find(|entry| entry.action_code == 13)
        .expect("MLS登记动作已登记");
    assert_eq!(action.action_key, "mls_device_bind");
    assert_eq!(action.decoder, "mls_device_bind");
    assert_eq!(action.signing_category, SigningCategory::MlsDeviceBind);
    assert_eq!(
        action.required_fields,
        vec![
            "cid_number",
            "binding_revision",
            "account_id",
            "public_key",
            "issued_at"
        ]
    );
    assert!(entries.iter().all(|entry| entry.action_code != 14));
    assert!(kinds()
        .expect("码型登记可解析")
        .iter()
        .all(|entry| entry.kind_code != 6));
}

#[test]
fn generated_qr_body_validators_are_current() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("qr-protocol 必须位于 citizenchain/crates/protocol");
    assert_eq!(
        generated_sha256(&export_qr_bodies_dart().expect("Dart body schema 必须可生成"))
            .expect("Dart body 原消费者金标摘要"),
        "b8e228e48f8f1a49171c138148aae7b8a8a98f53253d1a1b937a9ea65c4dfec8",
        "Dart body 与原消费者完整字节不一致"
    );
    let cases = [
        (
            "node/frontend/protocol/qrBodies.g.ts",
            export_qr_bodies_typescript().expect("TS body schema 必须可生成"),
        ),
        (
            "onchina/frontend/core/qrBodies.g.ts",
            export_qr_bodies_typescript().expect("TS body schema 必须可生成"),
        ),
        (
            "onchina/src/protocol/generated.rs",
            export_qr_bodies_rust().expect("Rust body schema 必须可生成"),
        ),
    ];
    for (relative, expected) in cases {
        let path = repo_root.join(relative);
        let actual = fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("{} 读取失败: {error}", path.display()));
        assert_eq!(actual, expected, "{} 不是最新生成产物", path.display());
    }
}
