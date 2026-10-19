//! 行政区划运行时模型。

/// 镇行政区划。
///
/// CID 公权机构名录只覆盖到镇目录。镇下面的完整地址保存在
/// `china.sqlite.addresses` 中,不参与公权机构目录和 CID 行政区编码。
#[derive(Debug)]
pub struct TownDivision {
    pub town_name: &'static str,
    pub town_code: &'static str,
}

#[derive(Debug)]
pub struct CityDivision {
    pub city_name: &'static str,
    pub city_code: &'static str,
    pub towns: &'static [TownDivision],
}

#[derive(Debug)]
pub struct ProvinceDivision {
    pub province_name: &'static str,
    pub province_code: &'static str,
    pub cities: &'static [CityDivision],
}
