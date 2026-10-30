//! 地址目录与地址变更的真实调用基准。
//!
//! 授权主体由运行时在计时区间外准备；计时区间内始终执行原权限检查。

use crate::{
    pallet::{
        AddressHashes, AddressKeyOf, AddressNameHashes, AddressNameKeyOf, AddressNameVersions,
        AddressVersions, CatalogHash, CatalogVersion, CodeOf, VersionOf,
    },
    ActorCidNumber, ActorRoleCode, AddressUpdateAuthority, Call, Config, Event, Pallet,
};
use frame_benchmarking::v2::*;
use frame_support::traits::Get;
use frame_system::RawOrigin;
use sp_runtime::traits::Hash;
use sp_std::{vec, vec::Vec};

/// 基准专用授权上下文；不进入交易载荷、元数据或生产存储。
pub struct Authority<AccountId> {
    pub caller: AccountId,
    pub actor_cid_number: ActorCidNumber,
    pub actor_role_code: ActorRoleCode,
    pub province_code: Vec<u8>,
    pub city_code: Vec<u8>,
    pub town_code: Vec<u8>,
}

fn authority<T: Config>(action_code: u32) -> Result<Authority<T::AccountId>, BenchmarkError> {
    frame_system::Pallet::<T>::set_block_number(1u32.into());
    T::AddressAuthority::benchmark_authority(action_code)
        .ok_or(BenchmarkError::Stop("地址基准缺少真实授权主体与辖区"))
}

fn name_key<T: Config>(
    actor: &Authority<T::AccountId>,
) -> Result<AddressNameKeyOf<T>, BenchmarkError> {
    let bounded = |raw: &[u8]| -> Result<CodeOf<T>, BenchmarkError> {
        raw.to_vec()
            .try_into()
            .map_err(|_| BenchmarkError::Stop("基准行政区码超出上限"))
    };
    Ok((
        bounded(&actor.province_code)?,
        bounded(&actor.city_code)?,
        bounded(&actor.town_code)?,
        b"101"
            .to_vec()
            .try_into()
            .map_err(|_| BenchmarkError::Stop("基准地址名称编号超出上限"))?,
    ))
}

fn address_key<T: Config>(
    actor: &Authority<T::AccountId>,
) -> Result<AddressKeyOf<T>, BenchmarkError> {
    let (province, city, town, name) = name_key::<T>(actor)?;
    Ok((
        province,
        city,
        town,
        name,
        b"0001"
            .to_vec()
            .try_into()
            .map_err(|_| BenchmarkError::Stop("基准地址局部编号超出上限"))?,
        vec![b'd'; T::MaxAddressDetailLen::get() as usize]
            .try_into()
            .map_err(|_| BenchmarkError::Stop("基准详细地址超出上限"))?,
    ))
}

fn address_name<T: Config>() -> Result<Vec<u8>, BenchmarkError> {
    let name = vec![b'n'; T::MaxAddressNameLen::get() as usize];
    if name.is_empty() {
        return Err(BenchmarkError::Stop("基准地址名称上限必须大于零"));
    }
    Ok(name)
}

fn assert_last_event<T: Config>(event: Event<T>) {
    let event: <T as Config>::RuntimeEvent = event.into();
    frame_system::Pallet::<T>::assert_last_event(event.into());
}

#[benchmarks]
pub(crate) mod benchmarks {
    use super::*;

    /// 根目录测试只调用FRAME生成的原验证器，不复制或替代基准实现。
    #[cfg(test)]
    pub(crate) fn test_case<T: Config>(name: &[u8]) -> Result<(), BenchmarkError> {
        Pallet::<T>::test_bench_by_name(name)
    }

    #[benchmark]
    fn set_catalog_version() -> Result<(), BenchmarkError> {
        let actor = authority::<T>(0)?;
        let raw_version = vec![b'v'; T::MaxVersionLen::get() as usize];
        if raw_version.is_empty() {
            return Err(BenchmarkError::Stop("基准目录版本上限必须大于零"));
        }
        let version: VersionOf<T> = raw_version
            .clone()
            .try_into()
            .map_err(|_| BenchmarkError::Stop("基准目录版本超出上限"))?;
        let hash = T::Hashing::hash(b"address-catalog-benchmark");

        #[extrinsic_call]
        _(
            RawOrigin::Signed(actor.caller),
            actor.actor_cid_number.clone(),
            actor.actor_role_code,
            raw_version,
            hash,
        );

        assert_eq!(CatalogVersion::<T>::get(), Some(version.clone()));
        assert_eq!(CatalogHash::<T>::get(), Some(hash));
        assert_last_event::<T>(Event::<T>::CatalogVersionSet {
            actor_cid_number: actor.actor_cid_number,
            catalog_version: version,
            catalog_hash: hash,
        });
        Ok(())
    }

    #[benchmark]
    fn set_address_name() -> Result<(), BenchmarkError> {
        let actor = authority::<T>(1)?;
        let key = name_key::<T>(&actor)?;
        let name = address_name::<T>()?;
        let bounded_name: crate::pallet::AddressNameOf<T> = name
            .clone()
            .try_into()
            .map_err(|_| BenchmarkError::Stop("基准地址名称超出上限"))?;
        let version = AddressNameVersions::<T>::get(&key).saturating_add(1);
        let hash = T::Hashing::hash_of(&(b"address-name".as_slice(), &key, &bounded_name, version));

        #[extrinsic_call]
        _(
            RawOrigin::Signed(actor.caller),
            actor.actor_cid_number.clone(),
            actor.actor_role_code,
            actor.province_code,
            actor.city_code,
            actor.town_code,
            key.3.to_vec(),
            name,
        );

        assert_eq!(AddressNameVersions::<T>::get(&key), version);
        assert_eq!(AddressNameHashes::<T>::get(&key), Some(hash));
        assert_last_event::<T>(Event::<T>::AddressNameSet {
            actor_cid_number: actor.actor_cid_number,
            province_code: key.0,
            city_code: key.1,
            town_code: key.2,
            address_name_code: key.3,
            address_name: bounded_name,
            catalog_version: CatalogVersion::<T>::get(),
            address_version: version,
            address_hash: hash,
        });
        Ok(())
    }

    #[benchmark]
    fn remove_address_name() -> Result<(), BenchmarkError> {
        let actor = authority::<T>(2)?;
        let key = name_key::<T>(&actor)?;
        // 删除前通过原写入调用建立记录，同时执行其真实授权检查。
        Pallet::<T>::set_address_name(
            RawOrigin::Signed(actor.caller.clone()).into(),
            actor.actor_cid_number.clone(),
            actor.actor_role_code.clone(),
            actor.province_code.clone(),
            actor.city_code.clone(),
            actor.town_code.clone(),
            key.3.to_vec(),
            address_name::<T>()?,
        )
        .map_err(|_| BenchmarkError::Stop("地址名称删除基准前置写入失败"))?;
        assert!(AddressNameHashes::<T>::contains_key(&key));
        let version = AddressNameVersions::<T>::get(&key).saturating_add(1);

        #[extrinsic_call]
        _(
            RawOrigin::Signed(actor.caller),
            actor.actor_cid_number.clone(),
            actor.actor_role_code,
            actor.province_code,
            actor.city_code,
            actor.town_code,
            key.3.to_vec(),
        );

        assert_eq!(AddressNameVersions::<T>::get(&key), version);
        assert!(!AddressNameHashes::<T>::contains_key(&key));
        assert_last_event::<T>(Event::<T>::AddressNameRemoved {
            actor_cid_number: actor.actor_cid_number,
            province_code: key.0,
            city_code: key.1,
            town_code: key.2,
            address_name_code: key.3,
            catalog_version: CatalogVersion::<T>::get(),
            address_version: version,
        });
        Ok(())
    }

    #[benchmark]
    fn set_address() -> Result<(), BenchmarkError> {
        let actor = authority::<T>(3)?;
        let key = address_key::<T>(&actor)?;
        let version = AddressVersions::<T>::get(&key).saturating_add(1);
        let hash = T::Hashing::hash_of(&(b"address".as_slice(), &key, version));

        #[extrinsic_call]
        _(
            RawOrigin::Signed(actor.caller),
            actor.actor_cid_number.clone(),
            actor.actor_role_code,
            actor.province_code,
            actor.city_code,
            actor.town_code,
            key.3.to_vec(),
            key.4.to_vec(),
            key.5.to_vec(),
        );

        assert_eq!(AddressVersions::<T>::get(&key), version);
        assert_eq!(AddressHashes::<T>::get(&key), Some(hash));
        assert_last_event::<T>(Event::<T>::AddressSet {
            actor_cid_number: actor.actor_cid_number,
            province_code: key.0,
            city_code: key.1,
            town_code: key.2,
            address_name_code: key.3,
            address_local_no: key.4,
            address_detail: key.5,
            catalog_version: CatalogVersion::<T>::get(),
            address_version: version,
            address_hash: hash,
        });
        Ok(())
    }

    #[benchmark]
    fn remove_address() -> Result<(), BenchmarkError> {
        let actor = authority::<T>(4)?;
        let key = address_key::<T>(&actor)?;
        Pallet::<T>::set_address(
            RawOrigin::Signed(actor.caller.clone()).into(),
            actor.actor_cid_number.clone(),
            actor.actor_role_code.clone(),
            actor.province_code.clone(),
            actor.city_code.clone(),
            actor.town_code.clone(),
            key.3.to_vec(),
            key.4.to_vec(),
            key.5.to_vec(),
        )
        .map_err(|_| BenchmarkError::Stop("地址删除基准前置写入失败"))?;
        assert!(AddressHashes::<T>::contains_key(&key));
        let version = AddressVersions::<T>::get(&key).saturating_add(1);

        #[extrinsic_call]
        _(
            RawOrigin::Signed(actor.caller),
            actor.actor_cid_number.clone(),
            actor.actor_role_code,
            actor.province_code,
            actor.city_code,
            actor.town_code,
            key.3.to_vec(),
            key.4.to_vec(),
            key.5.to_vec(),
        );

        assert_eq!(AddressVersions::<T>::get(&key), version);
        assert!(!AddressHashes::<T>::contains_key(&key));
        assert_last_event::<T>(Event::<T>::AddressRemoved {
            actor_cid_number: actor.actor_cid_number,
            province_code: key.0,
            city_code: key.1,
            town_code: key.2,
            address_name_code: key.3,
            address_local_no: key.4,
            address_detail: key.5,
            catalog_version: CatalogVersion::<T>::get(),
            address_version: version,
        });
        Ok(())
    }
}
