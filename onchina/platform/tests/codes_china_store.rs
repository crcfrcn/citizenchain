//! 原所属模块回归，断言与输入保持。
use super::*;

fn first_town_triple() -> (&'static str, &'static str, &'static str) {
    for p in provinces() {
        for c in p.cities {
            if let Some(t) = c.towns.first() {
                return (p.province_code, c.city_code, t.town_code);
            }
        }
    }
    panic!("china.sqlite 至少应有一个镇供测试");
}

#[test]
fn town_exists_true_for_real_triple() {
    let (pc, cc, tc) = first_town_triple();
    assert!(town_exists(pc, cc, tc));
}

#[test]
fn town_exists_false_for_retired_town_code() {
    let (pc, cc, _tc) = first_town_triple();
    // 同省同市但镇 code 不存在(模拟退役/被删镇),应判孤儿。
    assert!(!town_exists(pc, cc, "ZZZ_NOT_A_TOWN"));
}

#[test]
fn town_exists_false_for_unknown_province() {
    assert!(!town_exists("__NOPE__", "001", "001"));
}

#[test]
fn town_exists_true_for_empty_town_code() {
    // 空镇 code = 市级机构/储委会/部委合法态,永远不是孤儿。
    assert!(town_exists("ZS", "001", ""));
    assert!(town_exists("ZS", "", ""));
    assert!(town_exists("anything", "anything", "   "));
}
