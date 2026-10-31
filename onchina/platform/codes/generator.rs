//! onchina CID 号生成适配层。
//!
//!
//! CID 号码字节协议在 `primitives::cid::generator`。本文件只负责 onchina 运行态输入:
//! 从 SQLite 行政区真源解析省/市代码、读取当前年份,再调用 runtime primitives 纯协议。

use chrono::Utc;

use crate::codes::china::{city_code_by_name, province_code_by_name};

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
#[allow(clippy::expect_used, clippy::unwrap_used)]
#[path = "../tests/codes_generator.rs"]
mod tests;
