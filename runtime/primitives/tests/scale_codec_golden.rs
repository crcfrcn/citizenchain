//! SCALE 编码原语金标测试。
//! `SCALE_GOLDEN_UPDATE=1` 重写 fixture;默认只断言编码结果不漂移。
//!
//! 为什么需要:签名 payload 的字节由各端**手写**的 SCALE 编码拼出——
//! Worker TS 的 `scaleCompact`/`scaleString`/`u64Le`、citizenapp Dart 的同名函数、
//! citizenwallet Dart 的 `_decodeCompactU32`(解码方向)。三端此前都只用自己的实现
//! 构造期望值,实现算错期望值同步错,测试照样绿。
//!
//! 本文件用 parity-scale-codec 生成真值,各端直读比对,把自证变他证。
//! 编码错 → 签出链端不认的交易;解码错 → 冷钱包展示的交易内容与实际要签的不符,
//! 用户在错误信息下按下签名,后者危险一个量级。

use codec::{Compact, Encode};

const FIXTURE_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/scale_codec_vectors.json"
);

const UPDATE_ENV: &str = "SCALE_GOLDEN_UPDATE";

/// `u64_le` 向量的上界。
///
/// 两端上界本就不同:TS 有 `Number.isSafeInteger` 守卫(2^53-1),Dart `int` 为 64 位
/// 有符号(2^63-1)。向量取二者交集,超过 2^53-1 会让 TS 侧恒红。实际字段(时间戳、
/// binding_revision)远小于此，该差异暂不构成问题,但向量不得跨过这条线。
const U64_SAFE_MAX: u64 = (1u64 << 53) - 1;

fn hex_encode(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{:02x}", b));
    }
    s
}

fn load_fixture() -> serde_json::Value {
    let raw = std::fs::read_to_string(FIXTURE_PATH)
        .unwrap_or_else(|e| panic!("读取金标 fixture 失败 {FIXTURE_PATH}: {e}"));
    serde_json::from_str(&raw).expect("金标 fixture 不是合法 JSON")
}

/// compact<u32> 取值:三档分支(< 2^6 / < 2^14 / < 2^30)的两侧边界各取一对。
/// 每一对跨越一次字节数跳变,手写实现把 `<` 写成 `<=` 立即暴露。
fn compact_values() -> Vec<u32> {
    vec![
        0,
        1,
        63, // 1 字节上界
        64, // 2 字节下界
        255,
        16_383, // 2 字节上界
        16_384, // 4 字节下界
        65_535,
        (1u32 << 30) - 1, // 4 字节上界(各端手写实现的支持上限)
    ]
}

/// SCALE 字符串取值:长度前缀跨档 + UTF-8 字节数 ≠ 字符数。
fn string_values() -> Vec<String> {
    vec![
        String::new(),
        "a".to_string(),
        "x".repeat(63),                           // 长度前缀 1 字节上界
        "x".repeat(64),                           // 长度前缀 2 字节下界
        "中华民族联邦共和国".to_string(),         // 每字符 3 字节
        "公民🧪链".to_string(),                   // 含 4 字节中性 Unicode 码点
        "CN220-CTZN2-198805200-2026".to_string(), // 真实 CID 形态
    ]
}

/// u64 小端取值:字节序 + JS 安全整数边界。
fn u64_values() -> Vec<u64> {
    vec![
        0,
        1,
        255,
        256,
        u64::from(u32::MAX),
        1u64 << 32,
        U64_SAFE_MAX,
    ]
}

/// 生成当前真值。返回 (compact, string, u64) 三组 JSON 数组。
fn compute_vectors() -> (
    Vec<serde_json::Value>,
    Vec<serde_json::Value>,
    Vec<serde_json::Value>,
) {
    let compact = compact_values()
        .into_iter()
        .map(|value| {
            serde_json::json!({
                "value": value,
                "hex": hex_encode(&Compact(value).encode()),
            })
        })
        .collect();

    let strings = string_values()
        .into_iter()
        .map(|value| {
            // Rust `String` 的 Encode 即 `Compact(len) ++ utf8`,与各端 scaleString 同构。
            let encoded = value.encode();
            serde_json::json!({
                "value": value,
                "utf8_len": value.len(),
                "hex": hex_encode(&encoded),
            })
        })
        .collect();

    let u64s = u64_values()
        .into_iter()
        .map(|value| {
            // SCALE 对 u64 的 Encode 就是小端 8 字节,与各端 u64Le 同构。
            let encoded = value.encode();
            assert_eq!(
                encoded,
                value.to_le_bytes(),
                "u64 SCALE 编码应为小端 8 字节"
            );
            serde_json::json!({
                "value": value,
                "hex": hex_encode(&encoded),
            })
        })
        .collect();

    (compact, strings, u64s)
}

fn assert_group_matches(name: &str, computed: &[serde_json::Value], fixture: &serde_json::Value) {
    let recorded = fixture[name]
        .as_array()
        .unwrap_or_else(|| panic!("fixture 缺少数组 {name},请先跑 {UPDATE_ENV}=1 回填"));
    assert_eq!(
        recorded.len(),
        computed.len(),
        "{name}: fixture 向量条数与生成器不符,增删取值后须跑 {UPDATE_ENV}=1 回填"
    );
    for (index, expected) in computed.iter().enumerate() {
        assert_eq!(
            &recorded[index], expected,
            "{name}[{index}]: SCALE 编码与金标不一致(编码漂移!)"
        );
    }
}

#[test]
fn scale_codec_golden_vectors() {
    let (compact, strings, u64s) = compute_vectors();
    let update = std::env::var(UPDATE_ENV).map(|v| v == "1").unwrap_or(false);

    if update {
        let fixture = serde_json::json!({
            "_comment": "SCALE 编码原语金标向量 (canonical, Rust parity-scale-codec 权威源). \
                         各端手写 SCALE 实现(Worker scaleCompact/scaleString/u64Le、\
                         citizenapp signing.dart、citizenwallet payload_decoder 解码方向)\
                         必须与本文件逐字节一致。\
                         【真源】改动取值集合后必须跑 SCALE_GOLDEN_UPDATE=1 重新生成。\
                         u64 上界钉在 2^53-1:TS 侧 Number.isSafeInteger 守卫不接受更大值。",
            "compact_u32": compact,
            "scale_string": strings,
            "u64_le": u64s,
        });
        let pretty = serde_json::to_string_pretty(&fixture).expect("序列化 fixture 失败");
        std::fs::write(FIXTURE_PATH, format!("{pretty}\n"))
            .unwrap_or_else(|e| panic!("写回金标 fixture 失败 {FIXTURE_PATH}: {e}"));
        eprintln!("[scale golden] 已用 parity-scale-codec 重算并写回 {FIXTURE_PATH}");
        return;
    }

    let fixture = load_fixture();
    assert_group_matches("compact_u32", &compact, &fixture);
    assert_group_matches("scale_string", &strings, &fixture);
    assert_group_matches("u64_le", &u64s, &fixture);
}

#[test]
fn compact_encoding_flags_match_the_two_bit_mode() {
    // 独立于向量表的第二道锁:按 SCALE 规范推导,不看上面的取值集合。
    // compact 用低 2 位标记字节数(0b00=1B / 0b01=2B / 0b10=4B),
    // 手写实现最常见的错误就是这两位或移位写错。
    for value in compact_values() {
        let encoded = Compact(value).encode();
        let mode = encoded[0] & 0b11;
        let expected_len = match value {
            v if v < 1 << 6 => 1,
            v if v < 1 << 14 => 2,
            _ => 4,
        };
        let expected_mode = match expected_len {
            1 => 0b00,
            2 => 0b01,
            _ => 0b10,
        };
        assert_eq!(
            encoded.len(),
            expected_len,
            "compact({value}) 字节数应为 {expected_len}"
        );
        assert_eq!(
            mode, expected_mode,
            "compact({value}) 低 2 位模式标记应为 {expected_mode:#04b}"
        );
    }
}

#[test]
fn scale_string_is_compact_len_then_utf8() {
    // 字符串编码 = compact(字节长度) ++ utf8。这里断言"长度前缀用的是**字节**数
    // 而非字符数"——中文与 emoji 用例专门覆盖这一点,各端若误用字符数会立即暴露。
    for value in string_values() {
        let encoded = value.encode();
        let prefix = Compact(value.len() as u32).encode();
        assert!(
            encoded.starts_with(&prefix),
            "{value:?} 的编码未以 compact(utf8 字节长度) 开头"
        );
        assert_eq!(
            &encoded[prefix.len()..],
            value.as_bytes(),
            "{value:?} 的编码尾部不是原始 utf8 字节"
        );
    }
}
