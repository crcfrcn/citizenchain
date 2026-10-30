//! 所属共享SCALE、Runtime元数据解码和UTF-8截断。

// Substrate service API 固定返回 sc_service::Error，Node 必须保持框架函数签名。
#![allow(clippy::result_large_err)]
use runtime;

use codec::Decode;

use frame_metadata::RuntimeMetadata;
use frame_metadata::RuntimeMetadataPrefixed;
use frame_metadata::META_RESERVED;

use sc_network::NetworkBackend as _;

use sp_api::Metadata as _;

use sp_runtime::traits::Block as BlockT;

/// 父块事件解码和RPC能力探测只信任该块Runtime自身的完整元数据。
pub fn decode_runtime_metadata(raw: &[u8]) -> Result<RuntimeMetadata, String> {
    let mut input = raw;
    let RuntimeMetadataPrefixed(prefix, metadata) = RuntimeMetadataPrefixed::decode(&mut input)
        .map_err(|_| "Runtime元数据SCALE解码失败".to_string())?;
    if prefix != META_RESERVED || !input.is_empty() {
        return Err("Runtime元数据前缀或尾随字节非法".into());
    }
    if !matches!(
        &metadata,
        RuntimeMetadata::V14(_) | RuntimeMetadata::V15(_) | RuntimeMetadata::V16(_)
    ) {
        return Err("Runtime元数据版本不受支持".into());
    }
    Ok(metadata)
}

// ──── SCALE 解码 ────

pub fn decode_hex_storage(hex_str: &str) -> Result<Vec<u8>, String> {
    let clean = hex_str.strip_prefix("0x").unwrap_or(hex_str);
    hex::decode(clean).map_err(|e| format!("hex 解码失败: {e}"))
}

pub fn decode_u64_vec(data: &[u8]) -> Result<Vec<u64>, String> {
    if data.is_empty() {
        return Ok(Vec::new());
    }
    let (count, len_bytes) = read_compact_u32(data, 0)?;
    let mut offset = len_bytes;
    let mut ids = Vec::with_capacity(count as usize);
    for _ in 0..count {
        if offset + 8 > data.len() {
            break;
        }
        let val = u64::from_le_bytes(
            data[offset..offset + 8]
                .try_into()
                .map_err(|_| "SCALE 数据长度不足".to_string())?,
        );
        ids.push(val);
        offset += 8;
    }
    Ok(ids)
}

pub fn read_compact_u32(data: &[u8], offset: usize) -> Result<(u32, usize), String> {
    if offset >= data.len() {
        return Err("Compact<u32> 数据不足".to_string());
    }
    let first = data[offset];
    let mode = first & 0x03;
    match mode {
        0 => Ok(((first >> 2) as u32, 1)),
        1 => {
            if offset + 2 > data.len() {
                return Err("Compact<u32> two-byte 数据不足".to_string());
            }
            let value = (((data[offset + 1] as u32) << 8) | first as u32) >> 2;
            Ok((value, 2))
        }
        2 => {
            if offset + 4 > data.len() {
                return Err("Compact<u32> four-byte 数据不足".to_string());
            }
            let value = ((data[offset + 3] as u32) << 24)
                | ((data[offset + 2] as u32) << 16)
                | ((data[offset + 1] as u32) << 8)
                | (data[offset] as u32);
            Ok((value >> 2, 4))
        }
        _ => Err("Compact<u32> big-integer 模式暂不支持".to_string()),
    }
}

/// 从当前位置解码 SCALE `CidNumber`，并推进 offset。
pub fn decode_cid_number(data: &[u8], offset: &mut usize) -> Option<String> {
    let (len, len_len) = read_compact_u32(data, *offset).ok()?;
    *offset = offset.checked_add(len_len)?;
    let end = offset.checked_add(len as usize)?;
    if end > data.len() {
        return None;
    }
    let cid = core::str::from_utf8(&data[*offset..end]).ok()?.to_string();
    *offset = end;
    Some(cid)
}

/// 从当前位置解码 SCALE `RoleCode`，并推进 offset。
pub fn decode_role_code(data: &[u8], offset: &mut usize) -> Option<String> {
    let (len, len_len) = read_compact_u32(data, *offset).ok()?;
    *offset = offset.checked_add(len_len)?;
    let end = offset.checked_add(len as usize)?;
    if end > data.len() {
        return None;
    }
    let role_code = core::str::from_utf8(&data[*offset..end]).ok()?.to_string();
    *offset = end;
    Some(role_code)
}

pub fn decode_subject_cid_numbers(data: &[u8], offset: usize) -> Option<(Vec<String>, usize)> {
    let (count, count_len) = read_compact_u32(data, offset).ok()?;
    let mut pos = offset + count_len;
    let mut cids = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let (len, len_len) = read_compact_u32(data, pos).ok()?;
        pos += len_len;
        let end = pos.checked_add(len as usize)?;
        if end > data.len() {
            return None;
        }
        cids.push(String::from_utf8_lossy(&data[pos..end]).to_string());
        pos = end;
    }
    Some((cids, pos))
}

// ──── summary 格式化(纯函数,便于单测) ────

/// 安全截断 UTF-8 字符串(按 Unicode 字符数计,避免切中 multi-byte 字符)。
pub fn truncate_chars(s: &str, max_chars: usize) -> String {
    let mut chars = s.chars();
    let truncated: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        format!("{truncated}…")
    } else {
        truncated
    }
}

#[cfg(test)]
#[path = "../tests/codec.rs"]
mod tests;
