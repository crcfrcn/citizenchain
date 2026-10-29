//! 立法机关代表表决门槛的唯一实现。

use crate::types::RepresentativeVoteRule;

/// 表决期满时按全部有资格成员和已投赞成/反对票判断是否通过。
pub fn representative_final_passed(
    rule: RepresentativeVoteRule,
    total: u32,
    yes: u32,
    no: u32,
) -> bool {
    let casted = yes.saturating_add(no);
    if total == 0 || casted == 0 {
        return false;
    }
    let (total, yes, casted) = (u64::from(total), u64::from(yes), u64::from(casted));
    match rule {
        RepresentativeVoteRule::Regular => casted * 100 > total * 80 && yes * 100 >= casted * 60,
        RepresentativeVoteRule::Major => casted * 100 > total * 90 && yes * 100 >= casted * 70,
        RepresentativeVoteRule::Special => casted == total && yes * 100 >= total * 70,
    }
}

/// 只做数学上绝对安全的提前判定，避免尚有翻盘可能时错误终结提案。
pub fn representative_decided(
    rule: RepresentativeVoteRule,
    total: u32,
    yes: u32,
    no: u32,
) -> Option<bool> {
    let casted = yes.saturating_add(no);
    if total == 0 {
        return Some(false);
    }
    if casted >= total {
        return Some(representative_final_passed(rule, total, yes, no));
    }
    let (total, no) = (u64::from(total), u64::from(no));
    let impossible = match rule {
        RepresentativeVoteRule::Regular => no * 100 > total * 40,
        RepresentativeVoteRule::Major | RepresentativeVoteRule::Special => no * 100 > total * 30,
    };
    impossible.then_some(false)
}
