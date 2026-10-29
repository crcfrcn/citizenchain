//! 链上转账现行权重；仅归并原调用注解中的公式，不是基准生成结果。

use frame_support::{traits::Get, weights::Weight};

pub(crate) fn transfer_with_remark<T: frame_system::Config>() -> Weight {
    T::DbWeight::get().reads_writes(1, 1)
}
