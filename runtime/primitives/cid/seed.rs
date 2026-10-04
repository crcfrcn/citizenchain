//! CID 确定性种子协议。
//! runtime 只保护字节格式,不做运行态查重。

use alloc::{format, string::String};

/// 公权机构确定性 account seed。
pub fn official_institution_account_seed(
    scope: &str,
    province_code: &str,
    city_code: &str,
    town_code: &str,
    institution_code: &str,
) -> String {
    format!("GOV-{scope}-{province_code}-{city_code}-{town_code}-{institution_code}")
}
