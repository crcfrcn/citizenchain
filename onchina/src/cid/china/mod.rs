//! onchina CID 中国行政区划真源。
//!
//! 行政区划由随包只读 `cid/china/china.sqlite` 单源承载;
//! 镇下完整地址只用于下游地址选择,不是行政区编码范围。
//! CID 编码只通过本模块查询省、市、镇代码。

pub(crate) mod admin;
pub(crate) mod model;
mod store;

pub(crate) use store::{
    area_display_names, area_name_by_codes, china_sqlite_hash, city_code_by_name,
    province_code_by_name, provinces, town_code_by_name, town_exists, with_china_connection,
};
