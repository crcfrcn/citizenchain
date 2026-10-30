use super::*;

use crate::ledger::OffchainLedger;
use crate::listener::convert_event;
use offchain::pallet::Event as PalletEvent;
use std::fs;
use std::sync::Arc;

fn mk() -> (Arc<OffchainLedger>, Vec<u8>) {
    let tmp = std::env::temp_dir().join("offchain_event_test");
    let _ = fs::remove_dir_all(&tmp);
    let bank: Vec<u8> = b"ZS001-PRB08-233384677-2026".to_vec();
    (Arc::new(crate::ledger::OffchainLedger::new(&tmp)), bank)
}

#[test]
fn deposited_event_updates_own_bank_ledger() {
    let (ledger, bank) = mk();
    let listener = crate::listener::EventListener::new(ledger.clone(), bank.clone());
    let user = AccountId32::new([1u8; 32]);
    listener.handle(crate::listener::OffchainChainEvent::Deposited {
        user: user.clone(),
        bank_cid: bank.clone(),
        amount: 500,
    });
    assert_eq!(ledger.available_balance(&user), 500);
}

#[test]
fn deposited_event_ignored_for_other_bank() {
    let (ledger, bank) = mk();
    let listener = crate::listener::EventListener::new(ledger.clone(), bank);
    let other_bank: Vec<u8> = b"AH001-PRB0X-111111111-2026".to_vec();
    let user = AccountId32::new([1u8; 32]);
    listener.handle(crate::listener::OffchainChainEvent::Deposited {
        user: user.clone(),
        bank_cid: other_bank,
        amount: 500,
    });
    assert_eq!(ledger.available_balance(&user), 0);
}

#[test]
fn withdrawn_decreases_confirmed() {
    let (ledger, bank) = mk();
    let listener = crate::listener::EventListener::new(ledger.clone(), bank.clone());
    let user = AccountId32::new([2u8; 32]);
    listener.handle(crate::listener::OffchainChainEvent::Deposited {
        user: user.clone(),
        bank_cid: bank.clone(),
        amount: 1000,
    });
    listener.handle(crate::listener::OffchainChainEvent::Withdrawn {
        user: user.clone(),
        bank_cid: bank,
        amount: 300,
    });
    assert_eq!(ledger.available_balance(&user), 700);
}

#[test]
fn convert_event_deposited() {
    let user = AccountId32::new([1u8; 32]);
    let bank: Vec<u8> = b"ZS001-PRB08-233384677-2026".to_vec();
    let ev = runtime::RuntimeEvent::OffchainTransaction(PalletEvent::Deposited {
        user: user.clone(),
        bank_cid: bank.clone().try_into().unwrap(),
        amount: 100,
    });
    match convert_event(ev) {
        Some(crate::listener::OffchainChainEvent::Deposited {
            user: u,
            bank_cid: b,
            amount,
        }) => {
            assert_eq!(u, user);
            assert_eq!(b, bank);
            assert_eq!(amount, 100);
        }
        other => panic!("unexpected: {other:?}"),
    }
}

#[test]
fn convert_event_payment_settled_maps_field_names() {
    // runtime pallet 字段 `transfer_amount` / `fee_amount` 要映射到 node 侧
    // 简化结构 `amount` / `fee`,顺序和语义都不能丢。
    let payer = AccountId32::new([1u8; 32]);
    let payer_bank: Vec<u8> = b"GD001-PRB0T-239565809-2026".to_vec();
    let recipient = AccountId32::new([2u8; 32]);
    let recipient_bank: Vec<u8> = b"AH001-PRB0X-111111111-2026".to_vec();
    let tx_id = H256::repeat_byte(7);
    let ev = runtime::RuntimeEvent::OffchainTransaction(PalletEvent::PaymentSettled {
        tx_id,
        payer_account_id: payer.clone(),
        payer_bank_cid: payer_bank.clone().try_into().unwrap(),
        recipient_account_id: recipient.clone(),
        recipient_bank_cid: recipient_bank.clone().try_into().unwrap(),
        transfer_amount: 9_800,
        fee_amount: 200,
    });
    match convert_event(ev) {
        Some(crate::listener::OffchainChainEvent::PaymentSettled {
            tx_id: t,
            payer: p,
            payer_bank_cid: pb,
            recipient: r,
            recipient_bank_cid: rb,
            amount,
            fee,
        }) => {
            assert_eq!(t, tx_id);
            assert_eq!(p, payer);
            assert_eq!(pb, payer_bank);
            assert_eq!(r, recipient);
            assert_eq!(rb, recipient_bank);
            assert_eq!(amount, 9_800);
            assert_eq!(fee, 200);
        }
        other => panic!("unexpected: {other:?}"),
    }
}

#[test]
fn convert_event_non_offchain_returns_none() {
    // `System::Remarked` 这类非本 pallet 的事件必须返回 None,
    // 否则会把所有系统事件都喂给 ledger。
    let ev = runtime::RuntimeEvent::System(frame_system::Event::CodeUpdated {
        hash: Default::default(),
    });
    assert!(convert_event(ev).is_none());
}
