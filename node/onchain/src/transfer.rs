//! 校验转账账户、精确金额、费用和备注，管理冷签交易提交。

use nodedesktop::constants::RPC_RESPONSE_LIMIT_SMALL;

use std::time::Duration;

use crate::wallet::SignMode;

use crate::wallet::Wallet;

use crate::wallet::WalletStore;

use crate::types::TransferSubmitResult;
use std::collections::HashSet;
use frame_support::{dispatch::GetDispatchInfo,traits::Get};

const TRANSFER_RPC_TIMEOUT: Duration = Duration::from_secs(45);

const EXISTENTIAL_DEPOSIT_FEN: u128 = primitives::core_const::ACCOUNT_EXISTENTIAL_DEPOSIT;

// 1.11 元
const MAX_TRANSFER_REMARK_BYTES: usize = 99;

pub(crate) fn normalize_account_id(account_id: &str) -> Result<String, String> {
    nodedesktop::validation::normalize_account_id(account_id)
        .map_err(|_| "account_id 格式无效，应为小写 0x + 64 位十六进制".to_string())
}

/// 金额仅接受规范十进制整分，拒绝浮点、指数和溢出。
pub fn parse_amount_fen(value: &str) -> Result<u128, String> {
    if value.is_empty() || value.len()>39 || !value.bytes().all(|b| b.is_ascii_digit()) || value.starts_with('0') {
        return Err("金额必须为正整数分".into());
    }
    value.parse().map_err(|_| "转账金额超出允许范围".into())
}

pub(crate) fn calculate_transfer_fee(amount_fen: u128, remark: &str) -> u128 {
    primitives::fee_policy::calculate_transfer_fee(amount_fen, !remark.is_empty())
}

fn validate_transfer_count(mode:crate::types::TransferMode,count:usize)->Result<(),String>{
    if count==0||count>runtime::configs::batch_calls_limit() as usize {return Err("转账清单数量超出官方限制".into());}
    if mode==crate::types::TransferMode::Transfer&&count!=1 {return Err("单笔入口必须只有一个收款项".into());}
    // 官方Utility还会追加开销；仅子项预算已超限时先拒绝，避免无意义的地址RPC解析。
    let leaf=<<runtime::Runtime as onchain::pallet::Config>::WeightInfo as onchain::weights::WeightInfo>::transfer_with_remark();
    if runtime::configs::RuntimeBlockWeights::get().get(frame_support::dispatch::DispatchClass::Normal).max_extrinsic.is_some_and(|max|leaf.saturating_mul(count as u64).any_gt(max)){return Err("转账清单超过链上单笔资源限制，请减少收款项".into());}
    Ok(())
}

/// 官方类型负责清单、备注及调用编码，不维护手写SCALE格式。
pub fn build_transfer_call(mode: crate::types::TransferMode, items: &[crate::types::TransferItem]) -> Result<runtime::RuntimeCall, String> {
    use crate::types::TransferMode;
    validate_transfer_count(mode,items.len())?;
    let mut calls = Vec::with_capacity(items.len());
    let mut total = 0u128;
    let mut fees = 0u128;
    for item in items {
        if item.to_address.len()>64 {return Err("收款地址过长".into());}
        let account = nodeclient::signing::account_id_from_ss58_address(&item.to_address)?;
        let amount = parse_amount_fen(&item.amount_fen)?;
        validate_transfer_remark(&item.remark)?;
        total = total.checked_add(amount).ok_or("本金合计溢出")?;
        fees = fees.checked_add(calculate_transfer_fee(amount, &item.remark)).ok_or("手续费合计溢出")?;
        calls.push(runtime::RuntimeCall::Onchain(onchain::pallet::Call::transfer_with_remark {
            beneficiary_account_id: account.into(), amount,
            remark: item.remark.as_bytes().to_vec().try_into().map_err(|_| "备注过长")?,
        }));
    }
    total.checked_add(fees).ok_or("总支出溢出")?;
    let call=match mode {
        TransferMode::Transfer if calls.len() == 1 => calls.remove(0),
        TransferMode::Transfer => return Err("单笔入口必须只有一个收款项".into()),
        TransferMode::Batch => runtime::RuntimeCall::Utility(pallet_utility::Call::batch { calls }),
        TransferMode::BatchAll => runtime::RuntimeCall::Utility(pallet_utility::Call::batch_all { calls }),
        TransferMode::ForceBatch => runtime::RuntimeCall::Utility(pallet_utility::Call::force_batch { calls }),
    };
    let info=call.get_dispatch_info();
    if runtime::configs::RuntimeBlockWeights::get().get(info.class).max_extrinsic.is_some_and(|max|info.call_weight.any_gt(max)) {
        return Err("转账清单超过链上单笔资源限制，请减少收款项".into());
    }
    Ok(call)
}

pub(crate) fn normalize_transfers(account_id: &str, mode: crate::types::TransferMode, mut items: Vec<crate::types::TransferItem>) -> Result<(Vec<crate::types::TransferItem>, u128), String> {
    validate_transfer_count(mode,items.len())?;
    let mut total = 0u128;
    let mut fee = 0u128;
    for item in &mut items {
        if item.to_address.len()>64 {return Err("收款地址过长".into());}
        let recipient = nodeclient::signing::resolve_recipient_account_id(&item.to_address)?;
        if format!("0x{}", hex::encode(recipient)) == account_id { return Err("收款账户不能与付款账户相同".into()); }
        item.to_address = nodeclient::signing::account_id_to_ss58(&recipient)?;
        let amount = parse_amount_fen(&item.amount_fen)?;
        validate_transfer_remark(&item.remark)?;
        total = total.checked_add(amount).ok_or("本金合计溢出")?;
        fee = fee.checked_add(calculate_transfer_fee(amount, &item.remark)).ok_or("手续费合计溢出")?;
    }
    build_transfer_call(mode, &items)?;
    // force_batch允许前项失败后继续，不能用全组余额检查拒绝可支付的后项。
    if !matches!(mode, crate::types::TransferMode::ForceBatch | crate::types::TransferMode::Batch) {
        ensure_spendable_balance(account_id, total, fee)?;
    }
    Ok((items, fee))
}

pub(crate) fn scan_from() -> Result<u32, String> {
    let hash = nodeclient::query::fetch_finalized_head()?;
    let header = nodedesktop::rpc::rpc_post("chain_getHeader", serde_json::json!([hash]), TRANSFER_RPC_TIMEOUT, RPC_RESPONSE_LIMIT_SMALL)?;
    let number = header.get("number").and_then(|x| x.as_str()).ok_or("最终块高度缺失")?;
    u32::from_str_radix(number.trim_start_matches("0x"), 16).map_err(|_| "最终块高度无效".into())
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
    total_needed
        .checked_add(EXISTENTIAL_DEPOSIT_FEN)
        .ok_or("转账所需余额超出范围")?;
    // finalized 总余额用于展示；这里扣除保留金及冻结限制后校验，链上执行最终裁决。
    let available = nodeinstitution::balance::fetch_spendable_balance(sender_account_id)?
        .ok_or("发送方账户不存在或余额为零")?;
    ensure_balance(available, total_needed, fee_fen)
}

fn ensure_balance(available: u128, total_needed: u128, fee_fen: u128) -> Result<(), String> {
    if available < total_needed {
        return Err(format!(
            "余额不足：可用 {} 元，需要 {} 元（含手续费 {} 元）",
            format!("{}.{:02}",available/100,available%100),
            format!("{}.{:02}",total_needed/100,total_needed%100),
            format!("{}.{:02}",fee_fen/100,fee_fen%100),
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
    mode: crate::types::TransferMode,
    transfers: Vec<crate::types::TransferItem>,
) -> Result<TransferSubmitResult, String> {
    let hot_wallet = require_wallet_sign_mode(app, &account_id, SignMode::Hot)?;
    let (transfers, _) = normalize_transfers(&hot_wallet.account_id, mode, transfers)?;
    let scan_from = scan_from()?;
    let auth_token = nodedesktop::security::issue_miner_transfer_token()?;
    let result = nodedesktop::rpc::rpc_post("transaction_submitMinerTransfer",
        serde_json::json!([hot_wallet.account_id, mode, transfers, auth_token.clone()]), TRANSFER_RPC_TIMEOUT, RPC_RESPONSE_LIMIT_SMALL);
    if result.is_err() { nodedesktop::security::revoke_miner_transfer_token(&auth_token); }
    let value = result?;
    let tx_hash = value.as_str().filter(|s| !s.is_empty()).ok_or("节点未返回交易哈希")?.to_owned();
    Ok(TransferSubmitResult { tx_hash, scan_from })
}

#[cfg(test)]
#[path = "../tests/transfer.rs"]
mod tests;

#[cfg(test)]
#[path = "../tests/integration.rs"]
mod integration_tests;
