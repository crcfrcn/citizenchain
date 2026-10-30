use crate::wallet::Wallet;

use crate::wallet::WalletStore;

use crate::transfer::amount_yuan_to_fen;
use crate::transfer::build_transfer_with_remark_call;
use crate::transfer::calculate_transfer_fee;
use crate::transfer::ensure_spendable_balance;
use crate::transfer::local_miner_wallet;
use crate::transfer::normalize_account_id;
use crate::transfer::require_wallet_sign_mode;
use crate::transfer::submit_hot_transfer_inner;
use crate::transfer::validate_persisted_store;
use crate::transfer::validate_transfer_remark;
use crate::transfer::wallet_store_for_frontend;
use crate::types::TransferSignRequestResult;
use crate::types::TransferSubmitResult;

// ──── 钱包管理命令 ────

#[tauri::command]
pub fn get_wallets(app: tauri::AppHandle) -> Result<WalletStore, String> {
    let store = crate::wallet::load(&app)?;
    wallet_store_for_frontend(&app, store)
}

#[tauri::command]
pub fn add_wallet(
    app: tauri::AppHandle,
    name: String,
    ss58_address: String,
) -> Result<Wallet, String> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err("钱包名称不能为空".to_string());
    }
    let account_id_bytes = nodeclient::signing::account_id_from_ss58_address(ss58_address.trim())?;
    let account_id = format!("0x{}", hex::encode(account_id_bytes));

    let mut store = crate::wallet::load(&app)?;
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
        sign_mode: crate::wallet::SignMode::Cold,
        account_id,
    };

    store.wallets.push(wallet.clone());
    if store.active_account_id.is_none() && miner_wallet.is_none() {
        store.active_account_id = Some(wallet.account_id.clone());
    }
    crate::wallet::save(&app, &store)?;
    Ok(wallet)
}

#[tauri::command]
pub fn remove_wallet(app: tauri::AppHandle, account_id: String) -> Result<WalletStore, String> {
    let account_id = normalize_account_id(&account_id)?;
    let miner_wallet = local_miner_wallet(&app)?;
    if miner_wallet
        .as_ref()
        .is_some_and(|wallet| wallet.account_id == account_id)
    {
        return Err("矿工热钱包不能删除".to_string());
    }
    let mut store = crate::wallet::load(&app)?;
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
    crate::wallet::save(&app, &store)?;
    wallet_store_for_frontend(&app, store)
}

#[tauri::command]
pub fn set_active_wallet(app: tauri::AppHandle, account_id: String) -> Result<WalletStore, String> {
    let account_id = normalize_account_id(&account_id)?;
    let mut store = crate::wallet::load(&app)?;
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
    crate::wallet::save(&app, &store)?;
    wallet_store_for_frontend(&app, store)
}

#[tauri::command]
pub fn get_wallet_balance(account_id: String) -> Result<Option<String>, String> {
    let account_id = normalize_account_id(&account_id)?;
    match nodeinstitution::balance::fetch_balance(&account_id)? {
        Some(fen) => Ok(Some(fen.to_string())),
        // 成功读取但链上尚无账户记录时，余额为零；RPC/解码错误由 ? 保留。
        None => Ok(Some("0".to_string())),
    }
}

// ──── 转账命令 ────

/// 为 Cold 钱包构建 OnchainTransaction::transfer_with_remark 签名请求。
///
/// 返回 QR 签名请求 JSON，前端显示 QR 码供离线设备扫码签名。
#[tauri::command]
pub fn build_cold_transfer_request(
    app: tauri::AppHandle,
    account_id: String,
    to_ss58_address: String,
    amount_yuan: f64,
    remark: String,
) -> Result<TransferSignRequestResult, String> {
    let wallet = require_wallet_sign_mode(&app, &account_id, crate::wallet::SignMode::Cold)?;
    // sr25519 AccountId32 与签名公钥字节相同；签名者只取自后端钱包事实。
    let signer_public_key = wallet.account_id;
    let signer_public_key_bytes = hex::decode(signer_public_key.trim_start_matches("0x"))
        .map_err(|e| format!("发送方公钥解码失败: {e}"))?;

    // 校验收款地址
    let destination_account_id_bytes =
        nodeclient::signing::account_id_from_ss58_address(&to_ss58_address)?;
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
    let result = nodeclient::signing::build_sign_request_from_call_data(
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
#[tauri::command]
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
    let wallet = require_wallet_sign_mode(&app, &account_id, crate::wallet::SignMode::Cold)?;
    let call_data_clean = call_data_hex.strip_prefix("0x").unwrap_or(&call_data_hex);
    let call_data = hex::decode(call_data_clean).map_err(|e| format!("call_data 解码失败: {e}"))?;

    let result = nodeclient::signing::verify_and_submit(
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
#[tauri::command]
pub async fn submit_hot_transfer(
    app: tauri::AppHandle,
    account_id: String,
    to_ss58_address: String,
    amount_yuan: f64,
    remark: String,
    unlock_password: String,
) -> Result<TransferSubmitResult, String> {
    require_wallet_sign_mode(&app, &account_id, crate::wallet::SignMode::Hot)?;
    if let Err(e) = nodedesktop::security::append_audit_log(&app, "submit_hot_transfer", "attempt")
    {
        eprintln!("[审计] submit_hot_transfer attempt 日志写入失败: {e}");
    }

    let unlock = nodedesktop::security::ensure_unlock_password(&unlock_password)?;
    nodecontrol::password::verify_device_login_password(&app, unlock)?;
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
            if let Err(e) =
                nodedesktop::security::append_audit_log(&app, "submit_hot_transfer", "success")
            {
                eprintln!("[审计] submit_hot_transfer success 日志写入失败: {e}");
            }
        }
        Err(err) => {
            if let Err(e) =
                nodedesktop::security::append_audit_log(&app, "submit_hot_transfer", "failed")
            {
                eprintln!("[审计] submit_hot_transfer failed 日志写入失败: {e}");
            }
            eprintln!("[交易] Hot 钱包签名提交失败: {err}");
        }
    }

    result
}
