//! CID 行政区 / 选项元数据 DTO(管理员控制台元信息接口使用)。

use serde::{Deserialize, Serialize};

#[derive(Serialize)]
pub(crate) struct CidInstitutionCodeItem {
    pub(crate) institution_code: &'static str,
    pub(crate) institution_code_label: &'static str,
}

#[derive(Serialize)]
pub(crate) struct CidProvinceItem {
    pub(crate) province_name: String,
    pub(crate) province_code: String,
}

#[derive(Serialize)]
pub(crate) struct CidCityItem {
    pub(crate) city_name: String,
    pub(crate) city_code: String,
}

#[derive(Serialize)]
pub(crate) struct CidTownItem {
    pub(crate) town_name: String,
    pub(crate) town_code: String,
}

#[derive(Serialize)]
pub(crate) struct AdminCidMetaOutput {
    pub(crate) institution_options: Vec<CidInstitutionCodeItem>,
    pub(crate) provinces: Vec<CidProvinceItem>,
    pub(crate) all_provinces: Vec<CidProvinceItem>,
    pub(crate) scoped_province_name: Option<String>,
}

/// 公开机构码标签表(免登录):前端替代原硬编码 INSTITUTION_CODE_LABEL 的单一真源。
#[derive(Serialize)]
pub(crate) struct CidLabelsOutput {
    pub(crate) institution_labels: Vec<CidInstitutionCodeItem>,
}

#[derive(Deserialize)]
pub(crate) struct AdminCidCitiesQuery {
    pub(crate) province_name: String,
}

#[derive(Deserialize)]
pub(crate) struct AdminCidTownsQuery {
    pub(crate) province_name: String,
    pub(crate) city_code: String,
}
