//! 市注册局的累计登记人数与达标顺序。

use crate::{CidNumberBound, CityRegistryProgress, Config, Pallet, QualifiedCityCount};
use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use frame_support::{traits::Get, weights::Weight};
use primitives::cid::number::parse_cid_number_parts_bytes;
use scale_info::TypeInfo;

#[derive(Clone, Debug, Default, Decode, DecodeWithMemTracking, Encode, MaxEncodedLen, TypeInfo)]
pub struct Progress {
    pub registered: u64,
    pub rank: Option<u32>,
}

impl<T: Config> Pallet<T> {
    /// 只在公民 CID 首次成功建立正式身份后调用。
    pub(crate) fn record_city_registration(registry: &CidNumberBound) {
        if !parse_cid_number_parts_bytes(registry).is_ok_and(|cid| cid.institution == *b"CREG") {
            return;
        }

        CityRegistryProgress::<T>::mutate(registry, |progress| {
            progress.registered = progress.registered.saturating_add(1);
            if progress.rank.is_some() {
                return;
            }
            let Some(rank) = QualifiedCityCount::<T>::get().checked_add(1) else {
                return;
            };
            let minimum = match rank {
                1..=100 => 1_001,
                101..=1_000 => 10_001,
                _ => 100_001,
            };
            if progress.registered >= minimum {
                progress.rank = Some(rank);
                QualifiedCityCount::<T>::put(rank);
            }
        });
    }
}

/// 按登记恰好达标的最坏路径预留两次读取和两次写入。
pub(crate) fn weight<T: Config>() -> Weight {
    Weight::from_parts(10_000_000, 6_000).saturating_add(T::DbWeight::get().reads_writes(2, 2))
}
