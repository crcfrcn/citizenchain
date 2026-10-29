//! 链上发行模块现有动作标识、词表与输入校验的单元测试。
//! 当前未实现的业务流程不在此伪造成功路径。

mod blacklist {
    //! 字符串黑名单默认词表契约测试。
    //!
    //! 实装步骤(后续任务卡 A):
    //! 1. mock runtime 注入 GenesisConfig 的 default_blacklist_words
    //! 2. 构造命中字段(USD-Token / 锚定积分 / 数字人民币 等)发起 issue,断言 BlacklistedWord error
    //! 3. 构造干净字段(SafeCoin / 校园积分 等)发起 issue,断言 ok
    //! 4. 单独测试 RuntimeUpgrade 路径添词/删词后的行为变化
    //!
    //! 基础校验逻辑已在 `validation::tests::blacklist_*` 单元测试覆盖,
    //! 本文件先锁定创世默认词表的关键中英文条目；完整 extrinsic 场景由后续 mock runtime 测试覆盖。

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
    //! 实装步骤(后续任务卡 A):
    //! 1. 在本文件 `mod mock` 内构造 mock runtime,挂 pallet_balances + pallet_assets + onchainissue + votingengine
    //! 2. 用 `crate::execution::execute_issue` 走完整发行路径,断言 storage / event / 机构费用账户扣款
    //! 3. 覆盖 happy path + 失败分支（CID/执行账户上下文无效、decimals 越界、字段命中黑名单、余额不足）

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
    //! 实装步骤(后续任务卡 B):
    //! 1. mock runtime 中预置 NRC CID、监管管理员、用户代币与持币账户
    //! 2. 走 JointVote 通过路径 → callback → execute_monitor_*
    //! 3. 断言 storage / event / 持仓变化 / 30 天封禁倒计时
    //! 4. 失败分支:非 NRC 主体调用 reject、reason_hash 缺失 reject

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
