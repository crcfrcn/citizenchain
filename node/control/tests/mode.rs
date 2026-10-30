use crate::mode::build_node_mode_state;

#[test]
fn pending_selected_mode_falls_back_to_archive() {
    let state = build_node_mode_state(crate::mode::NodeMode::Normal);

    assert_eq!(state.selected_mode, crate::mode::NodeMode::Archive);
    assert_eq!(state.effective_mode, crate::mode::NodeMode::Archive);
}

#[test]
fn mode_options_expose_archive_active_and_normal_pending() {
    let state = build_node_mode_state(crate::mode::NodeMode::Archive);

    let archive = state
        .options
        .iter()
        .find(|option| option.mode == crate::mode::NodeMode::Archive)
        .expect("archive option exists");
    let normal = state
        .options
        .iter()
        .find(|option| option.mode == crate::mode::NodeMode::Normal)
        .expect("normal option exists");

    assert_eq!(
        archive.implementation_status,
        crate::mode::NodeModeStatus::Active
    );
    assert!(archive.enabled);
    assert_eq!(
        normal.implementation_status,
        crate::mode::NodeModeStatus::Pending
    );
    assert!(!normal.enabled);
    assert_eq!(state.options.len(), 2);
}

#[test]
fn wire_value_rejects_unknown_node_mode() {
    let error = crate::mode::NodeMode::from_wire_value("light").expect_err("unknown mode fails");

    assert!(error.contains("全节点模式无效"));
}

#[test]
fn pending_wire_value_is_not_selectable() {
    let error = crate::mode::NodeMode::from_wire_value("normal")
        .and_then(crate::mode::NodeMode::ensure_enabled)
        .expect_err("pending mode is disabled");

    assert!(error.contains("尚未完成"));
}

#[test]
fn stored_communication_mode_is_cleaned_to_archive() {
    let mode = crate::mode::parse_stored_node_mode("communication")
        .expect("bad previous local state falls back");

    assert_eq!(mode, crate::mode::NodeMode::Archive);
}
