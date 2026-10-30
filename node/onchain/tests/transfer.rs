use crate::transfer::amount_yuan_to_fen;
use crate::transfer::build_transfer_with_remark_call;
use crate::transfer::calculate_transfer_fee;
use crate::transfer::encode_compact_u32;
use crate::transfer::ensure_spendable_balance;
use crate::transfer::require_sign_mode;
use crate::transfer::validate_persisted_store;
use crate::transfer::EXISTENTIAL_DEPOSIT_FEN;
use crate::wallet::SignMode;
use crate::wallet::Wallet;
use crate::wallet::WalletStore;

#[test]
fn transfer_amount_rejects_nonfinite_and_below_one_fen() {
    for amount in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -1.0, 0.0, 0.009] {
        assert!(amount_yuan_to_fen(amount).is_err());
    }
    assert_eq!(amount_yuan_to_fen(0.01).unwrap(), 1);
}

#[test]
fn transfer_amount_rejects_finite_values_outside_u128() {
    for amount in [1.0e40, f64::MAX, (u128::MAX as f64) / 100.0] {
        assert!(amount.is_finite());
        assert_eq!(
            amount_yuan_to_fen(amount),
            Err("转账金额超出允许范围".to_string())
        );
    }
    // 金额边界拒绝不引入新的业务上限，原有普通金额仍按分舍入。
    assert_eq!(amount_yuan_to_fen(104.99).unwrap(), 10_499);
    assert!(amount_yuan_to_fen(1.0e30).is_ok());
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
    let call = build_transfer_with_remark_call(&[0xAA; 32], 10_500, "转账").unwrap();
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
