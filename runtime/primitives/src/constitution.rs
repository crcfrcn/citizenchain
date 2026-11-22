//! 分类宪法修改范围，并判定公投与护法审查的通过条件。

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
/// 全链单源:runtime `legislationvote` 结算与节点守卫「核心章公投凭据」背书共用同一口径,
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
/// 全链单源:legislationvote 终审结算与节点守卫「护宪终审凭据」背书共用,禁止两份漂移。
pub const CONSTITUTION_GUARD_APPROVAL_THRESHOLD: u32 = 4;

/// 护宪大法官终审通过口径(第21条):赞成票数 ≥ 4。`approve`=护宪大法官赞成票数。
/// runtime `legislationvote` 结算与节点守卫核心同一口径,禁止两份。
pub fn guard_review_passed(approve: u32) -> bool {
    approve >= CONSTITUTION_GUARD_APPROVAL_THRESHOLD
}
