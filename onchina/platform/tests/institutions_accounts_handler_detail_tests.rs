//! 验证账户状态、真实余额及缺失展示数据的处理。

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


#[test]
fn account_balances_match_real_addresses_preserve_zero_and_large_values() {
    use std::collections::BTreeMap;
    let accounts = [0x11, 0x22, 0x33].map(|byte| {
        super::chain_account_row(&crate::chain_runtime::OnChainInstitutionAccount {
            cid_number: b"GZ018-SFGYR-201206100-2026".to_vec(),
            account_name: format!("账户{byte}").into_bytes(),
            account_id: [byte; 32],
        })
    });
    let mut rows = accounts;
    let huge = u128::MAX.to_string();
    let balances = BTreeMap::from([
        (format!("0x{}", "22".repeat(32)), Some(huge.clone())),
        (format!("0x{}", "11".repeat(32)), Some("0".to_string())),
        (format!("0x{}", "44".repeat(32)), Some("12345".to_string())),
    ]);
    super::attach_account_balances(&mut rows, &balances);
    assert_eq!(rows[0].balance_fen.as_deref(), Some("0"));
    assert_eq!(rows[1].balance_fen.as_deref(), Some(huge.as_str()));
    assert_eq!(rows[2].balance_fen, None);
    let value = serde_json::to_value(&rows).unwrap();
    assert_eq!(value[0]["balanceFen"], "0");
    assert_eq!(value[1]["balanceFen"], huge);
    assert!(value[2]["balanceFen"].is_null());
    assert_eq!(value[0]["chainStatus"], "ACTIVE_ON_CHAIN");
    assert_eq!(value[0]["accountId"], format!("0x{}", "11".repeat(32)));
}

#[test]
fn missing_display_balances_do_not_fabricate_zero_or_change_account_status() {
    let mut rows = vec![super::chain_account_row(
        &crate::chain_runtime::OnChainInstitutionAccount {
            cid_number: b"GZ018-SFGYR-201206100-2026".to_vec(),
            account_name: b"main".to_vec(),
            account_id: [0x11; 32],
        },
    )];
    super::attach_account_balances(&mut rows, &std::collections::BTreeMap::new());
    assert!(rows[0].balance_fen.is_none());
    assert_eq!(rows[0].chain_status, "ACTIVE_ON_CHAIN");
}
