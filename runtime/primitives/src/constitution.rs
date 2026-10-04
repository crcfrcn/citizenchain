//! 公民宪法修改「章→档位」分类(第十九条落地单源)。
//!
//! 公民宪法第十九条把修宪分三档,本模块只做**纯判定**:给定「本次改动的条号集」
//! 「核心章条号集」「不可修改条款清单」,返回本次改动要求的表决档位。判定与链上
//! 泛型 `T` 解耦、无存储依赖。
//!
//! 两端分工(不是共用同一个函数):
//! - runtime `legislation-yuan` 调用本模块 `classify` 做**条文**档位判定,再与章/节
//!   标题变更的档位取更严一档(见 `Pallet::constitution_amendment_scope`);
//! - 节点守卫 `node/src/core/constitution` **不调用** `classify`,它从 block#0 派生
//!   `ImmutableReference`,以「禁改条逐字冻结」与「核心章非禁改条相对创世的 diff →
//!   要求该版本记录为特别案」两条不变式**背书**已导入区块。
//!
//! 章/节标题不参与本函数:标题没有条号,若折算成所辖条号并入 `changed`,会因命中禁改
//! 清单被误判为 [`AmendmentScope::ImmutableViolation`]。第十九条禁改保护的对象是**条**
//! 本身,改动禁改条所在章节的标题不等于改动该条,故标题变更由 runtime 侧单独按所属章
//! 判档。
//!
//! 判定语义(与第十九条逐字对应):
//!   1. 改动命中不可修改条款(第 1/2/3/17/19/24/34/42 条)→ [`AmendmentScope::ImmutableViolation`](违宪,拒);
//!   2. 改动命中第一章总则核心(非禁改)条款              → [`AmendmentScope::CoreChapter`](必须特别案 Special + 强制公投);
//!   3. 改动仅落第二章及以后的一般条款                    → [`AmendmentScope::GeneralOnly`](必须重要案 Major);
//!   4. 未改动任何条文                                    → [`AmendmentScope::NoChange`](空提案,拒)。
//!
//! 不可修改条款清单单源见 [`crate::count_const::IMMUTABLE_CONSTITUTION_ARTICLES`]。

/// 核心章在「章序列」中的下标。第十九条:第一章总则为宪法核心,故核心章恒为首章。
/// runtime 与节点守卫共用此常量,禁止两端各写字面 `0`。
pub const CONSTITUTION_CORE_CHAPTER_INDEX: usize = 0;

/// 一次修宪改动的范围判定结果,决定它**要求**的表决档位。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AmendmentScope {
    /// 未改动任何条文(空提案),应拒绝。
    NoChange,
    /// 触碰了不可修改条款(违反第十九条禁改清单),应拒绝。
    ImmutableViolation,
    /// 触碰了第一章总则的核心(非禁改)条款 → 要求特别案 Special + 强制公投。
    CoreChapter,
    /// 仅触碰第二章及以后的一般条款 → 要求重要案 Major。
    GeneralOnly,
}

/// 纯判定:由「变更条号」「核心章条号」「不可修改清单」决定本次改动落在哪一档。
///
/// - `changed`:新旧宪法逐条 diff 出的变更条号(增/删/改任一即变更),顺序不限、可含重复。
/// - `core`:第一章总则(核心章)的全部条号;禁改条虽在核心章内,但判定时禁改优先归入
///   [`AmendmentScope::ImmutableViolation`],故调用方无需预先剔除。
/// - `immutable`:不可修改条款清单(第十九条禁改的 8 条)。
///
/// 判定优先级:禁改 > 核心章 > 一般章,确保同批改动里只要触及更严的一档即按更严处理。
pub fn classify(changed: &[u32], core: &[u32], immutable: &[u32]) -> AmendmentScope {
    if changed.is_empty() {
        return AmendmentScope::NoChange;
    }
    if changed.iter().any(|n| immutable.contains(n)) {
        return AmendmentScope::ImmutableViolation;
    }
    if changed.iter().any(|n| core.contains(n)) {
        return AmendmentScope::CoreChapter;
    }
    AmendmentScope::GeneralOnly
}

/// 立法公投通过口径(公民宪法:≥70% 参与 + ≥70% 赞成,赞成率以参与者为基数)。
///
/// 全链单源:runtime `legislation-vote` 结算与节点守卫「核心章公投凭据」背书共用同一口径,
/// 禁止两份漂移(节点侧 `check_core_chapter_tier` 的 follow-up 直接调本函数)。
/// `eligible`=公投选民总数,`yes`/`no`=赞成/反对票数。分母为 0 或零参与 → 不通过(fail-closed)。
pub fn referendum_passed(eligible: u64, yes: u64, no: u64) -> bool {
    let casted = yes.saturating_add(no);
    if eligible == 0 || casted == 0 {
        return false;
    }
    casted.saturating_mul(100) >= eligible.saturating_mul(70)
        && yes.saturating_mul(100) >= casted.saturating_mul(70)
}

/// 护宪大法官终审通过阈值(公民宪法第21条:7 名护宪大法官中 4 名及以上赞成)。
/// 全链单源:legislation-vote 终审结算与节点守卫「护宪终审凭据」背书共用,禁止两份漂移。
pub const CONSTITUTION_GUARD_APPROVAL_THRESHOLD: u32 = 4;

/// 护宪大法官终审通过口径(第21条):赞成票数 ≥ 4。`approve`=护宪大法官赞成票数。
/// runtime `legislation-vote` 结算与节点守卫核心同一口径,禁止两份。
pub fn guard_review_passed(approve: u32) -> bool {
    approve >= CONSTITUTION_GUARD_APPROVAL_THRESHOLD
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::count_const::IMMUTABLE_CONSTITUTION_ARTICLES;

    // 第一章总则 = 第 1..=52 条(其中 8 条为禁改);第二章自第 53 条起。
    const CORE_CHAPTER: [u32; 5] = [4, 5, 6, 18, 20];

    #[test]
    fn empty_change_is_no_change() {
        assert_eq!(
            classify(&[], &CORE_CHAPTER, &IMMUTABLE_CONSTITUTION_ARTICLES),
            AmendmentScope::NoChange
        );
    }

    #[test]
    fn touching_immutable_article_is_violation() {
        // 第 19 条本身是禁改条款。
        assert_eq!(
            classify(&[19], &CORE_CHAPTER, &IMMUTABLE_CONSTITUTION_ARTICLES),
            AmendmentScope::ImmutableViolation
        );
    }

    #[test]
    fn touching_core_chapter_requires_special() {
        // 第 5 条属第一章核心且非禁改。
        assert_eq!(
            classify(&[5], &CORE_CHAPTER, &IMMUTABLE_CONSTITUTION_ARTICLES),
            AmendmentScope::CoreChapter
        );
    }

    #[test]
    fn touching_general_chapter_requires_major() {
        // 第 60 条属第二章一般条款。
        assert_eq!(
            classify(&[60], &CORE_CHAPTER, &IMMUTABLE_CONSTITUTION_ARTICLES),
            AmendmentScope::GeneralOnly
        );
    }

    #[test]
    fn immutable_takes_priority_over_core_and_general() {
        // 同批既动核心章第 5 条又动禁改第 1 条 → 禁改优先,整体判违宪。
        assert_eq!(
            classify(&[5, 60, 1], &CORE_CHAPTER, &IMMUTABLE_CONSTITUTION_ARTICLES),
            AmendmentScope::ImmutableViolation
        );
    }

    #[test]
    fn core_takes_priority_over_general() {
        // 同批既动一般章第 60 条又动核心章第 6 条 → 核心优先,要求特别案。
        assert_eq!(
            classify(&[60, 6], &CORE_CHAPTER, &IMMUTABLE_CONSTITUTION_ARTICLES),
            AmendmentScope::CoreChapter
        );
    }

    #[test]
    fn referendum_passes_at_seventy_seventy() {
        // 100 选民,85 参与(≥70%),80 赞成(80/85≥70%)→ 通过。
        assert!(referendum_passed(100, 80, 5));
        // 边界:恰 70% 参与 + 恰 70% 赞成。
        assert!(referendum_passed(100, 70, 0));
        assert!(referendum_passed(10, 7, 3)); // 参与 100%,赞成 7/10=70%
    }

    #[test]
    fn referendum_fails_below_threshold() {
        assert!(!referendum_passed(100, 60, 5)); // 参与 65% <70%
        assert!(!referendum_passed(100, 40, 40)); // 参与 80% 但赞成 40/80=50% <70%
        assert!(!referendum_passed(0, 0, 0)); // 无选民 fail-closed
        assert!(!referendum_passed(100, 0, 0)); // 零参与 fail-closed
    }

    #[test]
    fn guard_review_needs_four_approvals() {
        assert!(guard_review_passed(4)); // 恰 4/7
        assert!(guard_review_passed(7)); // 全票
        assert!(!guard_review_passed(3)); // 3/7 不足
        assert!(!guard_review_passed(0));
    }
}
