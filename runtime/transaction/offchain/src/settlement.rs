//! 扫码支付清算体系:清算行批次上链 settlement 路径。
//!
//!
//! - 同行清算(payer_bank_cid == recipient_bank_cid):仅 `DepositBalance` 轧差 + 手续费从清算账户到费用账户
//! - 跨行清算(payer_bank_cid != recipient_bank_cid):
//!   付款方清算账户 → 收款方清算账户(本金)
//!   + 付款方清算账户 → 收款方费用账户(fee)
//!   + 双方 `DepositBalance` / `BankTotalDeposits` 同步
//! - 每条 `OffchainBatchItem` 必经:
//!     1. L3 签名验证(`sr25519_verify` 对 `PaymentIntent::signing_hash`)
//!     2. nonce 单调递增(`nonce::consume_nonce`)
//!     3. 费率正确性(按 **收款方** 清算行 `L2FeeRateBp`,最低 1 分)
//!     4. 偿付自动保护(`solvency::ensure_can_debit`)
//!     5. 防重放(`ProcessedOffchainTx` 不命中)
//! - 手续费**全部归收款方清算行的费用账户**,无省储行分成。
//!
//! **收款方主导清算**模型。
//! - `submit_offchain_batch` 的 `institution_account_id` = 收款方清算行主账户(身份锚)
//! - 同一批次所有 item 的 `recipient_bank_cid` 必须 == `actor_cid_number`(收款方 CID)
//! - 提交者 = 收款方清算行的某个激活管理员
//! - 每个 item 的付款公民从其 L2 清算账户存款支付链下清算费;批尾对累计手续费收一次链上费(Step 3)
//!
//! 节点 packer 收齐多笔 → 提交 `submit_offchain_batch` 走到这里。

use codec::Decode;
use frame_support::{
    ensure,
    traits::{Currency, ExistenceRequirement::KeepAlive},
};
use sp_core::sr25519::Signature as Sr25519Signature;
use sp_io::crypto::sr25519_verify;
use sp_runtime::{traits::SaturatedConversion, DispatchResult};

use primitives::fee_policy::OnchainFeeCharger;
use primitives::institution_asset::{InstitutionAsset, InstitutionAssetAction};

use crate::batch_item::OffchainBatchItem;
use crate::{
    bank_check::{self, CidAccountQuery},
    fee_config, nonce, solvency, BankTotalDeposits, Config, DepositBalance, Error, Event, Pallet,
    ProcessedOffchainTx, ProcessedOffchainTxAt, UserBank,
};
use frame_system::pallet_prelude::BlockNumberFor;

type BalanceOf<T> =
    <<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;

/// 计算本笔按收款方清算行费率应收的手续费(分),最低取 `primitives::fee_policy::OFFCHAIN_MIN_FEE`。
fn calc_fee(transfer_amount: u128, rate_bp: u32) -> Result<u128, &'static str> {
    let numerator = transfer_amount
        .checked_mul(rate_bp as u128)
        .ok_or("fee overflow")?;
    let quotient = numerator / 10_000;
    let remainder = numerator % 10_000;
    let rounded = if remainder >= 5_000 {
        quotient + 1
    } else {
        quotient
    };
    Ok(core::cmp::max(
        rounded,
        primitives::fee_policy::OFFCHAIN_MIN_FEE,
    ))
}

/// 清算行批次上链的完整执行。
///
/// **收款方主导清算**。
///
/// [`submitter`] 提交该批次的清算行管理员
/// [`actor_cid_number`] 本次交易的机构唯一主键
/// [`institution_account_id`] 批次归属的清算行主账户(= **收款方**清算行)
/// [`batch`] SCALE 编码过的批次数据（已在 extrinsic 入口完成 `BoundedVec` 长度校验）。
///
/// 偿付预检按 **付款方清算行** 做(每个 payer_bank_cid 各自统计扣减总额),因为
/// 跨行支付的链上 Currency 流出来自付款方清算账户。同一批次内可能有多个 payer_bank_cid。
pub fn execute_clearing_bank_batch<T: Config>(
    submitter: &T::AccountId,
    actor_cid_number: &crate::InstitutionCidNumber,
    actor_role_code: &[u8],
    institution_account_id: &T::AccountId,
    batch: &[OffchainBatchItem<T::AccountId, BlockNumberFor<T>>],
) -> DispatchResult {
    // 批次级校验：CID、岗位码、签名账户和具体账户必须同时匹配。
    bank_check::ensure_institution_account::<T>(
        actor_cid_number.as_slice(),
        institution_account_id,
        bank_check::ACCOUNT_NAME_MAIN,
    )?;
    ensure!(
        T::CidAccountQuery::is_institution_role_authorized(
            actor_cid_number.as_slice(),
            actor_role_code,
            submitter,
            entity_primitives::business_action::ACTION_OFFCHAIN_SUBMIT_BATCH,
        ),
        Error::<T>::UnauthorizedAdmin
    );

    let now = frame_system::Pallet::<T>::block_number();

    // 按付款方清算行分组统计扣款总额(同行只扣 fee 不流出清算账户;跨行扣 transfer+fee)。
    // 用 BTreeMap 保证迭代顺序确定,与 saturating_add 一起避免重入风险。
    let mut projected_debits: sp_std::collections::btree_map::BTreeMap<
        crate::InstitutionCidNumber,
        u128,
    > = Default::default();

    // 本批累计手续费(收款方主导 → 全部落 actor 费用账户),批尾对其收一次链上交易费。
    let mut total_batch_fee: u128 = 0;

    for item in batch.iter() {
        // recipient_bank_cid 必须等于本批次提交方 CID(收款方主导清算)
        ensure!(
            &item.recipient_bank_cid == actor_cid_number,
            Error::<T>::InstitutionMismatch
        );
        ensure!(item.transfer_amount > 0, Error::<T>::InvalidTransferAmount);
        ensure!(
            item.payer_account_id != item.recipient_account_id,
            Error::<T>::SelfTransferNotAllowed
        );
        ensure!(now <= item.expires_at, Error::<T>::ExpiredIntent);
        ensure!(
            UserBank::<T>::get(&item.payer_account_id).as_ref() == Some(&item.payer_bank_cid),
            Error::<T>::UserBankMismatch
        );
        ensure!(
            UserBank::<T>::get(&item.recipient_account_id).as_ref()
                == Some(&item.recipient_bank_cid),
            Error::<T>::UserBankMismatch
        );

        // 付款方清算行必须合法(跨行时必要;同行时即提交方自身,已知合法)
        if item.payer_bank_cid != item.recipient_bank_cid {
            bank_check::ensure_can_be_bound::<T>(item.payer_bank_cid.as_slice())?;
        }

        // 费率校验(按收款方清算行 CID)
        let rate_bp = fee_config::current_rate_bp::<T>(&item.recipient_bank_cid);
        ensure!(rate_bp > 0, Error::<T>::L2FeeRateNotConfigured);
        let expected_fee = calc_fee(item.transfer_amount, rate_bp)
            .map_err(|_| Error::<T>::TransferAmountTooLarge)?;
        ensure!(
            item.fee_amount == expected_fee,
            Error::<T>::InvalidFeeAmount
        );
        total_batch_fee = total_batch_fee.saturating_add(item.fee_amount);

        // 统计付款方清算行即将扣减的总额(用于偿付预检)
        let item_debit = if item.payer_bank_cid == item.recipient_bank_cid {
            // 同行:fee 走 fee_account 但本金内部轧差(清算账户净流出 = fee)
            item.fee_amount
        } else {
            // 跨行:本金 + fee 都从付款方清算账户流出
            item.transfer_amount.saturating_add(item.fee_amount)
        };
        let entry = projected_debits
            .entry(item.payer_bank_cid.clone())
            .or_insert(0);
        *entry = entry.saturating_add(item_debit);
    }

    // 按付款方清算行做偿付预检
    let mut total_batch_debit: u128 = 0;
    for (payer_cid, debit) in projected_debits.iter() {
        solvency::ensure_can_debit::<T>(payer_cid, *debit)?;
        total_batch_debit = total_batch_debit.saturating_add(*debit);
    }

    // 逐笔执行
    for item in batch.iter() {
        execute_single_item::<T>(item, now)?;
    }

    // 全批手续费已入账收款方(= actor)费用账户,清算行为这批收益支付一次链上交易费,
    // 走标准 80/10/10 分账。付款账户是费用账户(非清算账户),不碰用户存款准备金 →
    // solvency 不受影响。已包在 submit_offchain_batch 的 with_transaction 内:费用账户
    // 不足即整批回滚(S3-① fail-closed)。
    let fee_account = bank_check::fee_account_of::<T>(actor_cid_number.as_slice())?;
    let onchain_fee_basis: BalanceOf<T> = total_batch_fee.saturated_into();
    T::OnchainFeeCharger::charge(&fee_account, onchain_fee_basis)
        .map_err(|_| Error::<T>::ClearingBatchOnchainFeeUnpaid)?;

    Pallet::<T>::deposit_event(Event::<T>::ClearingBankBatchSettled {
        bank_cid: actor_cid_number.clone(),
        submitter: submitter.clone(),
        item_count: batch.len() as u32,
        total_debit: total_batch_debit,
    });
    Ok(())
}

/// 单笔 item 的验证 + 分账。
fn execute_single_item<T: Config>(
    item: &OffchainBatchItem<T::AccountId, BlockNumberFor<T>>,
    now: BlockNumberFor<T>,
) -> DispatchResult {
    // 1. 验 L3 签名
    let intent = item.to_intent();
    let msg = intent.signing_hash();
    let payer_public_key = crate::sr25519_public_from_account_id(&item.payer_account_id);
    let sig = Sr25519Signature::try_from(&item.payer_sig[..])
        .map_err(|_| Error::<T>::InvalidL3Signature)?;
    ensure!(
        sr25519_verify(&sig, &msg, &payer_public_key),
        Error::<T>::InvalidL3Signature
    );

    // 2. 消费 nonce
    nonce::consume_nonce::<T>(&item.payer_account_id, item.payer_nonce)?;

    // 3. 防重放(shard key 从付款方清算行 cid_number 的 R5 前 2 字节取省码)。
    //
    //    `item.tx_id` 是 `H256`,链上 Storage 用 `T::Hash`(Substrate 默认等于
    //    H256)。通过 SCALE 编解码跨类型转换,与 frame_system 默认配置兼容。
    let province_shard = province_shard_from_cid(item.payer_bank_cid.as_slice())
        .ok_or(Error::<T>::InstitutionMismatch)?;
    let tx_hash: T::Hash = T::Hash::decode(&mut &item.tx_id.as_bytes()[..])
        .map_err(|_| Error::<T>::InvalidL3Signature)?;
    ensure!(
        !ProcessedOffchainTx::<T>::contains_key(province_shard, tx_hash),
        Error::<T>::TxAlreadyProcessed
    );

    // 4. 分账(身份=CID;资金落点=CID 派生清算账户;费用账户=CID 派生费用账户)
    let payer_cid = &item.payer_bank_cid;
    let recipient_cid = &item.recipient_bank_cid;
    let payer_clearing = bank_check::clearing_account_of::<T>(payer_cid.as_slice())?;
    let fee_account = bank_check::fee_account_of::<T>(recipient_cid.as_slice())?;

    // (S2-①)结算扣款必须过资金白名单:源恒为付款方清算账户,一次门禁覆盖本金 + fee 两笔转出。
    ensure!(
        T::InstitutionAsset::can_spend(&payer_clearing, InstitutionAssetAction::L2ClearingDebit),
        Error::<T>::ClearingDebitForbidden
    );

    // 付款方 L3 存款校验(账本按 CID 键)
    let payer_balance = DepositBalance::<T>::get(payer_cid, &item.payer_account_id);
    let total_debit = item.transfer_amount.saturating_add(item.fee_amount);
    ensure!(
        payer_balance >= total_debit,
        Error::<T>::InsufficientDepositBalance
    );

    if payer_cid == recipient_cid {
        // 同行:本金在 L2 内部轧差,只 fee 从清算账户流出到费用账户
        DepositBalance::<T>::mutate(payer_cid, &item.payer_account_id, |b| {
            *b = b.saturating_sub(total_debit);
        });
        DepositBalance::<T>::mutate(payer_cid, &item.recipient_account_id, |b| {
            *b = b.saturating_add(item.transfer_amount);
        });
        // 同行时 BankTotalDeposits 下降 fee 部分(手续费流出 L2)
        BankTotalDeposits::<T>::mutate(payer_cid, |t| {
            *t = t.saturating_sub(item.fee_amount);
        });
        let fee_bal: BalanceOf<T> = item.fee_amount.saturated_into();
        T::Currency::transfer(&payer_clearing, &fee_account, fee_bal, KeepAlive)?;
    } else {
        // 跨行:本金从付款方清算账户跨行转到收款方清算账户 + fee 转到收款方费用账户
        let recipient_clearing = bank_check::clearing_account_of::<T>(recipient_cid.as_slice())?;
        let transfer_bal: BalanceOf<T> = item.transfer_amount.saturated_into();
        let fee_bal: BalanceOf<T> = item.fee_amount.saturated_into();
        T::Currency::transfer(
            &payer_clearing,
            &recipient_clearing,
            transfer_bal,
            KeepAlive,
        )?;
        T::Currency::transfer(&payer_clearing, &fee_account, fee_bal, KeepAlive)?;

        DepositBalance::<T>::mutate(payer_cid, &item.payer_account_id, |b| {
            *b = b.saturating_sub(total_debit);
        });
        DepositBalance::<T>::mutate(recipient_cid, &item.recipient_account_id, |b| {
            *b = b.saturating_add(item.transfer_amount);
        });
        BankTotalDeposits::<T>::mutate(payer_cid, |t| {
            *t = t.saturating_sub(total_debit);
        });
        BankTotalDeposits::<T>::mutate(recipient_cid, |t| {
            *t = t.saturating_add(item.transfer_amount);
        });
    }

    // 5. 防重放标记 + 事件(tx_hash 已在本函数开头从 item.tx_id 解码)
    ProcessedOffchainTx::<T>::insert(province_shard, tx_hash, true);
    ProcessedOffchainTxAt::<T>::insert(province_shard, tx_hash, now);

    Pallet::<T>::deposit_event(Event::<T>::PaymentSettled {
        tx_id: tx_hash,
        payer_account_id: item.payer_account_id.clone(),
        payer_bank_cid: item.payer_bank_cid.clone(),
        recipient_account_id: item.recipient_account_id.clone(),
        recipient_bank_cid: item.recipient_bank_cid.clone(),
        transfer_amount: item.transfer_amount,
        fee_amount: item.fee_amount,
    });
    Ok(())
}

/// 从清算行 CID 直接取省行政区 shard key(CID 第一段 R5 前 2 字符)。
///
/// CID 格式 `R5-K3P1C1-N9-D4`,第一段 R5 前 2 字符是省编码。不足 2 字节返回 None。
fn province_shard_from_cid(cid_bytes: &[u8]) -> Option<[u8; 2]> {
    let shard = cid_bytes.get(0..2)?;
    Some([shard[0], shard[1]])
}
