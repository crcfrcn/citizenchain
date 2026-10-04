//! 链上事件 → 清算行本地账本的同步器。
//!
//!
//! - 本模块监听 `offchain_transaction` pallet 的事件:
//!     - `Deposited { user, bank, amount }`     → `ledger.on_deposited`
//!     - `Withdrawn { user, bank, amount }`     → `ledger.on_withdrawn`
//!     - `PaymentSettled { payer, recipient, amount, fee, ... }`
//!       → `ledger.on_payment_settled`(packer 上链后,pending 正式清理)
//!     - `BankBound / BankSwitched`(仅日志)
//! - 接入真实 `sc-client-api::BlockchainEvents` 订阅:
//!     - `run(client)`:订阅 `import_notification_stream`,每个新块读 `System::Events`
//!       → SCALE 解码 `Vec<EventRecord<runtime::RuntimeEvent, H256>>`
//!       → `convert_event` 过滤本 pallet 事件 → `handle` 分发到 ledger
//! - 与 `packer` 组合形成闭环:packer 提交 extrinsic 上链 → runtime 执行后发
//!   `PaymentSettled` 事件 → 本监听器收到 → `ledger.on_payment_settled` 清理 pending。

use codec::Decode;
use futures::StreamExt;
use sc_client_api::{BlockchainEvents, StorageProvider};
use sp_core::H256;
use sp_runtime::AccountId32;
use sp_storage::StorageKey;
use std::sync::Arc;

use citizenchain as runtime;

use crate::core::service::FullClient;

use super::super::ledger::OffchainLedger;

/// 抽象的链上事件。
#[derive(Clone, Debug)]
pub enum OffchainChainEvent {
    /// `offchain::Event::Deposited`
    Deposited {
        user: AccountId32,
        bank_cid: Vec<u8>,
        amount: u128,
    },
    /// `offchain::Event::Withdrawn`
    Withdrawn {
        user: AccountId32,
        bank_cid: Vec<u8>,
        amount: u128,
    },
    /// `offchain::Event::PaymentSettled`
    PaymentSettled {
        tx_id: H256,
        payer: AccountId32,
        payer_bank_cid: Vec<u8>,
        recipient: AccountId32,
        recipient_bank_cid: Vec<u8>,
        amount: u128,
        fee: u128,
    },
    /// 账户绑定 / 切换(仅日志)
    BankBound {
        user: AccountId32,
        bank_cid: Vec<u8>,
    },
    BankSwitched {
        user: AccountId32,
        old_bank_cid: Vec<u8>,
        new_bank_cid: Vec<u8>,
    },
}

/// 事件分发器:把解码后的链上事件喂给本地 ledger。
pub struct EventListener {
    ledger: Arc<OffchainLedger>,
    /// 本节点负责的清算行 CID(用于过滤:只处理与本清算行相关的事件)。
    my_bank_cid: Vec<u8>,
}

impl EventListener {
    pub fn new(ledger: Arc<OffchainLedger>, my_bank_cid: Vec<u8>) -> Self {
        Self {
            ledger,
            my_bank_cid,
        }
    }

    /// 单条事件处理入口。
    pub fn handle(&self, ev: OffchainChainEvent) {
        match ev {
            OffchainChainEvent::Deposited {
                user,
                bank_cid,
                amount,
            } => {
                if bank_cid == self.my_bank_cid {
                    self.ledger.on_deposited(&user, amount);
                }
            }
            OffchainChainEvent::Withdrawn {
                user,
                bank_cid,
                amount,
            } => {
                if bank_cid == self.my_bank_cid {
                    self.ledger.on_withdrawn(&user, amount);
                }
            }
            OffchainChainEvent::PaymentSettled {
                tx_id,
                payer,
                payer_bank_cid,
                recipient,
                recipient_bank_cid,
                amount,
                fee,
            } => {
                // 付款方 / 收款方任意一方在本清算行就要处理(跨行时只动其中一侧)。
                // ledger.on_payment_settled 内部用 `my_bank_cid` 过滤,避免跨行 ghost
                // account。
                if payer_bank_cid == self.my_bank_cid || recipient_bank_cid == self.my_bank_cid {
                    self.ledger.on_payment_settled(
                        tx_id,
                        &payer,
                        &payer_bank_cid,
                        &recipient,
                        &recipient_bank_cid,
                        &self.my_bank_cid,
                        amount,
                        fee,
                    );
                }
            }
            OffchainChainEvent::BankBound { user, bank_cid } => {
                if bank_cid == self.my_bank_cid {
                    log::info!("[OffchainListener] 新 L3 绑定本清算行:user={user:?}");
                }
            }
            OffchainChainEvent::BankSwitched {
                user,
                old_bank_cid,
                new_bank_cid,
            } => {
                if old_bank_cid == self.my_bank_cid {
                    log::info!("[OffchainListener] L3 从本清算行切走:user={user:?}");
                } else if new_bank_cid == self.my_bank_cid {
                    log::info!("[OffchainListener] L3 切到本清算行:user={user:?}");
                }
            }
        }
    }

    /// 订阅链上 import 通知,每个新块读 `System::Events` 解码分发。
    ///
    /// 本函数为 `async` 长循环,调用方应 `task_manager.spawn_handle().spawn(...)`
    /// 启动。订阅 `import_notification_stream`(而非 finality)以降低扫码支付
    /// 的端到端延迟;runtime 已通过防重放机制避免 reorg 时重复处理。
    pub async fn run(self: Arc<Self>, client: Arc<FullClient>) {
        let mut stream = client.import_notification_stream();
        log::info!("[EventListener] 开始订阅 offchain_transaction 事件");
        while let Some(notification) = stream.next().await {
            // 只处理"被纳入主链"的块(避免 re-import 重复分发)
            if !notification.is_new_best {
                continue;
            }
            if let Err(e) = self.process_block(client.as_ref(), notification.hash) {
                log::warn!("[EventListener] 处理块 {:?} 失败:{e}", notification.hash);
            }
        }
        log::warn!("[EventListener] import notification 订阅已结束");
    }

    /// 读取指定块的 `System::Events` storage → 解码 → 过滤分发。
    fn process_block(&self, client: &FullClient, block_hash: H256) -> Result<(), String> {
        let raw = client
            .storage(block_hash, &system_events_storage_key())
            .map_err(|e| format!("storage 读取失败:{e}"))?;
        let Some(data) = raw else {
            return Ok(());
        };

        type EventRecord = frame_system::EventRecord<runtime::RuntimeEvent, H256>;
        let records: Vec<EventRecord> =
            Decode::decode(&mut &data.0[..]).map_err(|e| format!("events 解码失败:{e}"))?;

        for record in records {
            if let Some(ev) = convert_event(record.event) {
                self.handle(ev);
            }
        }
        Ok(())
    }
}

/// 构造 `System::Events` 的 storage key:`twox_128("System") ++ twox_128("Events")`。
fn system_events_storage_key() -> StorageKey {
    let mut k = Vec::with_capacity(32);
    k.extend_from_slice(&sp_io::hashing::twox_128(b"System"));
    k.extend_from_slice(&sp_io::hashing::twox_128(b"Events"));
    StorageKey(k)
}

/// 从 `runtime::RuntimeEvent` 过滤出 `offchain_transaction` 的本地事件。
///
/// 返回 `None` 表示该事件不是本 pallet 的事件(或是本 pallet 的其他事件:
/// 费率治理 / 批次级别等在本步不触发 ledger 分发的事件)。
pub fn convert_event(ev: runtime::RuntimeEvent) -> Option<OffchainChainEvent> {
    use offchain::pallet::Event as OffchainEvent;
    match ev {
        runtime::RuntimeEvent::OffchainTransaction(inner) => match inner {
            OffchainEvent::Deposited {
                user,
                bank_cid,
                amount,
            } => Some(OffchainChainEvent::Deposited {
                user,
                bank_cid: bank_cid.into_inner(),
                amount,
            }),
            OffchainEvent::Withdrawn {
                user,
                bank_cid,
                amount,
            } => Some(OffchainChainEvent::Withdrawn {
                user,
                bank_cid: bank_cid.into_inner(),
                amount,
            }),
            OffchainEvent::PaymentSettled {
                tx_id,
                payer_account_id: payer,
                payer_bank_cid,
                recipient_account_id: recipient,
                recipient_bank_cid,
                transfer_amount,
                fee_amount,
            } => Some(OffchainChainEvent::PaymentSettled {
                tx_id,
                payer,
                payer_bank_cid: payer_bank_cid.into_inner(),
                recipient,
                recipient_bank_cid: recipient_bank_cid.into_inner(),
                amount: transfer_amount,
                fee: fee_amount,
            }),
            OffchainEvent::BankBound { user, bank_cid } => Some(OffchainChainEvent::BankBound {
                user,
                bank_cid: bank_cid.into_inner(),
            }),
            OffchainEvent::BankSwitched {
                user,
                old_bank_cid,
                new_bank_cid,
            } => Some(OffchainChainEvent::BankSwitched {
                user,
                old_bank_cid: old_bank_cid.into_inner(),
                new_bank_cid: new_bank_cid.into_inner(),
            }),
            // 其他 offchain_transaction 事件(费率治理、批次级别等)
            // 与本地 ledger 同步无关,忽略。
            _ => None,
        },
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn mk() -> (Arc<OffchainLedger>, Vec<u8>) {
        let tmp = std::env::temp_dir().join("offchain_event_test");
        let _ = fs::remove_dir_all(&tmp);
        let bank: Vec<u8> = b"ZS001-PRB08-233384677-2026".to_vec();
        (Arc::new(OffchainLedger::new(&tmp)), bank)
    }

    #[test]
    fn deposited_event_updates_own_bank_ledger() {
        let (ledger, bank) = mk();
        let listener = EventListener::new(ledger.clone(), bank.clone());
        let user = AccountId32::new([1u8; 32]);
        listener.handle(OffchainChainEvent::Deposited {
            user: user.clone(),
            bank_cid: bank.clone(),
            amount: 500,
        });
        assert_eq!(ledger.available_balance(&user), 500);
    }

    #[test]
    fn deposited_event_ignored_for_other_bank() {
        let (ledger, bank) = mk();
        let listener = EventListener::new(ledger.clone(), bank);
        let other_bank: Vec<u8> = b"AH001-PRB0X-111111111-2026".to_vec();
        let user = AccountId32::new([1u8; 32]);
        listener.handle(OffchainChainEvent::Deposited {
            user: user.clone(),
            bank_cid: other_bank,
            amount: 500,
        });
        assert_eq!(ledger.available_balance(&user), 0);
    }

    #[test]
    fn withdrawn_decreases_confirmed() {
        let (ledger, bank) = mk();
        let listener = EventListener::new(ledger.clone(), bank.clone());
        let user = AccountId32::new([2u8; 32]);
        listener.handle(OffchainChainEvent::Deposited {
            user: user.clone(),
            bank_cid: bank.clone(),
            amount: 1000,
        });
        listener.handle(OffchainChainEvent::Withdrawn {
            user: user.clone(),
            bank_cid: bank,
            amount: 300,
        });
        assert_eq!(ledger.available_balance(&user), 700);
    }

    // ─── convert_event 正负路径 ────────────────────────────────────────
    // 覆盖 5 个本 pallet 事件变体:Deposited / Withdrawn / PaymentSettled /
    // BankBound / BankSwitched,以及非本 pallet 的事件应返回 None。

    use offchain::pallet::Event as PalletEvent;

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
            Some(OffchainChainEvent::Deposited {
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
            Some(OffchainChainEvent::PaymentSettled {
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
}
