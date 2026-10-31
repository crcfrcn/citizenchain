//! 原所属模块回归，断言与输入保持。
#[test]
fn actual_chain_account_dto_exposes_status_without_changing_account_name() {
    let account = crate::chain_runtime::OnChainInstitutionAccount {
        cid_number: b"GZ018-SFGYR-201206100-2026".to_vec(),
        account_name: "业务专户A".as_bytes().to_vec(),
        account_id: [0x11; 32],
    };
    let value = serde_json::to_value(super::chain_account_row(&account))
        .expect("account DTO must serialize");
    assert_eq!(value["chainStatus"], "ACTIVE_ON_CHAIN");
    assert_eq!(value["accountName"], "业务专户A");
    assert_eq!(value["accountId"], format!("0x{}", "11".repeat(32)));
    assert_eq!(value["canClose"], true);
    assert!(value["createdAt"].is_null());
    // 对照前端 DTO 的全部真实协议账户分类，独立基金账户不得被漏成普通账户。
    let source = include_str!("../../platformweb/institutions/api.ts");
    assert!(source.contains("'federal_citizen_security_fund'"));
}
