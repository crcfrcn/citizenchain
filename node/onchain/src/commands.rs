//! 提供桌面钱包增删、选择及链上转账准备和提交命令。

use crate::wallet::Wallet;

use crate::wallet::WalletStore;

use crate::transfer::local_miner_wallet;
use crate::transfer::normalize_account_id;
use crate::transfer::require_wallet_sign_mode;
use crate::transfer::submit_hot_transfer_inner;
use crate::transfer::validate_persisted_store;
use crate::transfer::wallet_store_for_frontend;
use crate::types::TransferSignRequestResult;
use crate::types::TransferSubmitResult;
use std::sync::Mutex;
use codec::Encode;
use crate::types::{TransferMode, TransferItem};

// ──── 钱包管理命令 ────

static WALLET_UPDATE: Mutex<()> = Mutex::new(());

#[tauri::command]
pub fn get_wallets(app: tauri::AppHandle) -> Result<WalletStore, String> {
    let store = crate::wallet::load(&app)?;
    wallet_store_for_frontend(&app, store)
}

#[tauri::command]
pub async fn add_wallet(
    app: tauri::AppHandle,
    name: String,
    ss58_address: String,
) -> Result<Wallet, String> {
    let name = name.trim().to_string();
    crate::wallet::validate_new_wallet_name(&name)?;
    let account_id_bytes = tauri::async_runtime::spawn_blocking(move || {
        nodeclient::signing::resolve_recipient_account_id(&ss58_address)
    })
    .await
    .map_err(|_| "钱包地址解析任务失败".to_string())??;
    let account_id = format!("0x{}", hex::encode(account_id_bytes));

    let _update = WALLET_UPDATE.lock().map_err(|_| "钱包配置正在更新")?;
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
    let _update = WALLET_UPDATE.lock().map_err(|_| "钱包配置正在更新")?;
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
    let _update = WALLET_UPDATE.lock().map_err(|_| "钱包配置正在更新")?;
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
    // 所有钱包展示链上总余额；转账可支配金额由独立预检查及链上执行校验。
    let account_id = normalize_account_id(&account_id)?;
    match nodeinstitution::balance::fetch_balance(&account_id)? {
        Some(fen) => Ok(Some(fen.to_string())),
        // 成功读取但链上尚无账户记录时，余额为零；RPC/解码错误由 ? 保留。
        None => Ok(Some("0".to_string())),
    }
}

// ──── 转账命令 ────

/// 为 Cold 钱包构建 Onchain::transfer_with_remark 签名请求。
///
/// 返回 QR 签名请求 JSON，前端显示 QR 码供离线设备扫码签名。
#[tauri::command]
pub fn build_cold_transfer_request(
    app: tauri::AppHandle,
    account_id: String,
    mode: TransferMode,
    transfers: Vec<TransferItem>,
) -> Result<TransferSignRequestResult, String> {
    let wallet = require_wallet_sign_mode(&app, &account_id, crate::wallet::SignMode::Cold)?;
    // sr25519 AccountId32 与签名公钥字节相同；签名者只取自后端钱包事实。
    let signer_public_key = wallet.account_id;
    let signer_public_key_bytes = hex::decode(signer_public_key.trim_start_matches("0x"))
        .map_err(|e| format!("发送方公钥解码失败: {e}"))?;

    let (transfers, fee_fen) = crate::transfer::normalize_transfers(&signer_public_key, mode, transfers)?;
    let call_data = crate::transfer::build_transfer_call(mode, &transfers)?.encode();
    let scan_from = crate::transfer::scan_from()?;

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
        fee_fen: fee_fen.to_string(),
        scan_from,
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

    let scan_from = crate::transfer::scan_from()?;
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
        scan_from,
    })
}

/// 使用本机 Hot 钱包直接签名并提交转账。
#[tauri::command]
pub async fn submit_hot_transfer(
    app: tauri::AppHandle,
    account_id: String,
    mode: TransferMode,
    transfers: Vec<TransferItem>,
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
            mode,
            transfers,
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

#[tauri::command]
pub fn join_qr_frames(frames: Vec<String>) -> Result<Option<String>, String> {
    nodeclient::signing::join_qr_frames(frames)
}

/// 有界读取最终结果，不签名、不自动重发。
#[tauri::command]
pub fn get_transfer_result(tx_hash: String, cursor: u32) -> Result<serde_json::Value, String> {
    nodedesktop::rpc::rpc_post("transaction_getTransferResult", serde_json::json!([tx_hash, cursor, 16]), std::time::Duration::from_secs(45), nodedesktop::constants::RPC_RESPONSE_LIMIT_SMALL)
}

/// 仅为当前钱包目录查询本机最终链历史，不签名、不写入本地交易数据。
#[tauri::command]
pub async fn get_transaction_history(
    app: tauri::AppHandle,
    cursor: Option<crate::types::TransactionCursor>,
) -> Result<crate::types::TransactionHistory, String> {
    if !nodecontrol::identity::current_status(&app)?.running {
        return Err("节点未运行".into());
    }
    let store = wallet_store_for_frontend(&app, crate::wallet::load(&app)?)?;
    let accounts = store.wallets.into_iter().map(|wallet| wallet.account_id).collect::<Vec<_>>();
    tauri::async_runtime::spawn_blocking(move || {
        let value = nodedesktop::rpc::rpc_post(
            "transaction_getHistory", serde_json::json!([accounts, cursor]),
            std::time::Duration::from_secs(45), nodedesktop::constants::RPC_RESPONSE_LIMIT_LARGE,
        )?;
        serde_json::from_value(value).map_err(|error| format!("交易记录响应无效：{error}"))
    }).await.map_err(|error| error.to_string())?
}

/// 只读当前Runtime能力，不以节点编译期版本代替链上部署状态。
#[tauri::command]
pub fn get_transfer_policy() -> Result<serde_json::Value,String> {
    nodedesktop::rpc::rpc_post("transaction_transferPolicy",serde_json::json!([]),std::time::Duration::from_secs(15),nodedesktop::constants::RPC_RESPONSE_LIMIT_SMALL)
}

/// 签名前只读解析实际收款账户与整分报价，冷热钱包共用，不签名或扣款。
#[tauri::command]
pub fn prepare_transfer(app:tauri::AppHandle,account_id:String,mode:TransferMode,transfers:Vec<TransferItem>)->Result<serde_json::Value,String>{
    let account=normalize_account_id(&account_id)?;
    let store=wallet_store_for_frontend(&app,crate::wallet::load(&app)?)?;
    if !store.wallets.iter().any(|wallet|wallet.account_id==account){return Err("付款钱包不存在".into());}
    let (transfers,fee)=crate::transfer::normalize_transfers(&account,mode,transfers)?;
    Ok(serde_json::json!({"transfers":transfers,"feeFen":fee.to_string()}))
}
