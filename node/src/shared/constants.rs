// 全链共享常量，避免各模块重复定义导致一致性风险。
pub const SS58_PREFIX: u16 = 2027;
pub const EXPECTED_SS58_PREFIX: u64 = 2027;

/// 治理签名/机构查询等小响应上限（1 MB）。
pub const RPC_RESPONSE_LIMIT_SMALL: u64 = 1024 * 1024;
/// 挖矿统计/网络概览/设置等大响应上限（4 MB）。
pub const RPC_RESPONSE_LIMIT_LARGE: u64 = 4 * 1024 * 1024;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ss58_prefix_values_consistent() {
        assert_eq!(SS58_PREFIX as u64, EXPECTED_SS58_PREFIX);
    }

    #[test]
    fn ss58_prefix_is_2027() {
        assert_eq!(SS58_PREFIX, 2027);
    }
}
