// 挖矿模块入口，聚合挖矿收益、网络概览、出块记录与 GPU 挖矿能力。
pub mod dashboard;
#[cfg(feature = "gpu-mining")]
pub(crate) mod gpu_miner;
pub mod network_overview;
