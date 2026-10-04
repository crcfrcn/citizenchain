#![cfg_attr(not(feature = "std"), no_std)]
//! 地址变更上链模块。
//!
//! 本 pallet 不保存完整行政区地址库,只保存当前地址目录版本、当前地址哈希
//! 和地址变更事件。完整地址库仍由 OnChina 本地 `china.sqlite` 保存,节点通过链上事件
//! 对单条地址做增删改同步。

pub use pallet::*;

/// 地址变更发起注册局的机构唯一 CID。
pub type ActorCidNumber =
    frame_support::BoundedVec<u8, frame_support::pallet_prelude::ConstU32<32>>;
pub type ActorRoleCode = frame_support::BoundedVec<u8, frame_support::pallet_prelude::ConstU32<64>>;

/// 地址更新权限抽象。
///
/// FRG/CREG 的省市授权规则由 runtime 统一实现,本 pallet 不直接依赖
/// public-admins/public-manage 的 storage,避免地址模块复制注册局权限细节。
pub trait AddressUpdateAuthority<AccountId> {
    fn can_update_catalog(
        who: &AccountId,
        actor_cid_number: &[u8],
        actor_role_code: &[u8],
        action_code: u32,
    ) -> bool;

    // 链上授权接口必须逐字段接收行为主体和行政区范围，不能为通过 lint 改变协议形状。
    fn can_update_address(
        who: &AccountId,
        actor_cid_number: &[u8],
        actor_role_code: &[u8],
        province_code: &[u8],
        city_code: &[u8],
        action_code: u32,
    ) -> bool;
}

impl<AccountId> AddressUpdateAuthority<AccountId> for () {
    fn can_update_catalog(
        _who: &AccountId,
        _actor_cid_number: &[u8],
        _actor_role_code: &[u8],
        _action_code: u32,
    ) -> bool {
        false
    }

    fn can_update_address(
        _who: &AccountId,
        _actor_cid_number: &[u8],
        _actor_role_code: &[u8],
        _province_code: &[u8],
        _city_code: &[u8],
        _action_code: u32,
    ) -> bool {
        false
    }
}

#[frame_support::pallet]
pub mod pallet {
    // FRAME 宏在本模块层生成 Call 分发代码,其参数数由 extrinsic 载荷决定;
    // per-fn 与 call 块级 allow 都够不到那段生成代码,故放宽范围收敛到本 pallet 模块,
    // 不扩到整个 crate —— 模块外的辅助函数参数过多仍会被 lint 抓出。
    #![allow(clippy::too_many_arguments)]
    use super::{ActorCidNumber, ActorRoleCode, AddressUpdateAuthority};
    use frame_support::{ensure, pallet_prelude::*};
    use frame_system::pallet_prelude::*;
    use sp_runtime::traits::Hash;
    use sp_std::vec::Vec;

    const STORAGE_VERSION: StorageVersion = StorageVersion::new(0);

    pub type CodeOf<T> = BoundedVec<u8, <T as Config>::MaxCodeLen>;
    pub type VersionOf<T> = BoundedVec<u8, <T as Config>::MaxVersionLen>;
    pub type AddressNameCodeOf<T> = BoundedVec<u8, <T as Config>::MaxAddressNameCodeLen>;
    pub type AddressLocalNoOf<T> = BoundedVec<u8, <T as Config>::MaxAddressLocalNoLen>;
    pub type AddressNameOf<T> = BoundedVec<u8, <T as Config>::MaxAddressNameLen>;
    pub type AddressDetailOf<T> = BoundedVec<u8, <T as Config>::MaxAddressDetailLen>;
    pub type AddressNameKeyOf<T> = (CodeOf<T>, CodeOf<T>, CodeOf<T>, AddressNameCodeOf<T>);
    pub type AddressKeyOf<T> = (
        CodeOf<T>,
        CodeOf<T>,
        CodeOf<T>,
        AddressNameCodeOf<T>,
        AddressLocalNoOf<T>,
        AddressDetailOf<T>,
    );

    #[pallet::config]
    pub trait Config: frame_system::Config {
        #[allow(deprecated)]
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

        /// 注册局地址更新权限判断入口。
        type AddressAuthority: AddressUpdateAuthority<Self::AccountId>;

        /// 省/市/镇编码最大字节长度。
        #[pallet::constant]
        type MaxCodeLen: Get<u32>;
        /// 行政区数据库版本号最大字节长度,例如 `v1.0.1`。
        #[pallet::constant]
        type MaxVersionLen: Get<u32>;
        /// 地址名称编号固定 3 位。
        #[pallet::constant]
        type MaxAddressNameCodeLen: Get<u32>;
        /// 地址局部编号固定 4 位,也允许为空。
        #[pallet::constant]
        type MaxAddressLocalNoLen: Get<u32>;
        /// 镇下地址名称最大字节长度。
        #[pallet::constant]
        type MaxAddressNameLen: Get<u32>;
        /// 详细地址最大字节长度。
        #[pallet::constant]
        type MaxAddressDetailLen: Get<u32>;
    }

    #[pallet::pallet]
    #[pallet::storage_version(STORAGE_VERSION)]
    pub struct Pallet<T>(_);

    /// 当前行政区地址库版本号。
    #[pallet::storage]
    pub type CatalogVersion<T: Config> = StorageValue<_, VersionOf<T>, OptionQuery>;

    /// 当前行政区地址库整体哈希。
    #[pallet::storage]
    pub type CatalogHash<T: Config> = StorageValue<_, T::Hash, OptionQuery>;

    /// 地址名称当前版本。
    #[pallet::storage]
    pub type AddressNameVersions<T: Config> =
        StorageMap<_, Blake2_128Concat, AddressNameKeyOf<T>, u32, ValueQuery>;

    /// 地址名称当前内容哈希。
    #[pallet::storage]
    pub type AddressNameHashes<T: Config> =
        StorageMap<_, Blake2_128Concat, AddressNameKeyOf<T>, T::Hash, OptionQuery>;

    /// 完整地址当前版本。
    #[pallet::storage]
    pub type AddressVersions<T: Config> =
        StorageMap<_, Blake2_128Concat, AddressKeyOf<T>, u32, ValueQuery>;

    /// 完整地址当前内容哈希。
    #[pallet::storage]
    pub type AddressHashes<T: Config> =
        StorageMap<_, Blake2_128Concat, AddressKeyOf<T>, T::Hash, OptionQuery>;

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        CatalogVersionSet {
            actor_cid_number: ActorCidNumber,
            catalog_version: VersionOf<T>,
            catalog_hash: T::Hash,
        },
        AddressNameSet {
            actor_cid_number: ActorCidNumber,
            province_code: CodeOf<T>,
            city_code: CodeOf<T>,
            town_code: CodeOf<T>,
            address_name_code: AddressNameCodeOf<T>,
            address_name: AddressNameOf<T>,
            catalog_version: Option<VersionOf<T>>,
            address_version: u32,
            address_hash: T::Hash,
        },
        AddressNameRemoved {
            actor_cid_number: ActorCidNumber,
            province_code: CodeOf<T>,
            city_code: CodeOf<T>,
            town_code: CodeOf<T>,
            address_name_code: AddressNameCodeOf<T>,
            catalog_version: Option<VersionOf<T>>,
            address_version: u32,
        },
        AddressSet {
            actor_cid_number: ActorCidNumber,
            province_code: CodeOf<T>,
            city_code: CodeOf<T>,
            town_code: CodeOf<T>,
            address_name_code: AddressNameCodeOf<T>,
            address_local_no: AddressLocalNoOf<T>,
            address_detail: AddressDetailOf<T>,
            catalog_version: Option<VersionOf<T>>,
            address_version: u32,
            address_hash: T::Hash,
        },
        AddressRemoved {
            actor_cid_number: ActorCidNumber,
            province_code: CodeOf<T>,
            city_code: CodeOf<T>,
            town_code: CodeOf<T>,
            address_name_code: AddressNameCodeOf<T>,
            address_local_no: AddressLocalNoOf<T>,
            address_detail: AddressDetailOf<T>,
            catalog_version: Option<VersionOf<T>>,
            address_version: u32,
        },
    }

    #[pallet::error]
    pub enum Error<T> {
        Unauthorized,
        EmptyCode,
        FieldTooLong,
        EmptyCatalogVersion,
        InvalidAddressNameCode,
        InvalidAddressLocalNo,
        EmptyAddressName,
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        /// 设置行政区地址库版本号和整体哈希。
        #[pallet::call_index(0)]
        #[pallet::weight(T::DbWeight::get().reads_writes(0, 2))]
        pub fn set_catalog_version(
            origin: OriginFor<T>,
            actor_cid_number: ActorCidNumber,
            actor_role_code: ActorRoleCode,
            catalog_version: Vec<u8>,
            catalog_hash: T::Hash,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            let catalog_version = Self::bounded_version(catalog_version)?;
            ensure!(
                T::AddressAuthority::can_update_catalog(
                    &who,
                    actor_cid_number.as_slice(),
                    actor_role_code.as_slice(),
                    0,
                ),
                Error::<T>::Unauthorized
            );

            CatalogVersion::<T>::put(&catalog_version);
            CatalogHash::<T>::put(catalog_hash);
            Self::deposit_event(Event::CatalogVersionSet {
                actor_cid_number,
                catalog_version,
                catalog_hash,
            });
            Ok(())
        }

        /// 新增或修改镇下地址名称。
        #[pallet::call_index(1)]
        #[pallet::weight(T::DbWeight::get().reads_writes(2, 2))]
        pub fn set_address_name(
            origin: OriginFor<T>,
            actor_cid_number: ActorCidNumber,
            actor_role_code: ActorRoleCode,
            province_code: Vec<u8>,
            city_code: Vec<u8>,
            town_code: Vec<u8>,
            address_name_code: Vec<u8>,
            address_name: Vec<u8>,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            let (province_code, city_code, town_code, address_name_code) =
                Self::name_key_parts(province_code, city_code, town_code, address_name_code)?;
            let address_name = Self::bounded_address_name(address_name)?;
            Self::ensure_authority(
                &who,
                actor_cid_number.as_slice(),
                actor_role_code.as_slice(),
                &province_code,
                &city_code,
                1,
            )?;

            let key = (
                province_code.clone(),
                city_code.clone(),
                town_code.clone(),
                address_name_code.clone(),
            );
            let address_version = AddressNameVersions::<T>::get(&key).saturating_add(1);
            let address_hash = T::Hashing::hash_of(&(
                b"address-name".as_slice(),
                &key,
                &address_name,
                address_version,
            ));
            AddressNameVersions::<T>::insert(&key, address_version);
            AddressNameHashes::<T>::insert(&key, address_hash);
            Self::deposit_event(Event::AddressNameSet {
                actor_cid_number,
                province_code,
                city_code,
                town_code,
                address_name_code,
                address_name,
                catalog_version: CatalogVersion::<T>::get(),
                address_version,
                address_hash,
            });
            Ok(())
        }

        /// 删除镇下地址名称。
        #[pallet::call_index(2)]
        #[pallet::weight(T::DbWeight::get().reads_writes(2, 2))]
        pub fn remove_address_name(
            origin: OriginFor<T>,
            actor_cid_number: ActorCidNumber,
            actor_role_code: ActorRoleCode,
            province_code: Vec<u8>,
            city_code: Vec<u8>,
            town_code: Vec<u8>,
            address_name_code: Vec<u8>,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            let (province_code, city_code, town_code, address_name_code) =
                Self::name_key_parts(province_code, city_code, town_code, address_name_code)?;
            Self::ensure_authority(
                &who,
                actor_cid_number.as_slice(),
                actor_role_code.as_slice(),
                &province_code,
                &city_code,
                2,
            )?;

            let key = (
                province_code.clone(),
                city_code.clone(),
                town_code.clone(),
                address_name_code.clone(),
            );
            let address_version = AddressNameVersions::<T>::get(&key).saturating_add(1);
            AddressNameVersions::<T>::insert(&key, address_version);
            AddressNameHashes::<T>::remove(&key);
            Self::deposit_event(Event::AddressNameRemoved {
                actor_cid_number,
                province_code,
                city_code,
                town_code,
                address_name_code,
                catalog_version: CatalogVersion::<T>::get(),
                address_version,
            });
            Ok(())
        }

        /// 新增或修改完整地址。
        #[pallet::call_index(3)]
        #[pallet::weight(T::DbWeight::get().reads_writes(2, 2))]
        // extrinsic 参数顺序属于链上编码契约，不能为通过 lint 合并成另一种载荷。
        pub fn set_address(
            origin: OriginFor<T>,
            actor_cid_number: ActorCidNumber,
            actor_role_code: ActorRoleCode,
            province_code: Vec<u8>,
            city_code: Vec<u8>,
            town_code: Vec<u8>,
            address_name_code: Vec<u8>,
            address_local_no: Vec<u8>,
            address_detail: Vec<u8>,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            let key = Self::address_key(
                province_code,
                city_code,
                town_code,
                address_name_code,
                address_local_no,
                address_detail,
            )?;
            Self::ensure_authority(
                &who,
                actor_cid_number.as_slice(),
                actor_role_code.as_slice(),
                &key.0,
                &key.1,
                3,
            )?;

            let address_version = AddressVersions::<T>::get(&key).saturating_add(1);
            let address_hash = T::Hashing::hash_of(&(b"address".as_slice(), &key, address_version));
            AddressVersions::<T>::insert(&key, address_version);
            AddressHashes::<T>::insert(&key, address_hash);
            Self::deposit_event(Event::AddressSet {
                actor_cid_number,
                province_code: key.0,
                city_code: key.1,
                town_code: key.2,
                address_name_code: key.3,
                address_local_no: key.4,
                address_detail: key.5,
                catalog_version: CatalogVersion::<T>::get(),
                address_version,
                address_hash,
            });
            Ok(())
        }

        /// 删除完整地址。
        #[pallet::call_index(4)]
        #[pallet::weight(T::DbWeight::get().reads_writes(2, 2))]
        pub fn remove_address(
            origin: OriginFor<T>,
            actor_cid_number: ActorCidNumber,
            actor_role_code: ActorRoleCode,
            province_code: Vec<u8>,
            city_code: Vec<u8>,
            town_code: Vec<u8>,
            address_name_code: Vec<u8>,
            address_local_no: Vec<u8>,
            address_detail: Vec<u8>,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            let key = Self::address_key(
                province_code,
                city_code,
                town_code,
                address_name_code,
                address_local_no,
                address_detail,
            )?;
            Self::ensure_authority(
                &who,
                actor_cid_number.as_slice(),
                actor_role_code.as_slice(),
                &key.0,
                &key.1,
                4,
            )?;

            let address_version = AddressVersions::<T>::get(&key).saturating_add(1);
            AddressVersions::<T>::insert(&key, address_version);
            AddressHashes::<T>::remove(&key);
            Self::deposit_event(Event::AddressRemoved {
                actor_cid_number,
                province_code: key.0,
                city_code: key.1,
                town_code: key.2,
                address_name_code: key.3,
                address_local_no: key.4,
                address_detail: key.5,
                catalog_version: CatalogVersion::<T>::get(),
                address_version,
            });
            Ok(())
        }
    }

    impl<T: Config> Pallet<T> {
        fn ensure_authority(
            who: &T::AccountId,
            actor_cid_number: &[u8],
            actor_role_code: &[u8],
            province_code: &[u8],
            city_code: &[u8],
            action_code: u32,
        ) -> DispatchResult {
            ensure!(
                T::AddressAuthority::can_update_address(
                    who,
                    actor_cid_number,
                    actor_role_code,
                    province_code,
                    city_code,
                    action_code,
                ),
                Error::<T>::Unauthorized
            );
            Ok(())
        }

        fn bounded_code(raw: Vec<u8>) -> Result<CodeOf<T>, Error<T>> {
            ensure!(!raw.is_empty(), Error::<T>::EmptyCode);
            raw.try_into().map_err(|_| Error::<T>::FieldTooLong)
        }

        fn bounded_version(raw: Vec<u8>) -> Result<VersionOf<T>, Error<T>> {
            ensure!(!raw.is_empty(), Error::<T>::EmptyCatalogVersion);
            raw.try_into().map_err(|_| Error::<T>::FieldTooLong)
        }

        fn bounded_address_name(raw: Vec<u8>) -> Result<AddressNameOf<T>, Error<T>> {
            ensure!(!raw.is_empty(), Error::<T>::EmptyAddressName);
            raw.try_into().map_err(|_| Error::<T>::FieldTooLong)
        }

        fn is_ascii_digits(raw: &[u8]) -> bool {
            raw.iter().all(|b| b.is_ascii_digit())
        }

        fn bounded_address_name_code(raw: Vec<u8>) -> Result<AddressNameCodeOf<T>, Error<T>> {
            ensure!(
                raw.len() == 3 && raw.as_slice() != b"000" && Self::is_ascii_digits(&raw),
                Error::<T>::InvalidAddressNameCode
            );
            raw.try_into().map_err(|_| Error::<T>::FieldTooLong)
        }

        fn bounded_address_local_no(raw: Vec<u8>) -> Result<AddressLocalNoOf<T>, Error<T>> {
            ensure!(
                raw.is_empty()
                    || (raw.len() == 4 && raw.as_slice() != b"0000" && Self::is_ascii_digits(&raw)),
                Error::<T>::InvalidAddressLocalNo
            );
            raw.try_into().map_err(|_| Error::<T>::FieldTooLong)
        }

        fn bounded_address_detail(raw: Vec<u8>) -> Result<AddressDetailOf<T>, Error<T>> {
            raw.try_into().map_err(|_| Error::<T>::FieldTooLong)
        }

        fn name_key_parts(
            province_code: Vec<u8>,
            city_code: Vec<u8>,
            town_code: Vec<u8>,
            address_name_code: Vec<u8>,
        ) -> Result<AddressNameKeyOf<T>, Error<T>> {
            Ok((
                Self::bounded_code(province_code)?,
                Self::bounded_code(city_code)?,
                Self::bounded_code(town_code)?,
                Self::bounded_address_name_code(address_name_code)?,
            ))
        }

        fn address_key(
            province_code: Vec<u8>,
            city_code: Vec<u8>,
            town_code: Vec<u8>,
            address_name_code: Vec<u8>,
            address_local_no: Vec<u8>,
            address_detail: Vec<u8>,
        ) -> Result<AddressKeyOf<T>, Error<T>> {
            let (province_code, city_code, town_code, address_name_code) =
                Self::name_key_parts(province_code, city_code, town_code, address_name_code)?;
            Ok((
                province_code,
                city_code,
                town_code,
                address_name_code,
                Self::bounded_address_local_no(address_local_no)?,
                Self::bounded_address_detail(address_detail)?,
            ))
        }
    }
}
