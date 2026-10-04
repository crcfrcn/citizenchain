//! GMB 机构账户地址派生(后端薄适配)。
//!
//! 账户派生唯一真源 = 链端 `primitives::account_derive`:op_tag、
//! 5 个受限保留名、name→种类路由、payload 字段拼装、唯一派生入口 `AccountKind::derive`
//! 全部收敛在那里。本模块仅做 `&str` → `&[u8]`
//! 适配 + hex 编码,用于:
//!   - 创建账户时立即在本地算出规范 `account_id`，无需等激活上链
//!   - 激活成功后做 receipt 地址 ↔ 本地派生值的一致性断言,抓链端路由 / domain 漂移
//!
//! ## GMB 协议(定义见 `primitives::account_derive`)
//!
//! | op_tag | 账户类型            | account_name              | preimage 是否含 account_name |
//! |--------|---------------------|---------------------------|------------------------------|
//! | 0x00   | 主账户              | `"主账户"`(UTF-8 中文)   | 否                           |
//! | 0x01   | 费用账户            | `"费用账户"`(UTF-8 中文) | 否                           |
//! | 0x02   | 永久质押            | `"永久质押"`(UTF-8 中文) | 否                           |
//! | 0x03   | 安全基金            | `"安全基金"`(UTF-8 中文) | 否                           |
//! | 0x04   | 两和基金            | `"两和基金"`(UTF-8 中文) | 否                           |
//! | 0x06   | 清算账户            | `"清算账户"`(UTF-8 中文) | 否                           |
//! | 0x07   | 用户自定义其他账户  | 用户输入的任意非空字符串 | 是                           |
//!
//! ```text
//! preimage = b"GMB"                          // 3 字节 domain
//!         || op_tag                          // 1 字节 (0x00 / 0x01 / 0x02 / 0x03 / 0x04 / 0x06 / 0x07)
//!         || ss58.to_le_bytes()              // 2 字节 (2027 LE = [0xEB, 0x07])
//!         || cid_number.as_bytes()          // 变长
//!         || account_name.as_bytes()         // 仅 0x07 追加;0x00 / 0x01 / 0x06 不追加
//! account_id = 0x || hex(blake2b_256(preimage))
//! ```

use primitives::account_derive::{self, RESERVED_ACCOUNT_NAMES as RESERVED_ACCOUNT_NAME_BYTES};
use primitives::core_const::SS58_FORMAT;

/// 全部 6 个受限保留账户名(单一源 = 链端 `account_derive`,UTF-8 字符串视图)。
///
/// 自定义账户判定:account_name 命中其一即非自定义(走各自 op_tag),否则为
/// `OP_NAME` 自定义命名账户。CitizenApp BFF 据此过滤 custom_account_names。
pub(crate) fn reserved_account_names() -> [String; 6] {
    [
        String::from_utf8_lossy(RESERVED_ACCOUNT_NAME_BYTES[0]).into_owned(),
        String::from_utf8_lossy(RESERVED_ACCOUNT_NAME_BYTES[1]).into_owned(),
        String::from_utf8_lossy(RESERVED_ACCOUNT_NAME_BYTES[2]).into_owned(),
        String::from_utf8_lossy(RESERVED_ACCOUNT_NAME_BYTES[3]).into_owned(),
        String::from_utf8_lossy(RESERVED_ACCOUNT_NAME_BYTES[4]).into_owned(),
        String::from_utf8_lossy(RESERVED_ACCOUNT_NAME_BYTES[5]).into_owned(),
    ]
}

/// 按 `account_name` 路由并派生机构账户地址的 32 字节 `AccountId`(唯一真源同上)。
///
/// 返回 `None` 当 `account_name` 为空串(与链端 `EmptyAccountName` 对齐)。
/// 关闭账户提案需要账户与主账户的裸 32 字节 AccountId(SCALE 无长度前缀),用本函数取。
pub(crate) fn derive_account_bytes(cid_number: &str, account_name: &str) -> Option<[u8; 32]> {
    account_derive::institution_kind_by_name(cid_number.as_bytes(), account_name.as_bytes())
        .map(|kind| kind.derive(SS58_FORMAT))
}
