//! 个人多签管理员 pallet benchmark 占位。
//!
//! 此处仅保留模块编译入口;实际 benchmark 用例待 follow-up:
//! - propose_admin_set_change
//!
//! weights.rs 当前为保守非零权重,benchmark 补齐后再用实测值覆盖。

#![cfg(feature = "runtime-benchmarks")]

// 暂无 benchmark 用例;空文件保持模块编译入口存在,供 lib.rs 内
// `#[cfg(feature = "runtime-benchmarks")] mod benchmarks;` 引用通过。
// 后续补 benchmark 用例时,使用 `#[frame_benchmarking::v2::benchmarks]` 装饰一组
// 带 `#[extrinsic_call]` 标注的函数即可。
