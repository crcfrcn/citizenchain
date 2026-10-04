//! 地址库版本常量。

/// 本地地址库语义版本。
///
/// 链上 `address-registry` 的 `catalog_version` 使用同一字符串口径。
/// 发布安装包更换 `china.sqlite` 时同步提升本常量。
pub(crate) const ADDRESS_CATALOG_VERSION: &str = "v1.0.0";
