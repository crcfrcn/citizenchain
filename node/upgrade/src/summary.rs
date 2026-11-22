//! 生成运行时升级原因及代码信息摘要。

use crate::types::RuntimeUpgradeDetail;

use nodeclient::codec::truncate_chars;

pub fn format_runtime_upgrade_summary(d: &RuntimeUpgradeDetail) -> String {
    let reason_short = truncate_chars(&d.reason, 50);
    format!("协议升级：{reason_short}")
}
