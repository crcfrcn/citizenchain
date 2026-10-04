//! 签名协议金标测试。
//! `SIGN_GOLDEN_UPDATE=1` 重写 fixture;默认只断言 signing_message 不漂移。

use codec::Encode;
use primitives::core_const::GMB;
use primitives::sign::{signing_message, SIGN_OP_TAGS};

const FIXTURE_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/signing_domain_vectors.json"
);

const UPDATE_ENV: &str = "SIGN_GOLDEN_UPDATE";

// 极简 hex 编解码。

fn hex_encode(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{:02x}", b));
    }
    s
}

fn hex_decode(s: &str) -> Vec<u8> {
    assert!(s.len().is_multiple_of(2), "hex 串长度必须为偶数: {s}");
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("非法 hex 字符"))
        .collect()
}

fn parse_op_tag(v: &serde_json::Value) -> u8 {
    let raw = v["op_tag"]
        .as_str()
        .expect("向量缺少 op_tag(应为 0xNN hex 串)");
    let stripped = raw.strip_prefix("0x").unwrap_or(raw);
    u8::from_str_radix(stripped, 16).unwrap_or_else(|_| panic!("非法 op_tag: {raw}"))
}

fn load_fixture() -> serde_json::Value {
    let raw = std::fs::read_to_string(FIXTURE_PATH)
        .unwrap_or_else(|e| panic!("读取金标 fixture 失败 {FIXTURE_PATH}: {e}"));
    serde_json::from_str(&raw).expect("金标 fixture 不是合法 JSON")
}

#[test]
fn sign_golden_vectors() {
    let mut fixture = load_fixture();

    // fixture 域必须与链端 GMB 一致。
    assert_eq!(
        fixture["domain"].as_str(),
        Some(core::str::from_utf8(GMB).unwrap()),
        "fixture domain 与链端 GMB 不一致"
    );

    let update = std::env::var(UPDATE_ENV).map(|v| v == "1").unwrap_or(false);

    let vectors = fixture["vectors"]
        .as_array()
        .expect("vectors 必须是数组")
        .clone();
    assert!(!vectors.is_empty(), "fixture 至少需 1 条向量");

    // 每个注册 op_tag 都必须有向量。
    for tag in SIGN_OP_TAGS {
        let present = vectors.iter().any(|v| parse_op_tag(v) == tag);
        assert!(present, "op_tag 0x{tag:02x} 在注册表却无金标向量");
    }

    let mut updated = Vec::with_capacity(vectors.len());
    for v in &vectors {
        let op_tag = parse_op_tag(v);
        let payload = hex_decode(
            v["scale_payload_hex"]
                .as_str()
                .expect("缺 scale_payload_hex"),
        );
        let computed = signing_message(op_tag, &payload);
        let computed_hex = hex_encode(&computed);

        // 原语结果必须等于裸拼接哈希。
        let naive = {
            let mut data = Vec::new();
            data.extend_from_slice(GMB);
            data.push(op_tag);
            data.extend_from_slice(&payload);
            sp_crypto_hashing::blake2_256(&data)
        };
        assert_eq!(
            computed, naive,
            "op_tag 0x{op_tag:02x}: signing_message 与朴素拼接不一致(原语实现漂移!)"
        );

        // SCALE 元组域头必须保持 GMB||op_tag。
        let tuple_bytes = (GMB, op_tag, payload.as_slice());
        let scale_tuple = tuple_bytes.encode();
        // &[u8] 会加长度前缀,只比对前 4 字节域头。
        assert_eq!(
            &scale_tuple[..4],
            {
                let mut head = Vec::new();
                head.extend_from_slice(GMB);
                head.push(op_tag);
                head
            }
            .as_slice(),
            "op_tag 0x{op_tag:02x}: SCALE 元组域头 (GMB||op_tag) 字节漂移"
        );

        if update {
            let mut nv = v.clone();
            nv["message_hex"] = serde_json::Value::String(computed_hex);
            updated.push(nv);
        } else {
            let expected = v["message_hex"].as_str().unwrap_or("");
            assert!(
                !expected.is_empty(),
                "op_tag 0x{op_tag:02x}: fixture message_hex 为空,请先跑 {UPDATE_ENV}=1 回填"
            );
            assert_eq!(
                computed_hex, expected,
                "op_tag 0x{op_tag:02x}: signing_message 与金标 fixture 不一致(签名消息漂移!)"
            );
        }
    }

    if update {
        fixture["vectors"] = serde_json::Value::Array(updated);
        let pretty = serde_json::to_string_pretty(&fixture).expect("序列化 fixture 失败");
        std::fs::write(FIXTURE_PATH, format!("{pretty}\n"))
            .unwrap_or_else(|e| panic!("写回金标 fixture 失败 {FIXTURE_PATH}: {e}"));
        eprintln!("[sign golden] 已用 signing_message 重算并写回 {FIXTURE_PATH}");
    }
}
