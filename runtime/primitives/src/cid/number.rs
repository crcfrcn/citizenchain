//! CID 号格式校验。
//! 格式:`R5(5)-核心段(5)-N9(9)-D4(4)`。
//! 核心段:3 字符码走`码+盈利位+mod36`;4 字符码走`码+M1`。

use alloc::{
    format,
    string::{String, ToString},
    vec::Vec,
};

use crate::{
    cid::code::{self, InstitutionCode, ProfitPolicy},
    core_const::CID_NUMBER_MAX_BYTES,
};

const CHECKSUM_ALPHABET: &[u8; 36] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ";

pub const CID_NUMBER_SEGMENT_COUNT: usize = 4;
pub const CID_NUMBER_SEGMENT_R5_LEN: usize = 5;
/// 核心段长度。
pub const CID_NUMBER_SEGMENT_K3P1C1_LEN: usize = 5;
pub const CID_NUMBER_SEGMENT_N9_LEN: usize = 9;
pub const CID_NUMBER_SEGMENT_D4_LEN: usize = 4;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CidNumberParts {
    pub r5: String,
    /// 机构码。
    pub institution: InstitutionCode,
    /// 机构码文本。
    pub institution_code_text: String,
    /// 盈利属性。
    pub profit: bool,
    /// 校验位。
    pub checksum: char,
    pub n9: String,
    pub d4: String,
}

/// 校验和累加器。
pub(crate) fn checksum_acc(payload: &str) -> usize {
    let mut total: usize = 0;
    for (idx, ch) in payload.chars().enumerate() {
        let pos = CHECKSUM_ALPHABET
            .iter()
            .position(|&b| b == ch as u8)
            .unwrap_or(0);
        total = total.wrapping_add((idx + 1) * pos);
    }
    total
}

/// 3 字符布局校验位。
pub fn checksum_char_mod36(payload: &str) -> char {
    CHECKSUM_ALPHABET[checksum_acc(payload) % 36] as char
}

/// 4 字符布局 M1。
pub fn checksum_char_m1(payload: &str, profit: bool) -> char {
    if profit {
        (b'0' + (checksum_acc(payload) % 10) as u8) as char
    } else {
        (b'A' + (checksum_acc(payload) % 26) as u8) as char
    }
}

/// 校验 cid_number 格式。
pub fn validate_cid_number_format(raw: &str) -> Result<String, &'static str> {
    parse_cid_number_parts(raw).map(|_| raw.trim().to_string())
}

/// 从链上字节解析并校验 cid_number。
pub fn parse_cid_number_parts_bytes(raw: &[u8]) -> Result<CidNumberParts, &'static str> {
    let text = core::str::from_utf8(raw).map_err(|_| "cid_number must be utf-8")?;
    parse_cid_number_parts(text)
}

/// 从已校验的机构 CID 唯一解析省、市作用域码。
///
/// 机构 CID 的 R5 固定为“省码 2 字节 + 市码 3 字节”。所有需要按机构 CID
/// 推导治理或登记作用域的模块必须复用本函数，不得自行切割字符串形成第二真源。
/// 人主体(公民/居民/智能人)CID 去地域化,R5 = CN 国家码 + 号段,不承载省/市作用域;
/// 传入人主体 CID 即 fail-closed 返回错误,杜绝把号段误读成区划码。
pub fn cid_scope_codes(raw: &[u8]) -> Result<([u8; 2], [u8; 3]), &'static str> {
    let parts = parse_cid_number_parts_bytes(raw)?;
    if code::is_person_code(&parts.institution) {
        return Err("person cid has no province/city scope");
    }
    let bytes = parts.r5.as_bytes();
    if bytes.len() != CID_NUMBER_SEGMENT_R5_LEN {
        return Err("cid_number r5 segment invalid");
    }
    let mut province_code = [0_u8; 2];
    let mut city_code = [0_u8; 3];
    province_code.copy_from_slice(&bytes[..2]);
    city_code.copy_from_slice(&bytes[2..]);
    Ok((province_code, city_code))
}

/// 解析并校验 cid_number。
pub fn parse_cid_number_parts(raw: &str) -> Result<CidNumberParts, &'static str> {
    let normalized = raw.trim();
    if normalized.is_empty() {
        return Err("cid_number is required");
    }
    if !normalized.is_ascii() {
        return Err("cid_number must be ascii");
    }
    if normalized.len() > CID_NUMBER_MAX_BYTES as usize {
        return Err("cid_number length exceeds chain max");
    }
    if normalized
        .bytes()
        .any(|b| !(b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'-'))
    {
        return Err("cid_number charset invalid");
    }
    let segments = normalized.split('-').collect::<Vec<_>>();
    if segments.len() != CID_NUMBER_SEGMENT_COUNT {
        return Err("cid_number format invalid");
    }
    if segments[0].len() != CID_NUMBER_SEGMENT_R5_LEN
        || !segments[0]
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
    {
        return Err("cid_number r5 segment invalid");
    }
    if segments[1].len() != CID_NUMBER_SEGMENT_K3P1C1_LEN
        || !segments[1]
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
    {
        return Err("cid_number core segment invalid");
    }
    if segments[2].len() != CID_NUMBER_SEGMENT_N9_LEN
        || !segments[2].chars().all(|c| c.is_ascii_digit())
    {
        return Err("cid_number n9 segment invalid");
    }
    if segments[3].len() != CID_NUMBER_SEGMENT_D4_LEN
        || !segments[3].chars().all(|c| c.is_ascii_digit())
    {
        return Err("cid_number date segment invalid");
    }

    let seg2 = segments[1];
    let seg2_bytes = seg2.as_bytes();
    let index3 = seg2_bytes[3] as char;

    let (institution, institution_code_text, profit, checksum) = if index3.is_ascii_digit() {
        // A 布局:3 字符码 + 盈利位 + 校验。
        let code = &seg2[0..3];
        let profit_char = &seg2[3..4];
        let checksum = seg2_bytes[4] as char;
        let institution =
            code::institution_code_from_str(code).ok_or("cid_number institution code invalid")?;
        if !code::is_three_char_code(&institution) {
            return Err("cid_number 3-char layout code mismatch");
        }
        // 盈利位必须与机构码策略一致。
        let profit = match profit_char {
            "0" => false,
            "1" => true,
            _ => return Err("cid_number 3-char layout profit must be 0/1"),
        };
        match code::profit_policy(&institution) {
            Some(ProfitPolicy::NonProfit) if profit => {
                return Err("cid_number profit conflicts with code policy")
            }
            Some(ProfitPolicy::Profit) if !profit => {
                return Err("cid_number profit conflicts with code policy")
            }
            None => return Err("cid_number profit policy missing"),
            _ => {}
        }
        let payload = format!(
            "{}{}{}{}{}",
            segments[0], code, profit_char, segments[2], segments[3]
        );
        if checksum_char_mod36(&payload) != checksum {
            return Err("cid_number checksum invalid");
        }
        (institution, code.to_string(), profit, checksum)
    } else {
        // B 布局:4 字符码 + M1。
        let code = &seg2[0..4];
        let m1 = seg2_bytes[4] as char;
        let institution =
            code::institution_code_from_str(code).ok_or("cid_number institution code invalid")?;
        if code::is_three_char_code(&institution) {
            return Err("cid_number 4-char layout code mismatch");
        }
        let profit = m1.is_ascii_digit();
        // M1 必须与机构码策略一致。
        match code::profit_policy(&institution) {
            Some(ProfitPolicy::NonProfit) if profit => {
                return Err("cid_number profit conflicts with code policy")
            }
            Some(ProfitPolicy::Profit) if !profit => {
                return Err("cid_number profit conflicts with code policy")
            }
            None => return Err("cid_number profit policy missing"),
            _ => {}
        }
        let payload = format!("{}{}{}{}", segments[0], code, segments[2], segments[3]);
        if checksum_char_m1(&payload, profit) != m1 {
            return Err("cid_number checksum invalid");
        }
        (institution, code.to_string(), profit, m1)
    };

    Ok(CidNumberParts {
        r5: segments[0].to_string(),
        institution,
        institution_code_text,
        profit,
        checksum,
        n9: segments[2].to_string(),
        d4: segments[3].to_string(),
    })
}
