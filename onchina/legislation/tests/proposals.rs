//! 验证立法提案分类的接口文本及索引编码。

use super::*;

#[test]
fn proposal_category_serializes_to_snake_case() {
    // 前端 ProposalCategory 联合类型为 'law' | 'personnel' | 'budget',必须逐字一致。
    assert_eq!(
        serde_json::to_string(&ProposalCategory::Law).unwrap(),
        "\"law\""
    );
    assert_eq!(
        serde_json::to_string(&ProposalCategory::Personnel).unwrap(),
        "\"personnel\""
    );
    assert_eq!(
        serde_json::to_string(&ProposalCategory::Budget).unwrap(),
        "\"budget\""
    );
}

#[test]
fn proposal_category_index_matches_doc() {
    assert_eq!(ProposalCategory::Law.as_u8(), 0);
    assert_eq!(ProposalCategory::Personnel.as_u8(), 1);
    assert_eq!(ProposalCategory::Budget.as_u8(), 2);
}
