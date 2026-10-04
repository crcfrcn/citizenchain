//! 字符串黑名单 storage + 默认词表(ADR-011 第 5.3 节)。
//!
//! 命中任意词的 name / symbol / description 字段一律 reject。
//! 黑名单走 storage,GenesisConfig 注入默认词表,后续添词/删词必须走
//! `runtime_upgrade::propose_runtime_upgrade` 联合投票升级 wasm,**不可 sudo**。

use sp_std::vec;
use sp_std::vec::Vec;

/// GenesisConfig 注入的默认黑名单词表。
///
/// 四类违禁词(法币 / 锚定 / 权威 / 数字货币),小写规范化,
/// `validation::contains_blacklisted_word` 在比较前同步小写化字段值。
/// 中英文混合,英文一律小写;中文不分大小写直接命中。
pub fn default_blacklist_words() -> Vec<Vec<u8>> {
    vec![
        // 法币词
        b"\xe5\x85\x83".to_vec(), // 元
        b"rmb".to_vec(),
        b"cny".to_vec(),
        b"\xc2\xa5".to_vec(),                             // ¥
        b"\xe4\xba\xba\xe6\xb0\x91\xe5\xb8\x81".to_vec(), // 人民币
        b"$".to_vec(),
        b"usd".to_vec(),
        b"\xe7\xbe\x8e\xe5\x85\x83".to_vec(), // 美元
        b"\xe6\xac\xa7\xe5\x85\x83".to_vec(), // 欧元
        b"\xe6\x97\xa5\xe5\x85\x83".to_vec(), // 日元
        b"\xe6\xb8\xaf\xe5\xb8\x81".to_vec(), // 港币
        b"hkd".to_vec(),
        // 锚定词
        b"\xe9\x94\x9a\xe5\xae\x9a".to_vec(), // 锚定
        b"\xe7\xa8\xb3\xe5\xae\x9a".to_vec(), // 稳定
        b"stable".to_vec(),
        b"peg".to_vec(),
        b"\xe5\xaf\xb9\xe6\xa0\x87".to_vec(), // 对标
        b"\xe7\xad\x89\xe5\x80\xbc".to_vec(), // 等值
        b"1:1".to_vec(),
        // 权威词
        b"\xe5\xa4\xae\xe8\xa1\x8c".to_vec(), // 央行
        b"\xe5\x9b\xbd\xe5\xae\xb6".to_vec(), // 国家
        b"\xe5\xae\x98\xe6\x96\xb9".to_vec(), // 官方
        b"official".to_vec(),
        b"authorized".to_vec(),
        b"\xe7\x9b\x91\xe7\xae\xa1".to_vec(), // 监管
        // 数字货币词
        b"\xe6\x95\xb0\xe5\xad\x97\xe4\xba\xba\xe6\xb0\x91\xe5\xb8\x81".to_vec(), // 数字人民币
        b"dcep".to_vec(),
        b"e-cny".to_vec(),
        b"cbdc".to_vec(),
    ]
}
