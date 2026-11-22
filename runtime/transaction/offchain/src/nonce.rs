//! 读取并消费单调支付nonce，防止支付意图重放。

use crate::{Config, Error, L3PaymentNonce};

/// 消费 L3 的下一个 nonce。
///
/// 语义:`submitted_nonce` 必须严格等于链上当前值 + 1,否则拒绝。
/// 成功时把 `L3PaymentNonce[payer_account_id]` 更新为 `submitted_nonce`。
pub fn consume_nonce<T: Config>(
    payer_account_id: &T::AccountId,
    submitted_nonce: u64,
) -> Result<(), Error<T>> {
    let current = L3PaymentNonce::<T>::get(payer_account_id);
    let expected = current.checked_add(1).ok_or(Error::<T>::L3NonceOverflow)?;
    if submitted_nonce != expected {
        return Err(Error::<T>::InvalidL3Nonce);
    }
    L3PaymentNonce::<T>::insert(payer_account_id, submitted_nonce);
    Ok(())
}

/// 查询 L3 的"下一个应该使用的 nonce"(供 citizenapp RPC 查询)。
///
/// citizenapp 在签名前先问清算行节点拿这个值,避免本地 nonce 与链上错位。
pub fn next_nonce<T: Config>(payer_account_id: &T::AccountId) -> u64 {
    L3PaymentNonce::<T>::get(payer_account_id).saturating_add(1)
}
