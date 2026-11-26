//! 验证转账金额和余额边界，以及冷签会话篡改、过期和重放拒绝。

use crate::transfer::parse_amount_fen;
use crate::transfer::build_transfer_call;
use crate::types::{TransferMode,TransferItem};
use codec::Encode;
use crate::transfer::calculate_transfer_fee;

use crate::transfer::ensure_spendable_balance;
use crate::transfer::require_sign_mode;
use crate::transfer::validate_persisted_store;
use crate::transfer::EXISTENTIAL_DEPOSIT_FEN;
use crate::wallet::SignMode;
use crate::wallet::Wallet;
use crate::wallet::WalletStore;

fn transfer_call(dest:&[u8],amount:u128,remark:&str)->Result<Vec<u8>,String>{
    let address=nodeclient::signing::account_id_to_ss58(dest)?;
    build_transfer_call(TransferMode::Transfer,&[TransferItem{to_address:address,amount_fen:amount.to_string(),remark:remark.into()}]).map(|call|call.encode())
}
#[test]
fn transfer_amount_is_exact_positive_integer_fen(){
    for value in ["","0","01","-1","0.1","1e3","NaN","340282366920938463463374607431768211456"] {assert!(parse_amount_fen(value).is_err());}
    assert_eq!(parse_amount_fen("10499").unwrap(),10499);
    assert_eq!(parse_amount_fen(&u128::MAX.to_string()).unwrap(),u128::MAX);
}

#[test]
fn transfer_balance_overflow_is_rejected_before_rpc() {
    // 空账户不具备RPC查询条件；准确的溢出错误证明先拒绝本地算术溢出。
    assert_eq!(
        ensure_spendable_balance("", u128::MAX, 1),
        Err("转账金额与手续费之和超出范围".to_string())
    );
    assert_eq!(
        ensure_spendable_balance("", u128::MAX - EXISTENTIAL_DEPOSIT_FEN, 1),
        Err("转账所需余额超出范围".to_string())
    );
}

#[test]
fn cold_transfer_rejects_changed_expired_and_consumed_sessions_before_rpc() {
    let signer = format!("0x{}", "11".repeat(32));
    let payload_hash = "11".repeat(32);
    let call = transfer_call(&[0xAA; 32], 10_500, "转账").unwrap();
    // 这些已登记会话必须在接触RPC前拒绝；测试不连接生产节点或读取私钥。
    for scenario in [
        "call",
        "nonce",
        "hash",
        "signer",
        "expiry",
        "response-expiry",
    ] {
        let request_id = nodeclient::signing::generate_request_id("transfer-test");
        let now = nodeclient::signing::now_secs().unwrap();
        let expires_at = if scenario == "expiry" {
            now - 1
        } else {
            now + 90
        };
        nodeclient::session::remember_chain_sign_request_session(
            &request_id,
            &signer,
            &call,
            &payload_hash,
            &"22".repeat(32),
            7,
            expires_at,
        )
        .unwrap();
        let response = serde_json::json!({
                "p":nodeclient::signing::QR_V1,"k":nodeclient::signing::QR_KIND_SIGN_RESPONSE,"i":request_id,
                "e":if scenario == "response-expiry" { expires_at + 1 } else { expires_at },
                "b":{"u":nodeclient::signing::public_key_b64(&[0x11;32]).unwrap(),
                    "s":qr_protocol::bytes_to_b64(&[0;64])}
            })
            .to_string();
        let mut supplied_call = call.clone();
        if scenario == "call" {
            supplied_call.push(0);
        }
        let supplied_signer = if scenario == "signer" {
            format!("0x{}", "33".repeat(32))
        } else {
            signer.clone()
        };
        let supplied_hash = if scenario == "hash" {
            "33".repeat(32)
        } else {
            payload_hash.clone()
        };
        let result = nodeclient::signing::verify_and_submit(
            &request_id,
            &supplied_signer,
            &supplied_hash,
            &supplied_call,
            if scenario == "nonce" { 8 } else { 7 },
            0,
            &response,
        );
        assert!(result.is_err(), "被修改或过期的会话必须拒绝: {scenario}");
        let replay = nodeclient::signing::verify_and_submit(
            &request_id,
            &signer,
            &payload_hash,
            &call,
            7,
            0,
            &response,
        )
        .unwrap_err();
        assert!(replay.contains("未找到本地签名 session"));
    }
}

fn test_wallet(fill: u8, sign_mode: SignMode) -> Wallet {
    let account_id_bytes = [fill; 32];
    Wallet {
        name: "测试钱包".to_string(),
        sign_mode,
        account_id: format!("0x{}", hex::encode(account_id_bytes)),
    }
}

#[test]
fn sign_mode_routes_only_to_matching_path() {
    let hot = test_wallet(1, crate::wallet::SignMode::Hot);
    let cold = test_wallet(2, crate::wallet::SignMode::Cold);

    assert!(require_sign_mode(&hot, crate::wallet::SignMode::Hot).is_ok());
    assert!(require_sign_mode(&cold, crate::wallet::SignMode::Cold).is_ok());
    assert!(require_sign_mode(&hot, crate::wallet::SignMode::Cold).is_err());
    assert!(require_sign_mode(&cold, crate::wallet::SignMode::Hot).is_err());
}

#[test]
fn persisted_store_accepts_only_unique_cold_accounts() {
    let cold = test_wallet(3, crate::wallet::SignMode::Cold);
    let valid = WalletStore {
        wallets: vec![cold.clone()],
        active_account_id: Some(cold.account_id.clone()),
    };
    assert!(validate_persisted_store(&valid).is_ok());

    let hot = WalletStore {
        wallets: vec![test_wallet(4, crate::wallet::SignMode::Hot)],
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
fn persisted_store_rejects_invalid_account_identity() {
    let mut wallet = test_wallet(5, crate::wallet::SignMode::Cold);
    wallet.account_id = "0x11".to_string();
    let store = WalletStore {
        wallets: vec![wallet],
        active_account_id: None,
    };
    assert!(validate_persisted_store(&store).is_err());
}

#[test]
fn compact_u32_single_byte() {
    assert_eq!(codec::Compact(0u32).encode(), vec![0x00]);
    assert_eq!(codec::Compact(1u32).encode(), vec![0x04]);
    assert_eq!(codec::Compact(63u32).encode(), vec![0xfc]);
}

#[test]
fn compact_u32_two_bytes() {
    assert_eq!(codec::Compact(64u32).encode(), vec![0x01, 0x01]);
    assert_eq!(codec::Compact(16383u32).encode(), vec![0xfd, 0xff]);
}

#[test]
fn transfer_with_remark_call_data_uses_onchain_transaction_pallet() {
    let dest = [0xAAu8; 32];
    let call =
        transfer_call(&dest, 25_000, "ok").expect("call data should build");

    assert_eq!(&call[0..2], &[0x04, 0x00]);
    assert_eq!(&call[2..34], &dest);
    assert_eq!(&call[34..50], &25_000u128.to_le_bytes());
    assert_eq!(call[50], 0x08); // Compact(2)
    assert_eq!(&call[51..53], b"ok");
}

#[test]
fn transfer_remark_rejects_more_than_99_bytes() {
    let dest = [0xAAu8; 32];
    let err = transfer_call(&dest, 1, &"a".repeat(100))
        .expect_err("overlong remark should be rejected");
    assert!(err.contains("99"));
}

#[test]
fn ordinary_transfer_remark_adds_the_shared_fee_once() {
    for (amount, base) in [
        (100, 10),
        (10_499, 10),
        (10_500, 11),
        (100_000, 100),
        (
            u128::MAX,
            primitives::fee_policy::calculate_onchain_fee(u128::MAX),
        ),
    ] {
        assert_eq!(calculate_transfer_fee(amount, ""), base);
        for remark in [
            "a".to_string(),
            "中".to_string(),
            " ".to_string(),
            "\n".to_string(),
            "a".repeat(99),
        ] {
            assert_eq!(
                calculate_transfer_fee(amount, &remark),
                base + primitives::fee_policy::FLAT_FEE
            );
        }
    }
}

#[test]
fn transfer_checks_spendable_amount_including_fee_instead_of_display_total() {
    // 总资产1200分，其中200分reserved、800分frozen；实际只能再支出400分。
    assert!(crate::transfer::ensure_balance(400, 400, 11).is_ok());
    let error = crate::transfer::ensure_balance(400, 401, 11).unwrap_err();
    assert!(error.contains("可用 4.00 元"));
    assert!(error.contains("需要 4.01 元"));
    assert!(crate::transfer::ensure_balance(0, 1, 0).is_err());
}


#[test]
fn batch_resources_are_checked_before_address_resolution_and_single_is_not_guessed() {
    let item=TransferItem{to_address:nodeclient::signing::account_id_to_ss58(&[0xAA;32]).unwrap(),amount_fen:"100".into(),remark:String::new()};
    let call=build_transfer_call(TransferMode::ForceBatch,&[item.clone()]).unwrap();
    assert_eq!(&call.encode()[..2],&[27,4]);
    assert!(build_transfer_call(TransferMode::Transfer,&[item.clone(),item.clone()]).is_err());
    let excessive=vec![item;runtime::configs::batch_calls_limit() as usize];
    assert!(build_transfer_call(TransferMode::ForceBatch,&excessive).unwrap_err().contains("资源限制"));
}
