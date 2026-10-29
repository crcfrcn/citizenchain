//! Runtime benchmarks 占位。
//!
//! 框架阶段不实装,后续业务任务卡 A/B 落地时在此补 benchmark 用例,
//! 配合 weights.rs 的 SubstrateWeight 自动生成实际权重值。

#![cfg(feature = "runtime-benchmarks")]

// TODO: 业务任务卡 A 实装时启用 frame_benchmarking::benchmarks! 宏
//
// frame_benchmarking::benchmarks! {
//     issue { ... }: { Pallet::<T>::execute_issue(...) }
//     mint { ... }: { ... }
//     ...
//     impl_benchmark_test_suite!(Pallet, crate::tests::mock::new_test_ext(), crate::tests::mock::Test);
// }
