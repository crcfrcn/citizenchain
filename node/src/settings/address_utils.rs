// 共享的 32 字节密钥 / 地址解析工具，供 bootnodes_address、grandpa_address、reward_account 复用。
pub(crate) fn decode_hex_32_strict(input: &str) -> Result<[u8; 32], String> {
    if input.len() != 64 || !input.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err("hex 地址格式无效，应为 64 位十六进制".to_string());
    }
    decode_hex_32_raw(input)
}

pub(crate) fn decode_hex_32_with_optional_0x(input: &str) -> Result<[u8; 32], String> {
    let value = input.trim();
    let raw = value.strip_prefix("0x").unwrap_or(value);
    if raw.len() != 64 || !raw.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err("hex 地址格式无效，应为 0x + 64 位十六进制".to_string());
    }
    decode_hex_32_raw(raw)
}

pub(crate) fn decode_ss58_prefix(data: &[u8]) -> Result<(u16, usize), String> {
    if data.is_empty() {
        return Err("SS58 地址为空".to_string());
    }
    let first = data[0];
    match first {
        0..=63 => Ok((first as u16, 1)),
        64..=127 => {
            if data.len() < 2 {
                return Err("SS58 地址格式无效".to_string());
            }
            let second = data[1];
            let prefix = (((first & 0x3f) as u16) << 2)
                | ((second as u16) >> 6)
                | (((second & 0x3f) as u16) << 8);
            Ok((prefix, 2))
        }
        _ => Err("SS58 地址格式无效".to_string()),
    }
}

fn decode_hex_32_raw(raw_hex: &str) -> Result<[u8; 32], String> {
    let mut out = [0u8; 32];
    for (i, chunk) in raw_hex.as_bytes().chunks_exact(2).enumerate() {
        let part = std::str::from_utf8(chunk).map_err(|_| "hex 地址格式无效".to_string())?;
        out[i] = u8::from_str_radix(part, 16).map_err(|_| "hex 地址格式无效".to_string())?;
    }
    Ok(out)
}
