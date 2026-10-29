//! 所属原语的真实单元测试，保留原断言与固定输入。
mod constitution {

    use crate::constitution::*;
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
