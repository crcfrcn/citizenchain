//! 验证准确CID辖区及旧本地名册写入和挑战入口关闭。

use super::*;
use onchina::institutions::{
    GovernanceAssignment, GovernanceMember, GovernanceRole, GovernanceState,
};

fn cid(province: &str, city: &str) -> String {
    let payload = format!("{province}{city}CREG0000000012026");
    format!(
        "{province}{city}-CREG{}-000000001-2026",
        primitives::cid::number::checksum_char_m1(&payload, false)
    )
}
fn actor() -> AdminAuthContext {
    AdminAuthContext {
        account_id: format!("0x{}", "11".repeat(32)),
        institution_cid_number: "ZS001-FRG07-249474503-2026".into(),
        institution_code: "FRG".into(),
        admin_level: Some("PROVINCE".into()),
        family_name: "测".into(),
        given_name: "试".into(),
        scope_province_name: Some("贵州省".into()),
        scope_city_name: None,
        scope_town_name: None,
        cid_short_name: None,
    }
}
#[test]
fn registry_targets_require_exact_cid_province_and_current_institution() {
    let mut ctx = actor();
    let target = cid("GZ", "018");
    assert_eq!(
        registry_target_scope(&ctx, &target).unwrap(),
        ("贵州省".into(), "绥阳市".into())
    );
    assert!(registry_target_scope(&ctx, &cid("GD", "001")).is_err());
    assert!(registry_target_scope(&ctx, &cid("GZ", "000")).is_err());
    assert!(registry_target_scope(&ctx, "invalid").is_err());
    ctx.institution_code = "CREG".into();
    ctx.institution_cid_number = target.clone();
    assert!(registry_target_scope(&ctx, &target).is_ok());
    assert!(registry_target_scope(&ctx, &cid("GZ", "001")).is_err());
    ctx.institution_code = "CFIN".into();
    assert!(registry_target_scope(&ctx, &target).is_err());
}
#[test]
fn legacy_local_roster_create_delete_and_prepared_challenges_are_closed() {
    use crate::admins::actions::require_chain_roster_action;
    use onchina::authentication::operation_auth::AdminActionType;
    assert!(require_chain_roster_action(&AdminActionType::CreateCityRegistry).is_err());
    assert!(require_chain_roster_action(&AdminActionType::DeleteCityRegistry).is_err());
    assert!(require_chain_roster_action(&AdminActionType::NodeBindingUnbind).is_ok());
}
#[test]
fn chain_roster_keeps_unassigned_people_and_rebound_wallets_without_local_metadata() {
    let target = cid("GZ", "018");
    let anchor = format!("0x{}", "11".repeat(32));
    let signer = format!("0x{}", "22".repeat(32));
    let member = GovernanceMember {
        account_id: anchor.clone(),
        signer_id: signer.clone(),
        cid_number: "CN220-CTZN2-198805200-2026".into(),
        family_name: "张".into(),
        given_name: "三".into(),
    };
    let mut snapshot = GovernanceState {
        cid_number: target.clone(),
        block_hash: format!("0x{}", "ab".repeat(32)),
        block_number: 8,
        current_day: 10,
        module_tag: "pub-mgmt".into(),
        legal_role_code: "LR".into(),
        members: vec![
            member.clone(),
            GovernanceMember {
                account_id: format!("0x{}", "33".repeat(32)),
                signer_id: format!("0x{}", "33".repeat(32)),
                ..member
            },
        ],
        roles: vec![GovernanceRole {
            role_code: "R_DYNAMIC".into(),
            role_name: "登记办理岗".into(),
            term_required: false,
            active: true,
            fixed: false,
            assignment_locked: false,
            min_assignments: 0,
            max_assignments: None,
            permissions: vec![],
            assignments: vec![GovernanceAssignment {
                account_id: anchor.clone(),
                term_start: 0,
                term_end: 0,
                active: true,
                effective: true,
                source_label: "注册局".into(),
                source_ref: "0xff00".into(),
            }],
        }],
        representative: None,
        registration_state_hash: format!("0x{}", "12".repeat(32)),
        governance_threshold: Some(2),
        registration_roles: vec![],
    };
    let read = |s: &GovernanceState| {
        crate::admins::city_registry_admins::chain_registry_rows(&target, "绥阳市", s).unwrap()
    };
    let rows = read(&snapshot);
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].account_id, signer);
    assert_eq!(rows[0].roster_account_id, anchor);
    assert_eq!(rows[0].role_name, "登记办理岗");
    assert!(rows[1].role_name.is_empty());
    assert!(rows[0].created_at.is_none());
    assert!(rows[0].creator_account_id.is_empty());
    let original_id = rows[0].id.clone();
    snapshot.members[0].signer_id = format!("0x{}", "44".repeat(32));
    snapshot.roles[0].assignments[0].effective = false;
    let rows = read(&snapshot);
    assert_eq!(rows[0].id, original_id);
    assert!(rows[0].role_name.is_empty());
    assert_eq!(rows[0].account_id, snapshot.members[0].signer_id);
    assert!(crate::admins::city_registry_admins::chain_registry_rows(
        &cid("GD", "001"),
        "同名市",
        &snapshot
    )
    .is_err());
}
