use nodedesktop::constants::RPC_RESPONSE_LIMIT_SMALL;

use std::time::Duration;

use crate::wallet::SignMode;

use crate::wallet::Wallet;

use crate::wallet::WalletStore;

use crate::types::TransferSubmitResult;
use std::collections::HashSet;

const TRANSFER_RPC_TIMEOUT: Duration = Duration::from_secs(45);

const EXISTENTIAL_DEPOSIT_FEN: u128 = 111;

// 1.11 元
const ONCHAIN_TRANSACTION_PALLET_INDEX: u8 = 4;

const TRANSFER_WITH_REMARK_CALL_INDEX: u8 = 0;

const MAX_TRANSFER_REMARK_BYTES: usize = 99;

pub(crate) fn normalize_account_id(account_id: &str) -> Result<String, String> {
    nodedesktop::validation::normalize_account_id(account_id)
        .map_err(|_| "account_id 格式无效，应为小写 0x + 64 位十六进制".to_string())
}

pub(crate) fn amount_yuan_to_fen(amount_yuan: f64) -> Result<u128, String> {
    if !amount_yuan.is_finite() {
        return Err("转账金额格式无效".to_string());
    }
    if amount_yuan < 0.01 {
        return Err("转账金额不能小于 0.01 元".to_string());
    }
    let rounded_fen = (amount_yuan * 100.0).round();
    // u128::MAX 转为 f64 会舍入到 2^128，上界必须排除等号，避免饱和转换。
    if !rounded_fen.is_finite() || rounded_fen >= u128::MAX as f64 {
        return Err("转账金额超出允许范围".to_string());
    }
    let amount_fen = rounded_fen as u128;
    if amount_fen == 0 {
        return Err("转账金额不能为零".to_string());
    }
    Ok(amount_fen)
}

pub(crate) fn calculate_transfer_fee(amount_fen: u128) -> u128 {
    primitives::fee_policy::calculate_onchain_fee(amount_fen)
}

pub(crate) fn validate_transfer_remark(remark: &str) -> Result<(), String> {
    let len = remark.len();
    if len > MAX_TRANSFER_REMARK_BYTES {
        return Err(format!(
            "转账备注不能超过 {MAX_TRANSFER_REMARK_BYTES} 字节，当前 {len} 字节"
        ));
    }
    Ok(())
}

pub(crate) fn ensure_spendable_balance(
    sender_account_id: &str,
    amount_fen: u128,
    fee_fen: u128,
) -> Result<(), String> {
    // 在读取链数据前拒绝金额溢出；手续费公式及账户保留金规则保持原值。
    let total_needed = amount_fen
        .checked_add(fee_fen)
        .ok_or("转账金额与手续费之和超出范围")?;
    let minimum_balance = total_needed
        .checked_add(EXISTENTIAL_DEPOSIT_FEN)
        .ok_or("转账所需余额超出范围")?;
    // 桌面端金额显示统一以 finalized 为准；余额不足文案也使用同一口径，
    // 真正能否入块仍由 runtime 在交易执行时最终校验。
    let balance_fen = nodeinstitution::balance::fetch_balance(sender_account_id)?
        .ok_or("发送方账户不存在或余额为零")?;
    if balance_fen < minimum_balance {
        let available = if balance_fen > EXISTENTIAL_DEPOSIT_FEN {
            (balance_fen - EXISTENTIAL_DEPOSIT_FEN) as f64 / 100.0
        } else {
            0.0
        };
        return Err(format!(
            "余额不足：可用 {} 元，需要 {} 元（含手续费 {} 元）",
            nodeclient::signing::format_amount(available),
            nodeclient::signing::format_amount(total_needed as f64 / 100.0),
            nodeclient::signing::format_amount(fee_fen as f64 / 100.0),
        ));
    }
    Ok(())
}

pub(crate) fn local_miner_wallet(app: &tauri::AppHandle) -> Result<Option<Wallet>, String> {
    let Some(account_id) = nodecontrol::reward::local_powr_miner_account_id(app)? else {
        return Ok(None);
    };
    let account_id = normalize_account_id(&account_id)?;

    Ok(Some(Wallet {
        name: "矿工热钱包".to_string(),
        sign_mode: crate::wallet::SignMode::Hot,
        account_id,
    }))
}

/// 持久化文件只允许 Cold；Hot 必须由本机 powr 私钥事实动态生成。
pub(crate) fn validate_persisted_store(store: &WalletStore) -> Result<(), String> {
    let mut account_ids = HashSet::with_capacity(store.wallets.len());
    for wallet in &store.wallets {
        if wallet.sign_mode != crate::wallet::SignMode::Cold {
            return Err(format!(
                "钱包 {} 的 sign_mode 必须为 cold，请清理后重新导入",
                wallet.account_id
            ));
        }
        if wallet.name.trim().is_empty() {
            return Err("钱包名称不能为空，请清理后重新导入".to_string());
        }
        let account_id = normalize_account_id(&wallet.account_id)?;
        if account_id != wallet.account_id {
            return Err("account_id 必须使用规范小写格式，请清理后重新导入".to_string());
        }
        if !account_ids.insert(wallet.account_id.as_str()) {
            return Err(format!("钱包账户重复: {}", wallet.account_id));
        }
    }
    if let Some(active_account_id) = store.active_account_id.as_deref() {
        let normalized = normalize_account_id(active_account_id)?;
        if normalized != active_account_id {
            return Err("active_account_id 必须使用规范小写格式".to_string());
        }
    }
    Ok(())
}

pub(crate) fn wallet_store_for_frontend(
    app: &tauri::AppHandle,
    store: WalletStore,
) -> Result<WalletStore, String> {
    validate_persisted_store(&store)?;

    let miner_wallet = local_miner_wallet(app)?;
    if let Some(miner) = miner_wallet.as_ref() {
        if store
            .wallets
            .iter()
            .any(|wallet| wallet.account_id == miner.account_id)
        {
            return Err(format!(
                "账户 {} 同时存在 Hot 与 Cold 钱包事实，已拒绝加载",
                miner.account_id
            ));
        }
    }

    // 冷钱包文件只保存用户导入项；矿工 Hot 钱包每次从 powr 密钥事实动态注入。
    let mut wallets = Vec::with_capacity(store.wallets.len() + usize::from(miner_wallet.is_some()));
    if let Some(wallet) = miner_wallet {
        wallets.push(wallet);
    }
    wallets.extend(store.wallets);

    let active_account_id = store
        .active_account_id
        .filter(|account_id| {
            wallets
                .iter()
                .any(|wallet| wallet.account_id == account_id.as_str())
        })
        .or_else(|| wallets.first().map(|wallet| wallet.account_id.clone()));

    Ok(WalletStore {
        wallets,
        active_account_id,
    })
}

fn require_sign_mode(wallet: &Wallet, expected: SignMode) -> Result<(), String> {
    if wallet.sign_mode == expected {
        return Ok(());
    }
    let expected_name = match expected {
        crate::wallet::SignMode::Hot => "hot",
        crate::wallet::SignMode::Cold => "cold",
    };
    Err(format!(
        "钱包 {} 的 sign_mode 不是 {expected_name}，已拒绝签名",
        wallet.account_id
    ))
}

/// 以 account_id 为唯一键重新读取钱包事实并强制匹配签名模式。
pub(crate) fn require_wallet_sign_mode(
    app: &tauri::AppHandle,
    account_id: &str,
    expected: SignMode,
) -> Result<Wallet, String> {
    let account_id = normalize_account_id(account_id)?;
    let store = wallet_store_for_frontend(app, crate::wallet::load(app)?)?;
    let wallet = store
        .wallets
        .into_iter()
        .find(|wallet| wallet.account_id == account_id)
        .ok_or_else(|| format!("钱包账户不存在: {account_id}"))?;
    require_sign_mode(&wallet, expected)?;
    Ok(wallet)
}

pub(crate) fn submit_hot_transfer_inner(
    app: &tauri::AppHandle,
    account_id: String,
    to_ss58_address: String,
    amount_yuan: f64,
    remark: String,
) -> Result<TransferSubmitResult, String> {
    // 阻塞任务内再次读取 Hot 事实，避免密码校验期间本机密钥发生变化后继续签名。
    let hot_wallet = require_wallet_sign_mode(app, &account_id, crate::wallet::SignMode::Hot)?;
    let to_ss58_address = to_ss58_address.trim().to_string();
    let destination_account_id_bytes =
        nodeclient::signing::account_id_from_ss58_address(&to_ss58_address)?;
    let destination_account_id = format!("0x{}", hex::encode(destination_account_id_bytes));
    if destination_account_id == hot_wallet.account_id {
        return Err("收款地址不能与矿工热钱包相同".to_string());
    }

    let amount_fen = amount_yuan_to_fen(amount_yuan)?;
    validate_transfer_remark(&remark)?;
    let fee_fen = calculate_transfer_fee(amount_fen);
    ensure_spendable_balance(&hot_wallet.account_id, amount_fen, fee_fen)?;

    // 真正的私钥签名只发生在节点 RPC 内部；一次性令牌避免外部本机 RPC 直接花费矿工余额。
    let auth_token = nodedesktop::security::issue_miner_transfer_token()?;
    let result = nodedesktop::rpc::rpc_post(
        "transaction_submitMinerTransfer",
        serde_json::json!([
            to_ss58_address,
            amount_fen.to_string(),
            remark,
            auth_token.clone()
        ]),
        TRANSFER_RPC_TIMEOUT,
        RPC_RESPONSE_LIMIT_SMALL,
    );
    if result.is_err() {
        nodedesktop::security::revoke_miner_transfer_token(&auth_token);
    }
    let result = result?;
    let tx_hash = result
        .as_str()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .ok_or("节点未返回交易哈希")?
        .to_string();

    Ok(TransferSubmitResult { tx_hash })
}

// ──── 编码工具 ────

/// 构造 OnchainTransaction::transfer_with_remark 的 SCALE 编码 call data。
///
/// 格式：[pallet=4][call=0][beneficiary:AccountId32][amount:u128_le][remark:BoundedVec<u8>]
pub(crate) fn build_transfer_with_remark_call(
    destination_account_id: &[u8],
    amount_fen: u128,
    remark: &str,
) -> Result<Vec<u8>, String> {
    if destination_account_id.len() != 32 {
        return Err("收款账户公钥长度无效".to_string());
    }
    validate_transfer_remark(remark)?;

    let remark_bytes = remark.as_bytes();
    let remark_len = encode_compact_u32(remark_bytes.len() as u32);
    let mut call_data = Vec::with_capacity(2 + 32 + 16 + remark_len.len() + remark_bytes.len());
    call_data.push(ONCHAIN_TRANSACTION_PALLET_INDEX);
    call_data.push(TRANSFER_WITH_REMARK_CALL_INDEX);
    call_data.extend_from_slice(destination_account_id);
    call_data.extend_from_slice(&amount_fen.to_le_bytes());
    call_data.extend_from_slice(&remark_len);
    call_data.extend_from_slice(remark_bytes);
    Ok(call_data)
}

/// SCALE Compact<u32> 编码，用于备注字节长度。
fn encode_compact_u32(value: u32) -> Vec<u8> {
    if value < 0x40 {
        vec![(value as u8) << 2]
    } else if value < 0x4000 {
        let v = (value << 2) | 0x01;
        vec![v as u8, (v >> 8) as u8]
    } else {
        let v = (value << 2) | 0x02;
        v.to_le_bytes().to_vec()
    }
}

#[cfg(test)]
#[path = "../tests/transfer.rs"]
mod tests;

#[cfg(test)]
#[path = "../tests/integration.rs"]
mod integration_tests;
