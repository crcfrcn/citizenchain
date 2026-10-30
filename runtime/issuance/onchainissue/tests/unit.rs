//! 链上发行模块现有动作标识、词表与输入校验的单元测试。
//! 当前未实现的业务流程不在此伪造成功路径。

mod blacklist {
    //! 字符串黑名单默认词表契约测试。
    //!
    //! 这里只锁定当前已有校验及协议；执行拒绝由完整Runtime安全矩阵覆盖。

    #[test]
    fn default_blacklist_keeps_currency_and_anchor_terms() {
        let words = crate::blacklist::default_blacklist_words();
        assert!(words.iter().any(|word| word.as_slice() == b"usd"));
        assert!(words.iter().any(|word| word.as_slice() == b"stable"));
        assert!(words
            .iter()
            .any(|word| word.as_slice() == "数字人民币".as_bytes()));
    }
}
mod cases {
    //! 业务动作协议常量测试(issue/mint/burn/close/transfer)。
    //!
    //! 这里只锁定当前已有校验及协议；执行拒绝由完整Runtime安全矩阵覆盖。

    #[test]
    fn business_action_codes_are_stable_and_distinct() {
        use crate::proposal::{
            ACTION_ONCHAIN_ASSET_BURN, ACTION_ONCHAIN_ASSET_CLOSE, ACTION_ONCHAIN_ASSET_ISSUE,
            ACTION_ONCHAIN_ASSET_MINT, ACTION_ONCHAIN_ASSET_TRANSFER,
        };

        let actions = [
            ACTION_ONCHAIN_ASSET_ISSUE,
            ACTION_ONCHAIN_ASSET_MINT,
            ACTION_ONCHAIN_ASSET_BURN,
            ACTION_ONCHAIN_ASSET_CLOSE,
            ACTION_ONCHAIN_ASSET_TRANSFER,
        ];
        assert_eq!(crate::MODULE_TAG, b"onc-iss");
        for (index, action) in actions.iter().enumerate() {
            assert!(actions.iter().skip(index + 1).all(|other| other != action));
        }
    }
}
mod monitor {
    //! NRC 监管 5 动作协议常量测试(freeze/unfreeze/confiscate/forceTransfer/forceClose)。
    //!
    //! 这里只锁定当前已有校验及协议；执行拒绝由完整Runtime安全矩阵覆盖。

    #[test]
    fn monitor_action_codes_are_stable_and_distinct() {
        use crate::proposal::{
            ACTION_ONCHAIN_ASSET_MONITOR_CONFISCATE, ACTION_ONCHAIN_ASSET_MONITOR_FORCE_CLOSE,
            ACTION_ONCHAIN_ASSET_MONITOR_FORCE_TRANSFER, ACTION_ONCHAIN_ASSET_MONITOR_FREEZE,
            ACTION_ONCHAIN_ASSET_MONITOR_UNFREEZE,
        };

        let actions = [
            ACTION_ONCHAIN_ASSET_MONITOR_FREEZE,
            ACTION_ONCHAIN_ASSET_MONITOR_UNFREEZE,
            ACTION_ONCHAIN_ASSET_MONITOR_CONFISCATE,
            ACTION_ONCHAIN_ASSET_MONITOR_FORCE_TRANSFER,
            ACTION_ONCHAIN_ASSET_MONITOR_FORCE_CLOSE,
        ];
        for (index, action) in actions.iter().enumerate() {
            assert_eq!(&action[..2], b"OM");
            assert!(actions.iter().skip(index + 1).all(|other| other != action));
        }
    }
}

mod validation {
    use crate::types::AssetClass;

    use crate::validation::*;
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
