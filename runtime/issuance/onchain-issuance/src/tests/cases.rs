//! 业务动作协议常量测试(issue/mint/burn/close/transfer)。
//!
//! 实装步骤(后续任务卡 A):
//! 1. 在本文件 `mod mock` 内构造 mock runtime,挂 pallet_balances + pallet_assets + onchain_issuance + votingengine
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
