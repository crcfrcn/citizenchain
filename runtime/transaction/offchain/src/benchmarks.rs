//! 清算行(L2)扫码支付清算 pallet benchmarks。
//!
//! `weights.rs` 接入非零保守权重和 `T::WeightInfo`。
//! 这里保留 benchmark 模块入口,等待专用 benchmarking runtime WASM 准备后,
//! 再按同名 `WeightInfo` 方法生成正式权重文件。

#![cfg(feature = "runtime-benchmarks")]

// 当前 crate 暂未提供可执行 benchmark。这里不能保留空的
// `#[benchmarks]` 模块，否则 frame-benchmarking 宏会生成非法代码并阻断
// runtime-benchmarks 构建。正式补齐 offchain-transaction benchmark 时，再恢复
// `bind_clearing_bank` / `deposit` / `withdraw` / `switch_bank` 等入口。
