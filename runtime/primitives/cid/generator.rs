//! CID 号核心生成协议。
//! 这里只处理字节协议;SQLite、时间、UUID 和查重由 registry 负责。

use alloc::{format, string::String};

use sp_crypto_hashing::blake2_256;

use crate::cid::{
    code::{self, ProfitPolicy},
    number::{checksum_char_m1, checksum_char_mod36},
};

/// 个人主体公开编码只精确到省。
pub const RESERVED_PROVINCE_CITY_CODE: &str = "000";

pub struct GenerateCidNumberInput<'a> {
    pub public_key: &'a str,
    /// 可变/继承盈利策略读取的 0/1 输入。
    pub p1: &'a str,
    /// 两位省行政区代码。
    pub province_code: &'a str,
    /// 省行政区名称,N9 hash 使用。
    pub province_name: &'a str,
    /// 三位市行政区代码。
    pub city_code: &'a str,
    /// 市行政区名称,N9 hash 使用。
    pub city_name: &'a str,
    /// 生成年份 YYYY。
    pub year: &'a str,
    /// 机构码或机构简称。
    pub institution: &'a str,
}

fn hash_text(input: &str) -> u32 {
    let digest = blake2_256(input.as_bytes());
    let mut out = [0_u8; 4];
    out.copy_from_slice(&digest[..4]);
    u32::from_le_bytes(out)
}

/// 公民号段用:blake2_256 前 8 字节小端 → u64,承载 12 位十进制号段(1e12/年)。
fn hash_u64(input: &str) -> u64 {
    let digest = blake2_256(input.as_bytes());
    let mut out = [0_u8; 8];
    out.copy_from_slice(&digest[..8]);
    u64::from_le_bytes(out)
}

fn resolve_profit(p1: &str) -> Result<bool, &'static str> {
    match p1.trim() {
        "0" | "非盈利" => Ok(false),
        "1" | "盈利" => Ok(true),
        _ => Err("p1 must be 0/1 for variable/inherit institution code"),
    }
}

fn valid_ascii_code(value: &str, len: usize) -> bool {
    value.len() == len
        && value
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
}

pub fn generate_cid_number(input: GenerateCidNumberInput<'_>) -> Result<String, &'static str> {
    if input.public_key.trim().is_empty()
        || input.year.trim().is_empty()
        || input.institution.trim().is_empty()
    {
        return Err("public_key, year, institution are required");
    }
    if input.year.len() != 4 || !input.year.bytes().all(|b| b.is_ascii_digit()) {
        return Err("year must be YYYY");
    }

    let institution_code = code::institution_code_from_str(input.institution)
        .ok_or("institution must be a registered CID institution code")?;
    if institution_code == code::PMUL {
        return Err("personal multisig (PMUL) has no cid number");
    }

    // 盈利属性由机构码策略决定。
    let profit = match code::profit_policy(&institution_code) {
        Some(ProfitPolicy::NonProfit) => false,
        Some(ProfitPolicy::Profit) => true,
        Some(ProfitPolicy::Variable | ProfitPolicy::InheritParent) => resolve_profit(input.p1)?,
        None => return Err("institution profit policy missing"),
    };

    let code_str =
        code::institution_code_text(&institution_code).ok_or("institution code text missing")?;

    // 人主体(公民 CTZN / 居民 NATP / 智能人 SMTP)去地域化:R5 = CN 国家码 + 号码高 3 位,
    // N9 = 号码低 9 位。号段容量 = 12 位十进制 = 1e12/年(不占省/市码,`CN` 前缀与机构天然分流)。
    // 人与机构的 CID 生成方式彻底分开:人只吃公钥/码/年,不吃省市;撞号仅由桶数 1e12 决定,
    // 由 registry nonce 探测吸收。
    let (r5, n9) = if code::is_person_code(&institution_code) {
        let number = hash_u64(&format!("{}|{}|{}", input.public_key, code_str, input.year))
            % 1_000_000_000_000;
        let high3 = number / 1_000_000_000; // 0..=999
        let low9 = number % 1_000_000_000;
        (format!("CN{high3:03}"), format!("{low9:09}"))
    } else {
        // 机构:R5 = 省码 + 市码。
        if input.province_code.trim().is_empty()
            || input.province_name.trim().is_empty()
            || input.city_name.trim().is_empty()
        {
            return Err("province_code, province_name, city_name are required");
        }
        if !valid_ascii_code(input.province_code, 2) {
            return Err("province_code must be 2 uppercase ascii chars");
        }
        if !valid_ascii_code(input.city_code, 3) {
            return Err("city_code must be 3 uppercase ascii chars");
        }
        // 同一分类四元组共享 10 亿 n9 桶;碰撞由 registry 处理。
        let n9 = format!(
            "{:09}",
            (hash_text(&format!(
                "{}|{}|{}|{}|{}",
                input.public_key, code_str, input.province_name, input.city_name, input.year
            )) as usize)
                % 1_000_000_000
        );
        (format!("{}{}", input.province_code, input.city_code), n9)
    };

    if code::is_three_char_code(&institution_code) {
        let profit_char = if profit { "1" } else { "0" };
        let payload = format!("{r5}{code_str}{profit_char}{n9}{}", input.year);
        let c = checksum_char_mod36(&payload);
        Ok(format!(
            "{r5}-{code_str}{profit_char}{c}-{n9}-{}",
            input.year
        ))
    } else {
        let payload = format!("{r5}{code_str}{n9}{}", input.year);
        let m1 = checksum_char_m1(&payload, profit);
        Ok(format!("{r5}-{code_str}{m1}-{n9}-{}", input.year))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gen(institution: &str, p1: &str) -> String {
        generate_cid_number(GenerateCidNumberInput {
            public_key: "0xabcd",
            p1,
            province_code: "GD",
            province_name: "广东省",
            city_code: "001",
            city_name: "荔湾市",
            year: "2026",
            institution,
        })
        .expect("cid should generate")
    }

    #[test]
    fn all_person_codes_are_country_prefixed_not_province() {
        // 人主体(公民 CTZN / 居民 NATP / 智能人 SMTP)统一去地域化:R5 = CN + 号码高 3 位。
        for institution in ["CTZN", "NATP", "SMTP"] {
            let code = gen(institution, "1");
            let r5 = code.split('-').next().expect("r5 segment");
            assert_eq!(&r5[0..2], "CN", "{institution} r5 must be CN-prefixed");
            assert!(r5[2..5].chars().all(|c| c.is_ascii_digit()));
            assert_ne!(&r5[0..2], "GD");
        }
    }

    #[test]
    fn person_cid_needs_no_province() {
        // 自助占号无居住地:人主体号段只吃公钥/码/年,省市留空也能生成(区别于机构码)。
        for institution in ["CTZN", "NATP", "SMTP"] {
            let code = generate_cid_number(GenerateCidNumberInput {
                public_key: "0xabcd",
                p1: "1",
                province_code: "",
                province_name: "",
                city_code: "",
                city_name: "",
                year: "2026",
                institution,
            })
            .unwrap_or_else(|e| panic!("{institution} cid should generate without province: {e}"));
            assert_eq!(&code[0..2], "CN");
        }
    }

    #[test]
    fn citizen_cid_number_golden() {
        // 人主体 CID 号段金标(model B dev //0 公钥,CTZN,2026 年)。CN 前缀 + 12 位号段:
        // R5[2:5] 高 3 位 + N9 低 9 位共同构成 1e12/年容量。逐字节钉死,防生成规则漂移。
        let code = generate_cid_number(GenerateCidNumberInput {
            public_key: "0x2afba9278e30ccf6a6ceb3a8b6e336b70068f045c666f2e7f4f9cc5f47db8972",
            p1: "1",
            province_code: "",
            province_name: "",
            city_code: "",
            city_name: "",
            year: "2026",
            institution: "CTZN",
        })
        .expect("citizen golden cid should generate");
        assert_eq!(code, "CN951-CTZN1-539598435-2026");
    }

    #[test]
    fn public_legal_keeps_real_city_code() {
        let code = gen("CGOV", "0");
        assert_eq!(code.split('-').next(), Some("GD001"));
    }

    #[test]
    fn three_char_national_layout_shape() {
        let code = gen("NRC", "0");
        let seg2 = code.split('-').nth(1).unwrap();
        assert_eq!(seg2.len(), 5);
        assert_eq!(&seg2[0..3], "NRC");
        assert_eq!(&seg2[3..4], "0");
    }

    #[test]
    fn pmul_has_no_number() {
        let r = generate_cid_number(GenerateCidNumberInput {
            public_key: "0x1",
            p1: "0",
            province_code: "GD",
            province_name: "广东省",
            city_code: "001",
            city_name: "荔湾市",
            year: "2026",
            institution: "PMUL",
        });
        assert!(r.is_err());
    }
}
