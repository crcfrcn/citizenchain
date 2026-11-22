//! 将运行态行政区划、年份和唯一性输入交给链原语生成CID。

use chrono::{Datelike, Utc};

use crate::codes::china::{city_code_by_name, province_code_by_name};

pub struct GenerateCidInput<'a> {
    /// 内部发号种子原文，不是授权账户 ID；账户校验由原身份入口负责。
    pub seed: &'a str,
    /// 盈利输入,仅 Variable(注册协会/智能人)与 InheritParent(非法人组织,传父级)
    /// 策略的机构码读取;固定盈利策略的码忽略本字段。取值 0/1 或 非盈利/盈利。
    pub p1: &'a str,
    pub province_name: &'a str,
    pub city_name: &'a str,
    /// 机构码(3 或 4 字符代码,或机构码中文标签),全仓库机构分类唯一真源。
    pub institution: &'a str,
}

pub fn generate_cid_number(input: GenerateCidInput<'_>) -> Result<String, &'static str> {
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
    let year = u32::try_from(Utc::now().year()).map_err(|_| "UTC year must fit u32")?;

    primitives::cid::generator::generate_cid_number(
        primitives::cid::generator::GenerateCidNumberInput {
            public_key: input.seed,
            p1: input.p1,
            province_code,
            province_name: input.province_name,
            city_code,
            city_name: input.city_name,
            year,
            institution: input.institution,
        },
    )
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
#[path = "../tests/codes_generator.rs"]
mod tests;
