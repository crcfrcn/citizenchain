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
