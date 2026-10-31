#[test]
fn node_product_version_is_identical_in_cli_ui_and_tauri() {
    use sc_cli::SubstrateCli;
    let version = nodecontrol::rpc::NODE_VERSION;
    assert_eq!(<crate::cli::Cli as SubstrateCli>::impl_version(), version);
    // 官方测试上下文避免重复嵌入macOS系统plist，版本仍读取同一真实配置。
    let context: tauri::Context<tauri::Wry> =
        tauri::generate_context!("tauri.conf.json", test = true);
    assert_eq!(context.package_info().version.to_string(), version);
}
