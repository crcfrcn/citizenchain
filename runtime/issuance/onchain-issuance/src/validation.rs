//! 入参校验工具。
//!
//! 三大校验入口:
//! - `ensure_institution_context` — 机构 CID 与资产执行账户都必须存在
//! - `ensure_decimals_in_range` — decimals 必须落在 [0, 18]
//! - `contains_blacklisted_word` — name / symbol / description 字段不可命中黑名单
//! - `ensure_class_supported` — 第一期只支持 Plain,Pegged 直接 reject

use crate::types::AssetClass;
use sp_std::vec::Vec;

/// decimals 范围铁律:`0..=18`(与 ERC-20 主流上限对齐,与 GMB 8 位兼容)。
pub const MIN_DECIMALS: u8 = 0;
pub const MAX_DECIMALS: u8 = 18;

/// 校验发行机构 CID 与资产执行账户。
///
/// 具体“CID 是否已注册、执行账户是否属于 CID、发起人是否在 CID 的 admins 中”
/// 由 pallet 通过 entity/admins 唯一真源完成；本函数只做空值拒绝。
pub fn ensure_institution_context<AccountId: codec::Encode>(
    actor_cid_number: &[u8],
    execution_account_id: &AccountId,
) -> Result<(), &'static str> {
    if actor_cid_number.is_empty() || execution_account_id.encode().is_empty() {
        Err("institution_context_not_allowed")
    } else {
        Ok(())
    }
}

/// 校验 decimals 在合法区间。
pub fn ensure_decimals_in_range(decimals: u8) -> Result<(), &'static str> {
    if (MIN_DECIMALS..=MAX_DECIMALS).contains(&decimals) {
        Ok(())
    } else {
        Err("decimals_out_of_range")
    }
}

/// 校验资产 class 是否被第一期支持。
///
/// Pegged 协议位预留,当前一律 reject,避免锚定语义滑入。
pub fn ensure_class_supported(class: &AssetClass) -> Result<(), &'static str> {
    match class {
        AssetClass::Plain => Ok(()),
        AssetClass::Pegged => Err("unsupported_asset_class"),
    }
}

/// 检查字段是否命中黑名单(忽略大小写,中文直接字节匹配)。
///
/// 输入字段先全部 ASCII 小写化,再与黑名单逐词 substring 匹配。
/// 中文 UTF-8 字节固定,直接匹配。本函数是 O(N×M) 朴素扫描,
/// 词表规模小(<256 条)+ 字段长度小(<256 字节)下足够,无需 Aho-Corasick。
pub fn contains_blacklisted_word(field: &[u8], blacklist: &[Vec<u8>]) -> bool {
    let lowered: Vec<u8> = field
        .iter()
        .map(|&b| if b.is_ascii_uppercase() { b + 32 } else { b })
        .collect();
    blacklist.iter().any(|word| {
        if word.is_empty() || word.len() > lowered.len() {
            return false;
        }
        lowered.windows(word.len()).any(|w| w == word.as_slice())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use codec::Encode;

    #[test]
    fn institution_context_requires_cid_and_execution_account() {
        let acc: [u8; 32] = [0x77; 32];
        assert!(ensure_institution_context(b"CID", &acc).is_ok());
        assert!(ensure_institution_context(b"", &acc).is_err());
        assert!(!acc.encode().is_empty());
    }

    #[test]
    fn decimals_boundary() {
        assert!(ensure_decimals_in_range(0).is_ok());
        assert!(ensure_decimals_in_range(8).is_ok());
        assert!(ensure_decimals_in_range(18).is_ok());
        assert!(ensure_decimals_in_range(19).is_err());
        assert!(ensure_decimals_in_range(255).is_err());
    }

    #[test]
    fn class_only_plain() {
        assert!(ensure_class_supported(&AssetClass::Plain).is_ok());
        assert!(ensure_class_supported(&AssetClass::Pegged).is_err());
    }

    #[test]
    fn blacklist_hits_case_insensitive_ascii() {
        let blacklist = vec![b"usd".to_vec()];
        assert!(contains_blacklisted_word(b"usd-token", &blacklist));
        assert!(contains_blacklisted_word(b"USD-Token", &blacklist));
        assert!(contains_blacklisted_word(b"Some USD here", &blacklist));
        assert!(!contains_blacklisted_word(b"safecoin", &blacklist));
    }

    #[test]
    fn blacklist_hits_chinese_bytes() {
        // 「人民币」UTF-8 字节序
        let rmb_bytes = b"\xe4\xba\xba\xe6\xb0\x91\xe5\xb8\x81".to_vec();
        let blacklist = vec![rmb_bytes];
        // 「数字人民币」字段命中「人民币」
        let field = b"\xe6\x95\xb0\xe5\xad\x97\xe4\xba\xba\xe6\xb0\x91\xe5\xb8\x81";
        assert!(contains_blacklisted_word(field, &blacklist));
    }
}
