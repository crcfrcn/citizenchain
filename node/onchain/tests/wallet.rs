use crate::wallet::decode_store;
use crate::wallet::SignMode;
use crate::wallet::Wallet;
use crate::wallet::WalletStore;

const ACCOUNT_ID: &str = "0x1111111111111111111111111111111111111111111111111111111111111111";

fn cold_wallet() -> Wallet {
    cold_wallet_for_account(ACCOUNT_ID)
}

#[test]
fn sign_mode_serializes_only_hot_and_cold() {
    assert_eq!(
        serde_json::to_string(&crate::wallet::SignMode::Hot).unwrap(),
        "\"hot\""
    );
    assert_eq!(
        serde_json::to_string(&crate::wallet::SignMode::Cold).unwrap(),
        "\"cold\""
    );
    assert!(serde_json::from_str::<SignMode>("\"minerHot\"").is_err());
    assert!(serde_json::from_str::<SignMode>("\"external\"").is_err());
    assert!(serde_json::from_str::<SignMode>("\"local\"").is_err());
}

#[test]
fn wallet_store_uses_account_id_and_required_sign_mode() {
    let store = WalletStore {
        wallets: vec![cold_wallet()],
        active_account_id: Some(ACCOUNT_ID.to_string()),
    };
    let value = serde_json::to_value(&store).unwrap();
    assert_eq!(value["activeAccountId"], ACCOUNT_ID);
    assert_eq!(value["wallets"][0]["signMode"], "cold");
    assert!(value["wallets"][0].get("id").is_none());
}

#[test]
fn wallet_store_rejects_old_or_missing_mode_fields() {
    let old_kind = format!(
        r#"{{"wallets":[{{"name":"旧钱包","kind":"cold","ss58Address":"x","accountId":"{ACCOUNT_ID}","createdAt":1}}],"activeAccountId":null}}"#
    );
    let missing_mode = format!(
        r#"{{"wallets":[{{"name":"旧钱包","ss58Address":"x","accountId":"{ACCOUNT_ID}","createdAt":1}}],"activeAccountId":null}}"#
    );
    let random_id = format!(
        r#"{{"wallets":[{{"id":"legacy","name":"旧钱包","signMode":"cold","ss58Address":"x","accountId":"{ACCOUNT_ID}","createdAt":1}}],"activeAccountId":null}}"#
    );

    assert!(serde_json::from_str::<WalletStore>(&old_kind).is_err());
    assert!(serde_json::from_str::<WalletStore>(&missing_mode).is_err());
    assert!(serde_json::from_str::<WalletStore>(&random_id).is_err());
}

#[test]
fn current_store_round_trips_without_rewriting() {
    let store = WalletStore {
        wallets: vec![cold_wallet()],
        active_account_id: Some(ACCOUNT_ID.to_string()),
    };
    assert_eq!(
        decode_store(&serde_json::to_string(&store).unwrap()).unwrap(),
        store
    );
    assert_eq!(
        decode_store(r#"{"wallets":[],"activeAccountId":null}"#).unwrap(),
        crate::wallet::WalletStore::default()
    );
}

#[test]
fn current_store_rejects_legacy_and_ambiguous_fields() {
    let legacy = format!(
        r#"{{"wallets":[{{"id":"old","name":"旧钱包","kind":"cold","deletable":true,"ss58_address":"x","account_id":"{ACCOUNT_ID}","createdAt":1}}],"activeId":"old"}}"#
    );
    assert!(decode_store(&legacy).is_err());
    let store = WalletStore {
        wallets: vec![cold_wallet()],
        active_account_id: Some(ACCOUNT_ID.to_string()),
    };
    let value = serde_json::to_value(store).unwrap();
    for field in ["signMode", "accountId"] {
        let mut missing = value.clone();
        missing["wallets"][0].as_object_mut().unwrap().remove(field);
        assert!(decode_store(&missing.to_string()).is_err());
    }
    for (field, invalid) in [("signMode", "minerHot"), ("accountId", "0x11")] {
        let mut malformed = value.clone();
        malformed["wallets"][0][field] = serde_json::Value::String(invalid.to_string());
        assert!(decode_store(&malformed.to_string()).is_err());
    }
    for field in ["ss58Address", "createdAt"] {
        let mut shadow = value.clone();
        shadow["wallets"][0]
            .as_object_mut()
            .unwrap()
            .insert(field.into(), serde_json::json!("旧副本"));
        assert!(decode_store(&shadow.to_string()).is_err());
    }
    for field in ["activeId", "id", "extra"] {
        let mut unknown = value.clone();
        unknown
            .as_object_mut()
            .unwrap()
            .insert(field.to_string(), serde_json::Value::Null);
        assert!(decode_store(&unknown.to_string()).is_err());
    }
}

#[test]
fn current_store_rejects_duplicates_and_inconsistent_accounts() {
    let store = WalletStore {
        wallets: vec![cold_wallet()],
        active_account_id: Some(ACCOUNT_ID.to_string()),
    };
    let mut duplicate = store.clone();
    duplicate.wallets.push(cold_wallet());
    assert!(decode_store(&serde_json::to_string(&duplicate).unwrap()).is_err());
    let mut mismatched = store.clone();
    mismatched.wallets[0].account_id = "0x11".to_string();
    assert!(decode_store(&serde_json::to_string(&mismatched).unwrap()).is_err());
    let mut dangling = store;
    dangling.active_account_id =
        Some("0x2222222222222222222222222222222222222222222222222222222222222222".to_string());
    // Cold 文件的活动标识可能指向动态 Hot；读取时只校验规范格式并原样保留。
    assert_eq!(
        decode_store(&serde_json::to_string(&dangling).unwrap()).unwrap(),
        dangling
    );
    for invalid in [
        "0x1234",
        "0xAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
        "not-an-account",
    ] {
        let mut invalid_active = dangling.clone();
        invalid_active.active_account_id = Some(invalid.to_string());
        assert!(decode_store(&serde_json::to_string(&invalid_active).unwrap()).is_err());
    }
}

fn cold_wallet_for_account(account_id: &str) -> Wallet {
    Wallet {
        name: "测试冷钱包".to_string(),
        sign_mode: crate::wallet::SignMode::Cold,
        account_id: account_id.to_string(),
    }
}
