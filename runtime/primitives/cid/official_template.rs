//! 公权机构命名与机构集模板(全仓单源,ADR-031 卡3)。
//!
//! 用户统一命名规则后，把确定性模板从 OnChina 收归
//! primitives 单源,创世直铸与 onchina 目录共享,杜绝两处漂移。
//!
//! 组装规则(296 常量逆向验证零例外):
//!   cid_short_name = 行政区显示名 + `cid_short_name_suffix`
//!   cid_full_name = 行政区显示名 + `cid_full_name_suffix`
//! 显示名:省行政区部门=省名,市行政区=市名,镇行政区=镇名。
//! 创世只消费省/市模板;镇行政区模板只供注册局运行期按 town_code 注册上链。

use alloc::format;
use alloc::string::String;

/// 一个机构模板:机构码 + 简称后缀 + 全称后缀。
pub struct OfficialOrgTemplate {
    /// 机构码模板,用于 CID 生成和链上机构码写入。
    pub institution_code: &'static str,
    /// 生成最终 `cid_short_name` 的名称后缀,不是完整机构简称。
    pub cid_short_name_suffix: &'static str,
    /// 生成最终 `cid_full_name` 的名称后缀,不是完整机构全称。
    pub cid_full_name_suffix: &'static str,
}

impl OfficialOrgTemplate {
    /// 生成最终机构简称:`显示名 + cid_short_name_suffix`。
    pub fn cid_short_name(&self, display_area_name: &str) -> String {
        format!("{display_area_name}{}", self.cid_short_name_suffix)
    }

    /// 生成最终机构全称:`显示名 + cid_full_name_suffix`。
    pub fn cid_full_name(&self, display_area_name: &str) -> String {
        format!("{display_area_name}{}", self.cid_full_name_suffix)
    }
}

/// 省行政区部门模板(11 类;省核心治理 6 类为 china_*.rs 常量,不在此)。
pub const PROVINCE_DEPARTMENT_TEMPLATES: &[OfficialOrgTemplate] = &[
    OfficialOrgTemplate {
        institution_code: "PDF",
        cid_short_name_suffix: "国防厅",
        cid_full_name_suffix: "国家防务厅",
    },
    OfficialOrgTemplate {
        institution_code: "PHS",
        cid_short_name_suffix: "国安厅",
        cid_full_name_suffix: "国土安全厅",
    },
    OfficialOrgTemplate {
        institution_code: "PCW",
        cid_short_name_suffix: "民生厅",
        cid_full_name_suffix: "公民生活保障厅",
    },
    OfficialOrgTemplate {
        institution_code: "PHU",
        cid_short_name_suffix: "住建厅",
        cid_full_name_suffix: "住房与城镇建设厅",
    },
    OfficialOrgTemplate {
        institution_code: "PAG",
        cid_short_name_suffix: "农业厅",
        cid_full_name_suffix: "农业与农村发展厅",
    },
    OfficialOrgTemplate {
        institution_code: "PCM",
        cid_short_name_suffix: "商贸厅",
        cid_full_name_suffix: "商务与市场贸易厅",
    },
    OfficialOrgTemplate {
        institution_code: "PFT",
        cid_short_name_suffix: "财税厅",
        cid_full_name_suffix: "财政与税务厅",
    },
    OfficialOrgTemplate {
        institution_code: "PEN",
        cid_short_name_suffix: "能源厅",
        cid_full_name_suffix: "能源与环保发展厅",
    },
    OfficialOrgTemplate {
        institution_code: "PTR",
        cid_short_name_suffix: "交通厅",
        cid_full_name_suffix: "交通运输厅",
    },
    OfficialOrgTemplate {
        institution_code: "PSN",
        cid_short_name_suffix: "参议会",
        cid_full_name_suffix: "联邦立法院参议会",
    },
    OfficialOrgTemplate {
        institution_code: "PRP",
        cid_short_name_suffix: "众议会",
        cid_full_name_suffix: "联邦立法院众议会",
    },
];

/// 市行政区机构模板(C 族 17 类,全)。
pub const CITY_TEMPLATES: &[OfficialOrgTemplate] = &[
    OfficialOrgTemplate {
        institution_code: "CGOV",
        cid_short_name_suffix: "政府",
        cid_full_name_suffix: "自治政府",
    },
    OfficialOrgTemplate {
        institution_code: "CLEG",
        cid_short_name_suffix: "立法会",
        cid_full_name_suffix: "公民立法委员会",
    },
    OfficialOrgTemplate {
        institution_code: "CSUP",
        cid_short_name_suffix: "监察院",
        cid_full_name_suffix: "自治监察院",
    },
    OfficialOrgTemplate {
        institution_code: "CJUD",
        cid_short_name_suffix: "司法院",
        cid_full_name_suffix: "自治司法院",
    },
    OfficialOrgTemplate {
        institution_code: "CEDU",
        cid_short_name_suffix: "教委会",
        cid_full_name_suffix: "公民教育委员会",
    },
    OfficialOrgTemplate {
        institution_code: "CSLF",
        cid_short_name_suffix: "自治会",
        cid_full_name_suffix: "公民自治委员会",
    },
    OfficialOrgTemplate {
        institution_code: "CDEF",
        cid_short_name_suffix: "国防局",
        cid_full_name_suffix: "国家防务局",
    },
    OfficialOrgTemplate {
        institution_code: "CHSC",
        cid_short_name_suffix: "国安局",
        cid_full_name_suffix: "国土安全局",
    },
    OfficialOrgTemplate {
        institution_code: "CPOL",
        cid_short_name_suffix: "公安局",
        cid_full_name_suffix: "公民安全局",
    },
    OfficialOrgTemplate {
        institution_code: "CCWF",
        cid_short_name_suffix: "民生局",
        cid_full_name_suffix: "公民生活保障局",
    },
    OfficialOrgTemplate {
        institution_code: "CHUD",
        cid_short_name_suffix: "住建局",
        cid_full_name_suffix: "住房与城镇建设局",
    },
    OfficialOrgTemplate {
        institution_code: "CAGR",
        cid_short_name_suffix: "农业局",
        cid_full_name_suffix: "农业与农村发展局",
    },
    OfficialOrgTemplate {
        institution_code: "CCOM",
        cid_short_name_suffix: "商贸局",
        cid_full_name_suffix: "商务与市场贸易局",
    },
    OfficialOrgTemplate {
        institution_code: "CFIN",
        cid_short_name_suffix: "财税局",
        cid_full_name_suffix: "财政与税务局",
    },
    OfficialOrgTemplate {
        institution_code: "CENR",
        cid_short_name_suffix: "能源局",
        cid_full_name_suffix: "能源与环保发展局",
    },
    OfficialOrgTemplate {
        institution_code: "CTRN",
        cid_short_name_suffix: "交通局",
        cid_full_name_suffix: "交通运输局",
    },
    OfficialOrgTemplate {
        institution_code: "CREG",
        cid_short_name_suffix: "注册局",
        cid_full_name_suffix: "身份注册局",
    },
];

/// 镇行政区机构模板(D 族 14 类,全;镇无立法/教委,制度设计)。
///
/// 本模板不参与创世直铸。镇行政区公权机构由市注册局在运行期选择
/// `province_code/city_code/town_code` 后注册上链,链上机构记录才是唯一真源。
pub const TOWN_TEMPLATES: &[OfficialOrgTemplate] = &[
    OfficialOrgTemplate {
        institution_code: "TGOV",
        cid_short_name_suffix: "政府",
        cid_full_name_suffix: "自治政府",
    },
    OfficialOrgTemplate {
        institution_code: "TCWF",
        cid_short_name_suffix: "民生科",
        cid_full_name_suffix: "公民生活保障科",
    },
    OfficialOrgTemplate {
        institution_code: "THUD",
        cid_short_name_suffix: "住建科",
        cid_full_name_suffix: "住房与城镇建设科",
    },
    OfficialOrgTemplate {
        institution_code: "TAGR",
        cid_short_name_suffix: "农业科",
        cid_full_name_suffix: "农业与农村发展科",
    },
    OfficialOrgTemplate {
        institution_code: "TFIN",
        cid_short_name_suffix: "财税科",
        cid_full_name_suffix: "财政与税务科",
    },
    OfficialOrgTemplate {
        institution_code: "TDEF",
        cid_short_name_suffix: "国防科",
        cid_full_name_suffix: "国家防务科",
    },
    OfficialOrgTemplate {
        institution_code: "THSC",
        cid_short_name_suffix: "国安科",
        cid_full_name_suffix: "国土安全科",
    },
    OfficialOrgTemplate {
        institution_code: "TCOM",
        cid_short_name_suffix: "商贸科",
        cid_full_name_suffix: "商务与市场贸易科",
    },
    OfficialOrgTemplate {
        institution_code: "TENR",
        cid_short_name_suffix: "能源科",
        cid_full_name_suffix: "能源与环保发展科",
    },
    OfficialOrgTemplate {
        institution_code: "TTRN",
        cid_short_name_suffix: "交通科",
        cid_full_name_suffix: "交通运输科",
    },
    OfficialOrgTemplate {
        institution_code: "TPOL",
        cid_short_name_suffix: "公安科",
        cid_full_name_suffix: "公民安全科",
    },
    OfficialOrgTemplate {
        institution_code: "TSLF",
        cid_short_name_suffix: "自治会",
        cid_full_name_suffix: "公民自治委员会",
    },
    OfficialOrgTemplate {
        institution_code: "TSUP",
        cid_short_name_suffix: "监察院",
        cid_full_name_suffix: "自治监察院",
    },
    OfficialOrgTemplate {
        institution_code: "TJUD",
        cid_short_name_suffix: "司法院",
        cid_full_name_suffix: "自治司法院",
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn template_family_counts() {
        assert_eq!(PROVINCE_DEPARTMENT_TEMPLATES.len(), 11);
        assert_eq!(CITY_TEMPLATES.len(), 17);
        assert_eq!(TOWN_TEMPLATES.len(), 14);
    }

    #[test]
    fn name_composition_matches_reverse_verified_examples() {
        let gov = &CITY_TEMPLATES[0];
        assert_eq!(gov.cid_short_name("荔湾市"), "荔湾市政府");
        assert_eq!(gov.cid_full_name("荔湾市"), "荔湾市自治政府");
        let dept = &PROVINCE_DEPARTMENT_TEMPLATES[0];
        assert_eq!(dept.cid_short_name("广东省"), "广东省国防厅");
        let town = &TOWN_TEMPLATES[0];
        assert_eq!(town.cid_short_name("锦程镇"), "锦程镇政府");
    }
}
