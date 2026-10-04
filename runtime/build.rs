#[cfg(feature = "std")]
fn main() {
    // 强制环境切换时重新运行 build.rs，确保 WASM 来源明确。
    println!("cargo:rerun-if-env-changed=WASM_FILE");
    println!("cargo:rerun-if-env-changed=WASM_BUILD_FROM_SOURCE");
    println!("cargo:rerun-if-env-changed=CITIZENCHAIN_PRODUCTION_WASM_BUILD");
    println!("cargo:rerun-if-env-changed=CARGO_FEATURE_RUNTIME_BENCHMARKS");

    // 正式候选 WASM 与 benchmark runtime 必须是两条不可交叉的构建路径。
    // 权重生成也需要源码 WASM，因此只由正式流水线显式开启本闸门。
    if std::env::var_os("CITIZENCHAIN_PRODUCTION_WASM_BUILD").is_some()
        && std::env::var_os("CARGO_FEATURE_RUNTIME_BENCHMARKS").is_some()
    {
        panic!("正式 WASM 源码构建禁止启用 runtime-benchmarks");
    }

    if let Ok(wasm_file) = std::env::var("WASM_FILE") {
        // ── 使用 CI 预编译的 WASM（本地启动脚本、全新创世、升级工具）──
        let out_dir =
            std::env::var("OUT_DIR").unwrap_or_else(|error| panic!("OUT_DIR not set: {error}"));
        let dest = std::path::Path::new(&out_dir).join("wasm_binary.rs");

        let wasm_path = std::path::Path::new(&wasm_file)
            .canonicalize()
            .unwrap_or_else(|e| panic!("WASM_FILE 路径无效: {wasm_file}: {e}"));
        let wasm_path_str = wasm_path.display().to_string().replace('\\', "/");

        std::fs::write(
            &dest,
            format!(
                r#"pub const WASM_BINARY: Option<&[u8]> = Some(include_bytes!("{wasm_path_str}"));
pub const WASM_BINARY_BLOATY: Option<&[u8]> = Some(include_bytes!("{wasm_path_str}"));
"#,
            ),
        )
        .unwrap_or_else(|error| panic!("写入 wasm_binary.rs 失败: {error}"));

        eprintln!("使用 CI WASM: {wasm_path_str}");
    } else if std::env::var("WASM_BUILD_FROM_SOURCE").is_ok() {
        // ── WASM CI 专用：从源码编译 WASM（仅 WASM CI workflow 使用）──
        // WASM 链接参数（`--allow-undefined`）由公民链目录 config.toml 的
        // `[env] WASM_BUILD_RUSTFLAGS` 统一注入，此处不重复设置。
        substrate_wasm_builder::WasmBuilder::build_using_defaults();
    } else {
        // ── 普通桌面端打包：不内置 runtime WASM。
        // 现有链运行时从链上状态读取 runtime code，不依赖安装包内置 WASM。
        // 只有本地重新创世或 runtime 升级工具才需要通过 WASM_FILE 显式提供 WASM。
        let out_dir =
            std::env::var("OUT_DIR").unwrap_or_else(|error| panic!("OUT_DIR not set: {error}"));
        let dest = std::path::Path::new(&out_dir).join("wasm_binary.rs");
        std::fs::write(
            &dest,
            r#"pub const WASM_BINARY: Option<&[u8]> = None;
pub const WASM_BINARY_BLOATY: Option<&[u8]> = None;
"#,
        )
        .unwrap_or_else(|error| panic!("写入空 wasm_binary.rs 失败: {error}"));

        eprintln!("未设置 WASM_FILE；本次构建不内置 runtime WASM");
    }
}

#[cfg(not(feature = "std"))]
fn main() {}
