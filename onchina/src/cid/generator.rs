//! onchina CID 号生成适配层。
//!
//!
//! CID 号码字节协议在 `primitives::cid::generator`。本文件只负责 onchina 运行态输入:
//! 从 SQLite 行政区真源解析省/市代码、读取当前年份,再调用 runtime primitives 纯协议。

use chrono::Utc;

use crate::cid::china::{city_code_by_name, province_code_by_name};

pub struct GenerateCidInput<'a> {
    pub account_id: &'a str,
    /// 盈利输入,仅 Variable(注册协会/智能人)与 InheritParent(非法人组织,传父级)
    /// 策略的机构码读取;固定盈利策略的码忽略本字段。取值 0/1 或 非盈利/盈利。
    pub p1: &'a str,
    pub province_name: &'a str,
    pub city_name: &'a str,
    /// 机构码(3 或 4 字符代码,或机构码中文标签),全仓库机构分类唯一真源。
    pub institution: &'a str,
}

pub fn generate_cid_number(input: GenerateCidInput<'_>) -> Result<String, &'static str> {
    let account_id = crate::crypto::pubkey::normalize_account_id(input.account_id)
        .ok_or("account_id must be lowercase 0x plus 64 hexadecimal characters")?;
    let institution_code = primitives::cid::code::institution_code_from_str(input.institution)
        .ok_or("institution must be a registered CID institution code")?;
    let person_level = primitives::cid::code::is_person_code(&institution_code);
    let province_code =
        province_code_by_name(input.province_name).ok_or("province not found in code table")?;
    let city_code = if person_level {
        primitives::cid::generator::RESERVED_PROVINCE_CITY_CODE
    } else {
        city_code_by_name(input.province_name, input.city_name)
            .ok_or("city not found in province code table")?
    };
    let year = Utc::now().format("%Y").to_string();

    primitives::cid::generator::generate_cid_number(
        primitives::cid::generator::GenerateCidNumberInput {
            public_key: account_id.as_str(),
            p1: input.p1,
            province_code,
            province_name: input.province_name,
            city_code,
            city_name: input.city_name,
            year: year.as_str(),
            institution: input.institution,
        },
    )
}

#[cfg(test)]
// CID 生成夹具必须满足固定格式，断言式解包用于让契约偏差立即失败。
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    const ACCOUNT_ID_1: &str = "0x1111111111111111111111111111111111111111111111111111111111111111";
    const ACCOUNT_ID_2: &str = "0x2222222222222222222222222222222222222222222222222222222222222222";

    #[test]
    fn citizen_cid_is_country_prefixed() {
        // 公民(CTZN)去地域化:R5 = CN 国家码 + 号码高 3 位,不再是省码。
        let code = generate_cid_number(GenerateCidInput {
            account_id: ACCOUNT_ID_1,
            p1: "1",
            province_name: "广东省",
            city_name: "荔湾市",
            institution: "CTZN",
        })
        .expect("citizen cid should generate");
        let r5 = code.split('-').next().expect("r5 segment");
        assert_eq!(&r5[0..2], "CN");
        assert!(r5[2..5].chars().all(|c| c.is_ascii_digit()));
    }

    #[test]
    fn public_legal_keeps_real_city_code() {
        let code = generate_cid_number(GenerateCidInput {
            account_id: ACCOUNT_ID_2,
            p1: "0",
            province_name: "广东省",
            city_name: "荔湾市",
            institution: "CGOV",
        })
        .expect("public legal cid should generate");
        assert_eq!(code.split('-').next(), Some("GD001"));
    }

    #[test]
    fn three_char_national_layout_shape() {
        let code = generate_cid_number(GenerateCidInput {
            account_id: ACCOUNT_ID_1,
            p1: "0",
            province_name: "广东省",
            city_name: "荔湾市",
            institution: "NRC",
        })
        .expect("nrc cid should generate");
        let seg2 = code.split('-').nth(1).unwrap();
        assert_eq!(seg2.len(), 5);
        assert_eq!(&seg2[0..3], "NRC");
        assert_eq!(&seg2[3..4], "0");
    }

    #[test]
    fn pmul_has_no_number() {
        let r = generate_cid_number(GenerateCidInput {
            account_id: ACCOUNT_ID_1,
            p1: "0",
            province_name: "广东省",
            city_name: "荔湾市",
            institution: "PMUL",
        });
        assert!(r.is_err());
    }
}
