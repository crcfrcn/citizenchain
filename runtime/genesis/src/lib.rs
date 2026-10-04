//! 创世模块=genesis-pallet
//!
//! # 职责
//! 1. 存储链的当前运行阶段（创世期 Genesis / 运行期 Operation）及开发者直升开关。
//! 2. 存储创世常量（创世宣言、国名宣言、创世人口），在创世区块中初始化。
//! 3. 创世写入内置公权机构和创世公职人员，只写初始 storage，不承载运行期治理。
//!
//! # 设计原则
//! - 纯存储 + getter + trait，不暴露 extrinsic。
//! - 阶段切换仅通过 runtime 升级迁移（OnRuntimeUpgrade）一次性写入，不设链上调用。
//! - PoW 平均目标固定在 primitives 核心常量中，不属于阶段状态，也不得由本模块修改。

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub mod institution;
pub mod weights;

pub use pallet::*;

// ─── DeveloperUpgradeCheck trait ────────────────────────────────────────────
// 供 runtime-upgrade 通过关联类型读取开发者直升开关，不硬耦合。
pub trait DeveloperUpgradeCheck {
    /// 开发者直升是否启用。
    fn is_enabled() -> bool;
}

// ─── GenesisInstitutionSeeder trait ─────────────────────────────────────────
// 创世机构/管理员 seeding 注入口。把「写 public_manage/public_admins 治理存储」这件事
// 从本 pallet 的 Config supertrait 解耦成 runtime 注入,消费者与本 pallet 单测因此
// 不必 mock 整套治理栈;seeding 逻辑仍在 institution::build,由 runtime 的实现调用。
pub trait GenesisInstitutionSeeder {
    /// 执行创世机构/管理员 seeding。
    fn seed();
}

#[frame_support::pallet]
pub mod pallet {
    use alloc::vec::Vec;
    use frame_support::pallet_prelude::*;

    #[pallet::pallet]
    pub struct Pallet<T>(_);

    #[pallet::config]
    pub trait Config: frame_system::Config {
        type WeightInfo: crate::weights::WeightInfo;

        /// 创世宣言和国名宣言的最大字节长度。
        #[pallet::constant]
        type MaxDeclarationLen: Get<u32>;

        /// 创世机构/管理员 seeding 注入(治理存储写入从本 Config 解耦,由 runtime 提供实现)。
        type InstitutionSeeder: super::GenesisInstitutionSeeder;
    }

    // ─── 类型 ──────────────────────────────────────────────────────────────

    /// 链运行阶段：Genesis（创世期）或 Operation（运行期）。
    #[derive(
        Encode,
        Decode,
        DecodeWithMemTracking,
        Clone,
        PartialEq,
        Eq,
        Debug,
        TypeInfo,
        MaxEncodedLen,
        Default,
    )]
    pub enum ChainPhase {
        /// 创世期：开发者可直接升级 runtime。
        #[default]
        Genesis,
        /// 运行期：开发者直升永久关闭，升级必须走治理授权。
        Operation,
    }

    // ─── Storage: 链阶段 ─────────────────────────────────────────────────

    /// 当前链阶段。创世默认 Genesis。
    #[pallet::storage]
    #[pallet::getter(fn phase)]
    pub type Phase<T> = StorageValue<_, ChainPhase, ValueQuery>;

    /// 开发者直升 runtime 开关。创世默认 true（开启）。
    #[pallet::storage]
    #[pallet::getter(fn developer_upgrade_enabled)]
    pub type DeveloperUpgradeEnabled<T> = StorageValue<_, bool, ValueQuery, DefaultDevUpgrade>;

    /// 开发者直升默认值。
    pub struct DefaultDevUpgrade;
    impl Get<bool> for DefaultDevUpgrade {
        fn get() -> bool {
            true
        }
    }

    // ─── Storage: 创世常量 ───────────────────────────────────────────────

    /// 创世宣言。
    #[pallet::storage]
    #[pallet::getter(fn citizens_declaration)]
    pub type CitizensDeclaration<T: Config> =
        StorageValue<_, BoundedVec<u8, T::MaxDeclarationLen>, ValueQuery>;

    /// 公民宣言。
    #[pallet::storage]
    #[pallet::getter(fn country_declaration)]
    pub type CountryDeclaration<T: Config> =
        StorageValue<_, BoundedVec<u8, T::MaxDeclarationLen>, ValueQuery>;

    /// 创世人口。
    #[pallet::storage]
    #[pallet::getter(fn citizen_max)]
    pub type CitizenMax<T> = StorageValue<_, u64, ValueQuery>;

    // ─── Genesis Config ─────────────────────────────────────────────────

    #[pallet::genesis_config]
    #[derive(frame_support::DefaultNoBound)]
    pub struct GenesisConfig<T: Config> {
        /// 创世宣言（UTF-8 字节）。
        pub citizens_declaration: Vec<u8>,
        /// 公民宣言（UTF-8 字节）。
        pub country_declaration: Vec<u8>,
        /// 创世人口。
        pub citizen_max: u64,
        #[serde(skip)]
        pub _phantom: core::marker::PhantomData<T>,
    }

    #[pallet::genesis_build]
    impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
        fn build(&self) {
            let citizens: BoundedVec<u8, T::MaxDeclarationLen> = self
                .citizens_declaration
                .clone()
                .try_into()
                .expect("创世宣言超出 MaxDeclarationLen");
            let country: BoundedVec<u8, T::MaxDeclarationLen> = self
                .country_declaration
                .clone()
                .try_into()
                .expect("公民宣言超出 MaxDeclarationLen");
            CitizensDeclaration::<T>::put(citizens);
            CountryDeclaration::<T>::put(country);
            CitizenMax::<T>::put(self.citizen_max);
            // 创世机构 seeding 通过注入执行(治理存储写入的宿主是 institution::build,
            // 由 runtime 的 InstitutionSeeder 实现调用)。
            <T::InstitutionSeeder as super::GenesisInstitutionSeeder>::seed();
        }
    }

    // 本 pallet 不暴露 extrinsic。阶段切换仅通过 OnRuntimeUpgrade 迁移执行。
}

// ─── DeveloperUpgradeCheck 实现 ────────────────────────────────────────────
impl<T: pallet::Config> DeveloperUpgradeCheck for pallet::Pallet<T> {
    fn is_enabled() -> bool {
        // 开发者直升是创世/开发期专属能力。硬连线到链阶段:一旦进入 Operation
        // (由正式上线那次 runtime 升级的迁移把 Phase 翻到 Operation)即自动拒绝,
        // 不依赖任何人记得同步翻 DeveloperUpgradeEnabled 开关(fail-safe)。
        pallet::DeveloperUpgradeEnabled::<T>::get()
            && pallet::Phase::<T>::get() != pallet::ChainPhase::Operation
    }
}

// ─── ChainPhaseCheck 实现 ───────────────────────────────────────────────────
// 供 admin/entity 层读运行期强制门控:管理员字段强制仅在 Operation 期生效。
// trait 定义在 admin-primitives(本 pallet 反向依赖各 admin/entity pallet,不能定义于此)。
impl<T: pallet::Config> admin_primitives::ChainPhaseCheck for pallet::Pallet<T> {
    fn is_operation() -> bool {
        pallet::Phase::<T>::get() == pallet::ChainPhase::Operation
    }
}

#[cfg(test)]
mod tests;
