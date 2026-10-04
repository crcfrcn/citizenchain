use qr_protocol::export::{
    export_qr_bodies_dart, export_qr_bodies_rust, export_qr_bodies_typescript,
    export_registry_dart, export_registry_json,
};
use std::env;
use std::fs;
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = env::args().skip(1).collect::<Vec<_>>();
    match args.as_slice() {
        [] => {
            println!("{}", export_registry_json()?);
        }
        [flag, output] if flag == "--dart" => {
            let path = Path::new(output);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(path, export_registry_dart()?)?;
        }
        [flag, output] if flag == "--dart-bodies" => {
            write_generated(output, export_qr_bodies_dart()?)?;
        }
        [flag, output] if flag == "--ts-bodies" => {
            write_generated(output, export_qr_bodies_typescript()?)?;
        }
        [flag, output] if flag == "--rust-bodies" => {
            write_generated(output, export_qr_bodies_rust()?)?;
        }
        [flag] if flag == "--check" => check_generated_files()?,
        _ => {
            eprintln!(
                "用法: export_registry [--check | --dart|--dart-bodies|--ts-bodies|--rust-bodies <输出文件>]"
            );
            std::process::exit(2);
        }
    }
    Ok(())
}

fn write_generated(output: &str, content: String) -> Result<(), std::io::Error> {
    let path = Path::new(output);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, content)
}

fn check_generated_files() -> Result<(), Box<dyn std::error::Error>> {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
        .ok_or("qr-protocol 必须位于 citizenchain/crates/qr-protocol")?;
    let cases = [
        (
            "citizenapp/lib/qr/generated/qr_bodies.g.dart",
            export_qr_bodies_dart()?,
        ),
        (
            "citizenwallet/lib/qr/generated/qr_bodies.g.dart",
            export_qr_bodies_dart()?,
        ),
        (
            "citizenchain/node/frontend/shared/qr/qrBodies.g.ts",
            export_qr_bodies_typescript()?,
        ),
        (
            "citizenchain/onchina/frontend/core/qrBodies.g.ts",
            export_qr_bodies_typescript()?,
        ),
        (
            "citizenchain/onchina/src/core/qr/generated.rs",
            export_qr_bodies_rust()?,
        ),
    ];
    for (relative, expected) in cases {
        let path = repo_root.join(relative);
        let actual = fs::read_to_string(&path)?;
        if actual != expected {
            return Err(format!("生成产物不是最新版本: {}", path.display()).into());
        }
    }
    Ok(())
}
