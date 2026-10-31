//! 原所属模块回归，断言与输入保持。
use super::*;

fn sample_plan() -> BudgetPlan {
    BudgetPlan {
        budget_entity_code: "NGV".to_string(),
        fiscal_year: 2027,
        categories: vec![BudgetClass {
            code: "1".to_string(),
            name: "一般公共服务".to_string(),
            sections: vec![BudgetSection {
                code: "01".to_string(),
                name: "人大事务".to_string(),
                items: vec![BudgetItem {
                    code: "0101".to_string(),
                    name: "行政运行".to_string(),
                    subitems: vec![BudgetSubitem {
                        code: "010101".to_string(),
                        name: "基本支出".to_string(),
                        revenue: 0,
                        // 100 万亿元 = 10^16 分,超 JS 安全整数 2^53(≈9.007×10^15),
                        // 裸数字会在 JS 侧丢精度,必须字符串承载——用超阈值取样才真验证。
                        expenditure: 10_000_000_000_000_000,
                    }],
                }],
            }],
        }],
        total_revenue: 0,
        total_expenditure: 10_000_000_000_000_000,
    }
}

/// 预算金额以**字符串**出线(防 JS Number 精度丢失),结构为 camelCase。
#[test]
fn budget_amounts_serialize_as_strings() {
    let json = serde_json::to_string(&sample_plan()).expect("serialize plan");
    // 金额必须是带引号的字符串,而非裸数字(取样值超 2^53,裸数字会丢精度)。
    assert!(json.contains("\"totalExpenditure\":\"10000000000000000\""));
    assert!(json.contains("\"expenditure\":\"10000000000000000\""));
    assert!(json.contains("\"totalRevenue\":\"0\""));
    assert!(json.contains("\"budgetEntityCode\":\"NGV\""));
    assert!(json.contains("\"fiscalYear\":2027"));
    // 不得把金额写成裸数字。
    assert!(!json.contains(":10000000000000000"));
}

/// 金额字符串可无损反序列化回 u128(round-trip 保精度)。
#[test]
fn budget_amounts_roundtrip_without_precision_loss() {
    let json = serde_json::to_string(&sample_plan()).expect("serialize");
    let back: BudgetPlan = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back.total_expenditure, 10_000_000_000_000_000u128);
    assert_eq!(
        back.categories[0].sections[0].items[0].subitems[0].expenditure,
        10_000_000_000_000_000u128
    );
    let request = serde_json::json!({ "tier": 1, "voteType": 0, "plan": sample_plan() });
    let input: ProposeBudgetInput =
        serde_json::from_value(request).expect("request without region placeholder");
    assert_eq!(input.tier, 1);
    assert_eq!(input.plan.total_expenditure, back.total_expenditure);
    assert_eq!(ProposeBudgetInput::category(), ProposalCategory::Budget);
    assert_eq!(ProposeBudgetInput::category().as_u8(), 2);
}
