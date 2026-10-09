//! 区块链测试 harness 命令行入口。
//!
//! 该二进制只用于本地真实验收和坏块材料准备，不能被生产节点调用。

use std::{fs, path::PathBuf};

use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "blockchain-harness")]
#[command(about = "CitizenChain 区块链验收与坏块材料工具")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// 使用 Alice 开发测试密钥签出 System::remark extrinsic hex。
    AliceRemark {
        #[arg(long)]
        genesis_hash: String,
        #[arg(long)]
        nonce: u32,
        #[arg(long)]
        spec_version: u32,
        #[arg(long)]
        tx_version: u32,
        #[arg(long)]
        remark: String,
    },
    /// 输出 export-blocks JSON 文件摘要。
    SummarizeExport {
        #[arg(long)]
        input: PathBuf,
    },
    /// 生成 stateRoot 篡改版 export-blocks JSON 文件。
    TamperStateRoot {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        output: PathBuf,
        #[arg(long)]
        replacement_state_root: String,
    },
}

fn main() {
    if let Err(err) = run() {
        eprintln!("{err}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let cli = Cli::parse();
    match cli.command {
        Command::AliceRemark {
            genesis_hash,
            nonce,
            spec_version,
            tx_version,
            remark,
        } => {
            let hex = blockchain_harness::alice_system_remark_extrinsic_hex(
                &genesis_hash,
                nonce,
                spec_version,
                tx_version,
                remark.as_bytes(),
            )?;
            println!("{hex}");
        }
        Command::SummarizeExport { input } => {
            let raw = fs::read_to_string(&input)
                .map_err(|e| format!("读取导出块文件失败 {}: {e}", input.display()))?;
            let summaries = blockchain_harness::summarize_exported_blocks_json(&raw)?;
            for (idx, item) in summaries.iter().enumerate() {
                println!(
                    "#{idx}: number={} parent={} state_root={} extrinsics={} digest_logs={}",
                    item.number_hex,
                    item.parent_hash,
                    item.state_root,
                    item.extrinsics_len,
                    item.digest_logs_len
                );
            }
        }
        Command::TamperStateRoot {
            input,
            output,
            replacement_state_root,
        } => {
            let raw = fs::read_to_string(&input)
                .map_err(|e| format!("读取导出块文件失败 {}: {e}", input.display()))?;
            let tampered =
                blockchain_harness::tamper_first_state_root_json(&raw, &replacement_state_root)?;
            fs::write(&output, tampered)
                .map_err(|e| format!("写入篡改块文件失败 {}: {e}", output.display()))?;
        }
    }
    Ok(())
}
