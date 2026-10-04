// genesis-pallet 无 extrinsic，WeightInfo 为空实现。

pub trait WeightInfo {}

pub struct SubstrateWeight<T>(core::marker::PhantomData<T>);

impl<T: frame_system::Config> WeightInfo for SubstrateWeight<T> {}

impl WeightInfo for () {}
