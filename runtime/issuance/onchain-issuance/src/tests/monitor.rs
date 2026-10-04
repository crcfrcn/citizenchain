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
