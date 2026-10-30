//! 现有选举pallet定义，保持无业务调用、事件和存储。
#[frame_support::pallet]
pub mod pallet {
    #[pallet::config]
    pub trait Config: frame_system::Config {}

    #[pallet::pallet]
    pub struct Pallet<T>(_);
}
