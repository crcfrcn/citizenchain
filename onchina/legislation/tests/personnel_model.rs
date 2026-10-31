//! 原所属模块回归，断言与输入保持。
use super::*;

fn sample_decision() -> PersonnelDecision {
    PersonnelDecision {
        action: PersonnelAction::Appoint,
        office_institution_code: "NRP".to_string(),
        office_title: "部长".to_string(),
        office_seat: 1,
        nominee_cid_number: "LN001-NRC0G-944805165-2026".to_string(),
        nominee_name: "张三".to_string(),
        term_index: 1,
        term_years: 5,
        reason: "依宪法第55条第1款提名任命".to_string(),
    }
}

/// 任免职书序列化为 camelCase + action 为 snake_case 枚举文案(锁定 CitizenApp 契约)。
#[test]
fn personnel_decision_serializes_camel_case() {
    let json = serde_json::to_string(&sample_decision()).expect("serialize decision");
    assert!(json.contains("\"action\":\"appoint\""));
    assert!(json.contains("\"officeInstitutionCode\":\"NRP\""));
    assert!(json.contains("\"nomineeCidNumber\":"));
    assert!(json.contains("\"termYears\":5"));
    // 不得出现 snake_case 字段名。
    assert!(!json.contains("office_institution_code"));
}

/// ProposePersonnelInput 从 camelCase JSON 反序列化,并携正确提案类型判别。
#[test]
fn propose_personnel_input_roundtrips_and_categorizes() {
    let input = ProposePersonnelInput {
        tier: 1,
        vote_type: 0,
        decision: sample_decision(),
    };
    // 原请求夹具不再携数字地域；完整 DTO 仍能按原业务字段反序列化。
    let request = serde_json::json!({
        "tier": input.tier, "voteType": input.vote_type, "decision": &input.decision,
    });
    let parsed: ProposePersonnelInput = serde_json::from_value(request).expect("request");
    assert_eq!(parsed.tier, 1);
    assert_eq!(parsed.decision.action, PersonnelAction::Appoint);
    let json = serde_json::to_value(&input.decision).expect("value");
    let back: PersonnelDecision = serde_json::from_value(json).expect("deserialize decision back");
    assert_eq!(back.action, PersonnelAction::Appoint);
    assert_eq!(back.office_seat, 1);
    assert_eq!(
        ProposePersonnelInput::category(),
        ProposalCategory::Personnel
    );
    assert_eq!(ProposePersonnelInput::category().as_u8(), 1);
}
