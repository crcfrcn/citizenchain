//! 地址调用现行权重；原数据库读写公式保持，不替换为尚未测量的权重。

use frame_support::{traits::Get, weights::Weight};

pub(crate) fn set_catalog_version<T: frame_system::Config>() -> Weight {
    T::DbWeight::get().reads_writes(0, 2)
}

pub(crate) fn set_address_name<T: frame_system::Config>() -> Weight {
    T::DbWeight::get().reads_writes(2, 2)
}

pub(crate) fn remove_address_name<T: frame_system::Config>() -> Weight {
    T::DbWeight::get().reads_writes(2, 2)
}

pub(crate) fn set_address<T: frame_system::Config>() -> Weight {
    T::DbWeight::get().reads_writes(2, 2)
}

pub(crate) fn remove_address<T: frame_system::Config>() -> Weight {
    T::DbWeight::get().reads_writes(2, 2)
}
