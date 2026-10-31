#[cfg(feature = "std")]
fn main() {
    // 强制环境切换时重新运行 build.rs，确保嵌入的 WASM 来源明确；此文件不编译 WASM。
    println!("cargo:rerun-if-env-changed=WASM_FILE");
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
