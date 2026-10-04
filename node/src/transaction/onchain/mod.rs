// 交易模块：钱包管理 + 带备注链上转账（OnchainTransaction::transfer_with_remark）。
//
// Hot 只能使用本机 powr 私钥签名，Cold 只能通过 QR_V1 交给 CitizenWallet 离线签名；
// 所有签名入口都按 account_id 重新读取 SignMode，不信任前端选择的签名路径。

pub(crate) mod wallet_store;

use crate::{
    governance::{institution, signing},
    settings::{device_password, reward_account},
    shared::{constants::RPC_RESPONSE_LIMIT_SMALL, rpc, security},
};
use serde::Serialize;
use std::{
    collections::HashSet,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use wallet_store::{SignMode, Wallet, WalletStore};

const TRANSFER_RPC_TIMEOUT: Duration = Duration::from_secs(45);
const EXISTENTIAL_DEPOSIT_FEN: u128 = 111; // 1.11 元
const ONCHAIN_TRANSACTION_PALLET_INDEX: u8 = 4;
const TRANSFER_WITH_REMARK_CALL_INDEX: u8 = 0;
const MAX_TRANSFER_REMARK_BYTES: usize = 99;

/// 转账签名请求结果（前端用于显示 QR 码）。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferSignRequestResult {
    pub request_json: String,
    pub request_id: String,
    pub expected_payload_hash: String,
    pub sign_nonce: u32,
    pub sign_block_number: u64,
    /// call_data 的 hex 编码（提交时需要回传）。
    pub call_data_hex: String,
    /// 预估手续费（元）。
    pub fee_yuan: f64,
}

/// 转账提交结果。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferSubmitResult {
    pub tx_hash: String,
}

fn normalize_account_id(account_id: &str) -> Result<String, String> {
    crate::shared::validation::normalize_account_id(account_id)
        .map_err(|_| "account_id 格式无效，应为小写 0x + 64 位十六进制".to_string())
}

fn amount_yuan_to_fen(amount_yuan: f64) -> Result<u128, String> {
    if !amount_yuan.is_finite() {
        return Err("转账金额格式无效".to_string());
    }
    if amount_yuan < 0.01 {
        return Err("转账金额不能小于 0.01 元".to_string());
    }
    let amount_fen = (amount_yuan * 100.0).round() as u128;
    if amount_fen == 0 {
        return Err("转账金额不能为零".to_string());
    }
    Ok(amount_fen)
}

fn calculate_transfer_fee(amount_fen: u128) -> u128 {
    primitives::fee_policy::calculate_onchain_fee(amount_fen)
}

fn validate_transfer_remark(remark: &str) -> Result<(), String> {
    let len = remark.len();
    if len > MAX_TRANSFER_REMARK_BYTES {
        return Err(format!(
            "转账备注不能超过 {MAX_TRANSFER_REMARK_BYTES} 字节，当前 {len} 字节"
        ));
    }
    Ok(())
}

fn ensure_spendable_balance(
    sender_account_id: &str,
    amount_fen: u128,
    fee_fen: u128,
) -> Result<(), String> {
    // 桌面端金额显示统一以 finalized 为准；余额不足文案也使用同一口径，
    // 真正能否入块仍由 runtime 在交易执行时最终校验。
    let balance_fen =
        institution::fetch_balance(sender_account_id)?.ok_or("发送方账户不存在或余额为零")?;
    let total_needed = amount_fen + fee_fen;
    if balance_fen < total_needed + EXISTENTIAL_DEPOSIT_FEN {
        let available = if balance_fen > EXISTENTIAL_DEPOSIT_FEN {
            (balance_fen - EXISTENTIAL_DEPOSIT_FEN) as f64 / 100.0
        } else {
            0.0
        };
        return Err(format!(
            "余额不足：可用 {} 元，需要 {} 元（含手续费 {} 元）",
            signing::format_amount(available),
            signing::format_amount(total_needed as f64 / 100.0),
            signing::format_amount(fee_fen as f64 / 100.0),
        ));
    }
    Ok(())
}

fn local_miner_wallet(app: &tauri::AppHandle) -> Result<Option<Wallet>, String> {
    let Some(account_id) = reward_account::local_powr_miner_account_id(app)? else {
        return Ok(None);
    };
    let account_id = normalize_account_id(&account_id)?;
    let account_id_bytes: [u8; 32] = hex::decode(account_id.trim_start_matches("0x"))
        .map_err(|e| format!("矿工账户 ID 解码失败: {e}"))?
        .try_into()
        .map_err(|_| "矿工账户 ID 必须为 32 字节".to_string())?;
    let ss58_address = signing::account_id_to_ss58(&account_id_bytes)?;

    Ok(Some(Wallet {
        name: "矿工热钱包".to_string(),
        sign_mode: SignMode::Hot,
        ss58_address,
        account_id,
        created_at: 0,
    }))
}

/// 持久化文件只允许 Cold；Hot 必须由本机 powr 私钥事实动态生成。
fn validate_persisted_store(store: &WalletStore) -> Result<(), String> {
    let mut account_ids = HashSet::with_capacity(store.wallets.len());
    for wallet in &store.wallets {
        if wallet.sign_mode != SignMode::Cold {
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
        let ss58_account_id = signing::account_id_from_ss58_address(&wallet.ss58_address)?;
        let expected_account_id = format!("0x{}", hex::encode(ss58_account_id));
        if expected_account_id != wallet.account_id {
            return Err("ss58_address 与 account_id 不匹配，请清理后重新导入".to_string());
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

fn wallet_store_for_frontend(
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
        SignMode::Hot => "hot",
        SignMode::Cold => "cold",
    };
    Err(format!(
        "钱包 {} 的 sign_mode 不是 {expected_name}，已拒绝签名",
        wallet.account_id
    ))
}

/// 以 account_id 为唯一键重新读取钱包事实并强制匹配签名模式。
fn require_wallet_sign_mode(
    app: &tauri::AppHandle,
    account_id: &str,
    expected: SignMode,
) -> Result<Wallet, String> {
    let account_id = normalize_account_id(account_id)?;
    let store = wallet_store_for_frontend(app, wallet_store::load(app)?)?;
    let wallet = store
        .wallets
        .into_iter()
        .find(|wallet| wallet.account_id == account_id)
        .ok_or_else(|| format!("钱包账户不存在: {account_id}"))?;
    require_sign_mode(&wallet, expected)?;
    Ok(wallet)
}

// ──── 钱包管理命令 ────

#[tauri::command(rename_all = "snake_case")]
pub fn get_wallets(app: tauri::AppHandle) -> Result<WalletStore, String> {
    let store = wallet_store::load(&app)?;
    wallet_store_for_frontend(&app, store)
}

#[tauri::command(rename_all = "snake_case")]
pub fn add_wallet(
    app: tauri::AppHandle,
    name: String,
    ss58_address: String,
) -> Result<Wallet, String> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err("钱包名称不能为空".to_string());
    }
    let account_id_bytes = signing::account_id_from_ss58_address(ss58_address.trim())?;
    let ss58_address = signing::account_id_to_ss58(&account_id_bytes)?;
    let account_id = format!("0x{}", hex::encode(account_id_bytes));

    let mut store = wallet_store::load(&app)?;
    validate_persisted_store(&store)?;

    let miner_wallet = local_miner_wallet(&app)?;
    if let Some(miner_wallet) = miner_wallet.as_ref() {
        if miner_wallet.account_id == account_id {
            return Err("矿工热钱包已在列表中，无需重复添加".to_string());
        }
    }

    // 查重：同一账户 ID 不能重复添加。
    if store
        .wallets
        .iter()
        .any(|wallet| wallet.account_id == account_id)
    {
        return Err("该地址已存在".to_string());
    }

    let wallet = Wallet {
        name,
        sign_mode: SignMode::Cold,
        ss58_address,
        account_id,
        created_at: now_secs(),
    };

    store.wallets.push(wallet.clone());
    if store.active_account_id.is_none() && miner_wallet.is_none() {
        store.active_account_id = Some(wallet.account_id.clone());
    }
    wallet_store::save(&app, &store)?;
    Ok(wallet)
}

#[tauri::command(rename_all = "snake_case")]
pub fn remove_wallet(app: tauri::AppHandle, account_id: String) -> Result<WalletStore, String> {
    let account_id = normalize_account_id(&account_id)?;
    let miner_wallet = local_miner_wallet(&app)?;
    if miner_wallet
        .as_ref()
        .is_some_and(|wallet| wallet.account_id == account_id)
    {
        return Err("矿工热钱包不能删除".to_string());
    }
    let mut store = wallet_store::load(&app)?;
    validate_persisted_store(&store)?;
    let before_len = store.wallets.len();
    store
        .wallets
        .retain(|wallet| wallet.account_id != account_id);
    if store.wallets.len() == before_len {
        return Err("钱包不存在".to_string());
    }
    if store.active_account_id.as_deref() == Some(account_id.as_str()) {
        store.active_account_id = miner_wallet.map(|wallet| wallet.account_id).or_else(|| {
            store
                .wallets
                .first()
                .map(|wallet| wallet.account_id.clone())
        });
    }
    wallet_store::save(&app, &store)?;
    wallet_store_for_frontend(&app, store)
}

#[tauri::command(rename_all = "snake_case")]
pub fn set_active_wallet(app: tauri::AppHandle, account_id: String) -> Result<WalletStore, String> {
    let account_id = normalize_account_id(&account_id)?;
    let mut store = wallet_store::load(&app)?;
    validate_persisted_store(&store)?;
    let frontend_store = wallet_store_for_frontend(&app, store.clone())?;
    if !frontend_store
        .wallets
        .iter()
        .any(|wallet| wallet.account_id == account_id)
    {
        return Err("钱包不存在".to_string());
    }
    store.active_account_id = Some(account_id);
    wallet_store::save(&app, &store)?;
    wallet_store_for_frontend(&app, store)
}

#[tauri::command(rename_all = "snake_case")]
pub fn get_wallet_balance(account_id: String) -> Result<Option<String>, String> {
    let account_id = normalize_account_id(&account_id)?;
    match institution::fetch_balance(&account_id)? {
        Some(fen) => Ok(Some(fen.to_string())),
        None => Ok(None),
    }
}

// ──── 转账命令 ────

/// 为 Cold 钱包构建 OnchainTransaction::transfer_with_remark 签名请求。
///
/// 返回 QR 签名请求 JSON，前端显示 QR 码供离线设备扫码签名。
#[tauri::command(rename_all = "snake_case")]
pub fn build_cold_transfer_request(
    app: tauri::AppHandle,
    account_id: String,
    to_ss58_address: String,
    amount_yuan: f64,
    remark: String,
) -> Result<TransferSignRequestResult, String> {
    let wallet = require_wallet_sign_mode(&app, &account_id, SignMode::Cold)?;
    // sr25519 AccountId32 与签名公钥字节相同；签名者只取自后端钱包事实。
    let signer_public_key = wallet.account_id;
    let signer_public_key_bytes = hex::decode(signer_public_key.trim_start_matches("0x"))
        .map_err(|e| format!("发送方公钥解码失败: {e}"))?;

    // 校验收款地址
    let destination_account_id_bytes = signing::account_id_from_ss58_address(&to_ss58_address)?;
    let destination_account_id = format!("0x{}", hex::encode(destination_account_id_bytes));
    if destination_account_id == signer_public_key {
        return Err("收款地址不能与发送方相同".to_string());
    }

    let amount_fen = amount_yuan_to_fen(amount_yuan)?;
    validate_transfer_remark(&remark)?;

    // 前端预估费和链上实扣费统一复用 runtime 手续费公式。
    let fee_fen = calculate_transfer_fee(amount_fen);
    let fee_yuan = fee_fen as f64 / 100.0;

    // 校验余额
    ensure_spendable_balance(&signer_public_key, amount_fen, fee_fen)?;

    let call_data =
        build_transfer_with_remark_call(&destination_account_id_bytes, amount_fen, &remark)?;

    // 获取链上参数并构建签名载荷
    let result = signing::build_sign_request_from_call_data(
        &signer_public_key,
        &signer_public_key_bytes,
        &call_data,
    )?;

    Ok(TransferSignRequestResult {
        request_json: result.request_json,
        request_id: result.request_id,
        expected_payload_hash: result.expected_payload_hash,
        sign_nonce: result.sign_nonce,
        sign_block_number: result.sign_block_number,
        call_data_hex: format!("0x{}", hex::encode(&call_data)),
        fee_yuan,
    })
}

/// 提交 CitizenWallet 返回的 Cold 签名交易。
// Tauri IPC 必须逐字段对应前端已签名会话，聚合参数会改变公开命令载荷。
#[allow(clippy::too_many_arguments)]
#[tauri::command(rename_all = "snake_case")]
pub fn submit_cold_transfer(
    app: tauri::AppHandle,
    account_id: String,
    request_id: String,
    expected_payload_hash: String,
    call_data_hex: String,
    sign_nonce: u32,
    sign_block_number: u64,
    response_json: String,
) -> Result<TransferSubmitResult, String> {
    let wallet = require_wallet_sign_mode(&app, &account_id, SignMode::Cold)?;
    let call_data_clean = call_data_hex.strip_prefix("0x").unwrap_or(&call_data_hex);
    let call_data = hex::decode(call_data_clean).map_err(|e| format!("call_data 解码失败: {e}"))?;

    let result = signing::verify_and_submit(
        &request_id,
        &wallet.account_id,
        &expected_payload_hash,
        &call_data,
        sign_nonce,
        sign_block_number,
        &response_json,
    )?;

    Ok(TransferSubmitResult {
        tx_hash: result.tx_hash,
    })
}

/// 使用本机 Hot 钱包直接签名并提交转账。
#[tauri::command(rename_all = "snake_case")]
pub async fn submit_hot_transfer(
    app: tauri::AppHandle,
    account_id: String,
    to_ss58_address: String,
    amount_yuan: f64,
    remark: String,
    unlock_password: String,
) -> Result<TransferSubmitResult, String> {
    require_wallet_sign_mode(&app, &account_id, SignMode::Hot)?;
    if let Err(e) = security::append_audit_log(&app, "submit_hot_transfer", "attempt") {
        eprintln!("[审计] submit_hot_transfer attempt 日志写入失败: {e}");
    }

    let unlock = security::ensure_unlock_password(&unlock_password)?;
    device_password::verify_device_login_password(&app, unlock)?;
    drop(unlock_password);

    let app_for_task = app.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        submit_hot_transfer_inner(
            &app_for_task,
            account_id,
            to_ss58_address,
            amount_yuan,
            remark,
        )
    })
    .await
    .map_err(|e| format!("Hot 钱包签名任务失败: {e}"))?;

    match &result {
        Ok(_) => {
            if let Err(e) = security::append_audit_log(&app, "submit_hot_transfer", "success") {
                eprintln!("[审计] submit_hot_transfer success 日志写入失败: {e}");
            }
        }
        Err(err) => {
            if let Err(e) = security::append_audit_log(&app, "submit_hot_transfer", "failed") {
                eprintln!("[审计] submit_hot_transfer failed 日志写入失败: {e}");
            }
            eprintln!("[交易] Hot 钱包签名提交失败: {err}");
        }
    }

    result
}

fn submit_hot_transfer_inner(
    app: &tauri::AppHandle,
    account_id: String,
    to_ss58_address: String,
    amount_yuan: f64,
    remark: String,
) -> Result<TransferSubmitResult, String> {
    // 阻塞任务内再次读取 Hot 事实，避免密码校验期间本机密钥发生变化后继续签名。
    let hot_wallet = require_wallet_sign_mode(app, &account_id, SignMode::Hot)?;
    let to_ss58_address = to_ss58_address.trim().to_string();
    let destination_account_id_bytes = signing::account_id_from_ss58_address(&to_ss58_address)?;
    let destination_account_id = format!("0x{}", hex::encode(destination_account_id_bytes));
    if destination_account_id == hot_wallet.account_id {
        return Err("收款地址不能与矿工热钱包相同".to_string());
    }

    let amount_fen = amount_yuan_to_fen(amount_yuan)?;
    validate_transfer_remark(&remark)?;
    let fee_fen = calculate_transfer_fee(amount_fen);
    ensure_spendable_balance(&hot_wallet.account_id, amount_fen, fee_fen)?;

    // 真正的私钥签名只发生在节点 RPC 内部；一次性令牌避免外部本机 RPC 直接花费矿工余额。
    let auth_token = crate::core::rpc::issue_miner_transfer_token()?;
    let result = rpc::rpc_post(
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
        crate::core::rpc::revoke_miner_transfer_token(&auth_token);
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
fn build_transfer_with_remark_call(
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

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_wallet(fill: u8, sign_mode: SignMode) -> Wallet {
        let account_id_bytes = [fill; 32];
        Wallet {
            name: "测试钱包".to_string(),
            sign_mode,
            ss58_address: signing::account_id_to_ss58(&account_id_bytes).unwrap(),
            account_id: format!("0x{}", hex::encode(account_id_bytes)),
            created_at: 1,
        }
    }

    #[test]
    fn sign_mode_routes_only_to_matching_path() {
        let hot = test_wallet(1, SignMode::Hot);
        let cold = test_wallet(2, SignMode::Cold);

        assert!(require_sign_mode(&hot, SignMode::Hot).is_ok());
        assert!(require_sign_mode(&cold, SignMode::Cold).is_ok());
        assert!(require_sign_mode(&hot, SignMode::Cold).is_err());
        assert!(require_sign_mode(&cold, SignMode::Hot).is_err());
    }

    #[test]
    fn persisted_store_accepts_only_unique_cold_accounts() {
        let cold = test_wallet(3, SignMode::Cold);
        let valid = WalletStore {
            wallets: vec![cold.clone()],
            active_account_id: Some(cold.account_id.clone()),
        };
        assert!(validate_persisted_store(&valid).is_ok());

        let hot = WalletStore {
            wallets: vec![test_wallet(4, SignMode::Hot)],
            active_account_id: None,
        };
        assert!(validate_persisted_store(&hot).is_err());

        let duplicate = WalletStore {
            wallets: vec![cold.clone(), cold],
            active_account_id: None,
        };
        assert!(validate_persisted_store(&duplicate).is_err());
    }

    #[test]
    fn persisted_store_rejects_account_and_address_mismatch() {
        let mut wallet = test_wallet(5, SignMode::Cold);
        wallet.ss58_address = test_wallet(6, SignMode::Cold).ss58_address;
        let store = WalletStore {
            wallets: vec![wallet],
            active_account_id: None,
        };
        assert!(validate_persisted_store(&store).is_err());
    }

    #[test]
    fn compact_u32_single_byte() {
        assert_eq!(encode_compact_u32(0), vec![0x00]);
        assert_eq!(encode_compact_u32(1), vec![0x04]);
        assert_eq!(encode_compact_u32(63), vec![0xfc]);
    }

    #[test]
    fn compact_u32_two_bytes() {
        assert_eq!(encode_compact_u32(64), vec![0x01, 0x01]);
        assert_eq!(encode_compact_u32(16383), vec![0xfd, 0xff]);
    }

    #[test]
    fn transfer_with_remark_call_data_uses_onchain_transaction_pallet() {
        let dest = [0xAAu8; 32];
        let call =
            build_transfer_with_remark_call(&dest, 25_000, "ok").expect("call data should build");

        assert_eq!(&call[0..2], &[0x04, 0x00]);
        assert_eq!(&call[2..34], &dest);
        assert_eq!(&call[34..50], &25_000u128.to_le_bytes());
        assert_eq!(call[50], 0x08); // Compact(2)
        assert_eq!(&call[51..53], b"ok");
    }

    #[test]
    fn transfer_remark_rejects_more_than_99_bytes() {
        let dest = [0xAAu8; 32];
        let err = build_transfer_with_remark_call(&dest, 1, &"a".repeat(100))
            .expect_err("overlong remark should be rejected");
        assert!(err.contains("99"));
    }
}
