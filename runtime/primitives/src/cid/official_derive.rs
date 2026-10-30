//! 公权机构全量确定性派生(创世直铸单源,ADR-031 卡3)。
//!
//! 「行政区(`china::area`)× 机构码模板(`official_template`)」纯派生:号由
//! `seed + generator` 确定性生成,名称由模板组装。genesis 落地存储与数量/格式断言
//! 共享本枚举,杜绝逻辑漂移。创世只直铸当前国家/省/市骨架:
//! 数量 = 省部门 11×43 + 市行政区 17×市数,与 china_*.rs 296 常量互不重号。
//! 镇行政区公权机构保留模板,但运行期统一由注册局按镇码注册上链。

use alloc::string::String;

use crate::cid::china::area::{for_each_area, AreaItem};
use crate::cid::generator::{generate_cid_number, GenerateCidNumberInput};
use crate::cid::official_template::{
    OfficialOrgTemplate, CITY_TEMPLATES, PROVINCE_DEPARTMENT_TEMPLATES,
};
use crate::cid::seed::official_institution_account_seed;

/// 创世直铸年份(固定,确定性):与 china_*.rs 常量号年份一致。
pub const GENESIS_INSTITUTION_YEAR: &str = "2026";

/// 用模板 + 行政区确定性派生一个机构号(与 onchina official_institution_cid 同源)。
fn derive_template_cid(
    scope: &str,
    province_code: &str,
    city_code: &str,
    town_code: &str,
    template: &OfficialOrgTemplate,
    province_name: &str,
    city_name: &str,
) -> String {
    let seed = official_institution_account_seed(
        scope,
        province_code,
        city_code,
        town_code,
        template.institution_code,
    );
    generate_cid_number(GenerateCidNumberInput {
        public_key: seed.as_str(),
        p1: "0",
        province_code,
        province_name,
        city_code,
        city_name,
        year: GENESIS_INSTITUTION_YEAR,
        institution: template.institution_code,
    })
    .unwrap_or_else(|e| {
        panic!(
            "genesis template cid 生成失败 code={} scope={scope}: {e}",
            template.institution_code
        )
    })
}

/// 派生机构明细:链上创世与链下(onchina 目录物化/同源校验)共用的完整上下文。
pub struct DerivedInstitutionItem<'a> {
    /// "PROVINCE" | "CITY"(创世枚举的 seed 作用域,与 onchina 一致)。
    pub scope: &'a str,
    pub province_code: &'a str,
    pub city_code: &'a str,
    pub town_code: &'a str,
    pub province_name: &'a str,
    pub city_name: &'a str,
    /// 名称组装用显示名(省名/市名/镇名)。
    pub display_area_name: &'a str,
    pub template: &'static OfficialOrgTemplate,
    pub cid_number: &'a str,
    pub cid_full_name: &'a str,
    pub cid_short_name: &'a str,
}

/// 枚举全部派生公权机构,对每个机构调用 `f(cid_number, cid_full_name, cid_short_name)`。
///
/// 遍历顺序与 `AREA_DATA` 字节序一致,确定性。省行政区部门落省主市、显示名=省名;
/// 市行政区显示名=市名。
pub fn for_each_public_institution<F>(mut f: F)
where
    F: FnMut(&str, &str, &str),
{
    for_each_public_institution_detailed(|item| {
        f(item.cid_number, item.cid_full_name, item.cid_short_name)
    })
}

/// 明细版枚举(创世/onchina 物化/同源校验共用)。
pub fn for_each_public_institution_detailed<F>(mut f: F)
where
    F: for<'a> FnMut(&DerivedInstitutionItem<'a>),
{
    let mut emit = |scope: &str,
                    province_code: &str,
                    city_code: &str,
                    town_code: &str,
                    template: &'static OfficialOrgTemplate,
                    province_name: &str,
                    city_name: &str,
                    display_area_name: &str| {
        let cid = derive_template_cid(
            scope,
            province_code,
            city_code,
            town_code,
            template,
            province_name,
            city_name,
        );
        let full = template.cid_full_name(display_area_name);
        let short = template.cid_short_name(display_area_name);
        f(&DerivedInstitutionItem {
            scope,
            province_code,
            city_code,
            town_code,
            province_name,
            city_name,
            display_area_name,
            template,
            cid_number: cid.as_str(),
            cid_full_name: full.as_str(),
            cid_short_name: short.as_str(),
        });
    };

    for_each_area(|item| match item {
        AreaItem::Province {
            province_code,
            province_name,
            home_city_code,
            home_city_name,
        } => {
            for template in PROVINCE_DEPARTMENT_TEMPLATES {
                emit(
                    "PROVINCE",
                    province_code,
                    home_city_code,
                    "",
                    template,
                    province_name,
                    home_city_name,
                    province_name,
                );
            }
        }
        AreaItem::City(city) => {
            for template in CITY_TEMPLATES {
                emit(
                    "CITY",
                    city.province_code,
                    city.city_code,
                    "",
                    template,
                    city.province_name,
                    city.city_name,
                    city.city_name,
                );
            }
        }
        // 镇行政区仍是行政区真源的一部分,但镇行政区公权机构不再参与创世直铸。
        // 运行期由注册局按 town_code 创建,链上 PublicManage 才是机构唯一真源。
        AreaItem::Town(_) => {}
    });
}

/// 派生机构总数(= 省部门 11×省 + 市行政区 17×市)。
pub fn public_institution_derived_count() -> usize {
    let (provinces, cities, _towns) = crate::cid::china::area::area_counts();
    PROVINCE_DEPARTMENT_TEMPLATES.len() * provinces as usize
        + CITY_TEMPLATES.len() * cities as usize
}
