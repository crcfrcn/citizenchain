//! 司法工作台只覆盖现有能力、分区和禁用条件。
use super::*;
use onchina::modules::capability::{CapabilitySet, EMPTY};
use onchina::modules::model::WorkspaceSectionKind;

#[test]
fn sections_preserve_order_and_unopened_actions_stay_disabled() {
    let sections = judicial_sections(EMPTY);
    assert_eq!(
        sections
            .iter()
            .map(|section| section.workspace_section)
            .collect::<Vec<_>>(),
        vec![
            WorkspaceSectionKind::Operations,
            WorkspaceSectionKind::Display,
            WorkspaceSectionKind::Records
        ]
    );
    assert!(sections[0]
        .workspace_actions
        .iter()
        .all(|action| !action.enabled));
    assert_eq!(
        sections[2].workspace_actions[0].action,
        "view_operation_records"
    );
    assert!(!sections[2].workspace_actions[0].enabled);
}

#[test]
fn independent_capability_bits_only_enable_their_own_actions() {
    let sections = judicial_sections(CapabilitySet {
        can_sign_legislation: true,
        view_own_admins: true,
        ..EMPTY
    });
    assert!(
        sections[0]
            .workspace_actions
            .iter()
            .find(|action| action.action == "sign_legislation")
            .unwrap()
            .enabled
    );
    assert!(
        !sections[0]
            .workspace_actions
            .iter()
            .find(|action| action.action == "manage_own_admins")
            .unwrap()
            .enabled
    );
    assert!(
        sections[1]
            .workspace_actions
            .iter()
            .find(|action| action.action == "view_own_admins")
            .unwrap()
            .enabled
    );
    assert!(!sections[2].workspace_actions[0].enabled);
}
