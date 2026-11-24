//! 机构内部投票的严格多数阈值校验；个人阈值归personal.rs。
use super::*;
impl<T: Config> Pallet<T> {
    pub(crate) fn ensure_dynamic_threshold(voter_count: u32, threshold: u32) -> DispatchResult {
        // 机构岗位席位采用严格过半；统一用 u64 避免乘法溢出。
        ensure!(voter_count >= 2, Error::<T>::InvalidDynamicThreshold);
        ensure!(
            threshold > 0
                && threshold <= voter_count
                && u64::from(threshold).saturating_mul(2) > u64::from(voter_count),
            Error::<T>::InvalidDynamicThreshold
        );
        Ok(())
    }
}
