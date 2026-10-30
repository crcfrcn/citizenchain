//! CPU矿工线程、哈希率与停止后的资源释放。

// Substrate service API 固定返回 sc_service::Error，Node 必须保持框架函数签名。
#![allow(clippy::result_large_err)]
use runtime;

use crate::pow::hash_meets_difficulty;
use crate::pow::pow_hash;
use crate::pow::SimplePow;
use crate::pow::POW_AUTHOR_KEY_TYPE;

use codec::Encode;

use runtime::opaque::Block;

use sc_consensus_pow::MiningHandle;
use sc_consensus_pow::PowAlgorithm;

use sc_network::NetworkBackend as _;

use sp_api::Core as _;
use sp_api::Metadata as _;

use sp_core::sr25519;

use sp_keystore::Keystore;
use sp_runtime::traits::Block as BlockT;

use std::sync::atomic::AtomicBool;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use std::thread;
use std::time::Duration;
use std::time::Instant;

/// CPU 全线程合计哈希率（hashes/sec），以 f64 bits 存入 AtomicU64。
static CPU_HASHRATE: AtomicU64 = AtomicU64::new(0);

// 空块 proposal 防护由 mining worker 的 should_propose 与 CPU/GPU 交易池门控共同实现。

/// 获取当前 CPU 哈希率（hashes/sec）。
pub fn cpu_hashrate() -> f64 {
    f64::from_bits(CPU_HASHRATE.load(Ordering::Relaxed))
}

// CPU 矿工归本次服务所有；停止时先通知所有线程，再等待其释放交易池和客户端引用。
pub struct CpuMinerThreads {
    stopping: Arc<AtomicBool>,
    threads: Vec<thread::JoinHandle<()>>,
}

impl CpuMinerThreads {
    pub fn new() -> Self {
        Self {
            stopping: Arc::new(AtomicBool::new(false)),
            threads: Vec::new(),
        }
    }

    pub fn spawn(&mut self, run: impl FnOnce(Arc<AtomicBool>) + Send + 'static) {
        let stopping = self.stopping.clone();
        self.threads.push(thread::spawn(move || run(stopping)));
    }
}

impl Drop for CpuMinerThreads {
    fn drop(&mut self) {
        self.stopping.store(true, Ordering::Release);
        for handle in self.threads.drain(..) {
            if handle.join().is_err() {
                log::error!("CPU矿工线程退出时发生panic");
            }
        }
        CPU_HASHRATE.store(0, Ordering::Relaxed);
    }
}

pub fn start_cpu_miner(
    worker: MiningHandle<Block, SimplePow, ()>,
    num_threads: usize,
    pool_ready: Arc<dyn Fn() -> usize + Send + Sync>,
    keystore: sp_keystore::KeystorePtr,
    author_public: sr25519::Public,
) -> CpuMinerThreads {
    let stride = (num_threads as u64).max(1);
    let mut miners = crate::cpu::CpuMinerThreads::new();

    for thread_id in 0..num_threads {
        let worker = worker.clone();
        let pool_ready = pool_ready.clone();
        let keystore = keystore.clone();
        miners.spawn(move |stopping| {
            // 哈希率采样：仅 thread 0 每 SAMPLE_INTERVAL 次哈希统计一次，乘以线程数得到总哈希率。
            const SAMPLE_INTERVAL: u64 = 100_000;
            let mut sample_count: u64 = 0;
            let mut sample_start = Instant::now();

            while !stopping.load(Ordering::Acquire) {
                let Some(metadata) = worker.metadata() else {
                    thread::sleep(Duration::from_millis(200));
                    continue;
                };

                // 空块不提交：交易池无待打包交易时不挖矿，避免产生空块。
                if pool_ready() == 0 {
                    thread::sleep(Duration::from_millis(500));
                    continue;
                }

                let build_version = worker.version();

                // 共同随机基址（来自 pre_hash 前 8 字节）+ 线程号错位 + stride = 线程数。
                // 每轮 metadata 变化时基址自动更换；同一轮内各线程搜索的 nonce 集合完全不重叠。
                let random_base = {
                    let seed_bytes = metadata.pre_hash.as_ref();
                    let seed = u64::from_le_bytes(seed_bytes[..8].try_into().unwrap_or([0u8; 8]));
                    // CPU 使用低半区 nonce（bit 63 = 0），高半区留给 GPU。
                    seed & 0x7FFFFFFFFFFFFFFF
                };
                let mut nonce = random_base.wrapping_add(thread_id as u64);

                while !stopping.load(Ordering::Acquire) {
                    if worker.version() != build_version {
                        break;
                    }

                    // thread 0 负责采样更新全局哈希率。
                    if thread_id == 0 {
                        sample_count += 1;
                        if sample_count >= SAMPLE_INTERVAL {
                            let elapsed = sample_start.elapsed();
                            if elapsed.as_nanos() > 0 {
                                let per_thread = sample_count as f64 / elapsed.as_secs_f64();
                                let total = per_thread * stride as f64;
                                CPU_HASHRATE.store(total.to_bits(), Ordering::Relaxed);
                            }
                            sample_count = 0;
                            sample_start = Instant::now();
                        }
                    }

                    let hash = pow_hash(metadata.pre_hash.as_ref(), nonce);
                    if hash_meets_difficulty(&hash, metadata.difficulty) {
                        // 有效工作量证明找到后立即提交；只防止提交已经过期的工作。
                        if worker.version() != build_version {
                            break; // nonce 已过期，回外层重新获取 metadata
                        }

                        // 签名 pre_hash 证明矿工身份，签名失败则丢弃该 nonce。
                        let signature = match keystore.sr25519_sign(
                            POW_AUTHOR_KEY_TYPE,
                            &author_public,
                            metadata.pre_hash.as_ref(),
                        ) {
                            Ok(Some(sig)) => sig,
                            _ => {
                                log::warn!("PoW: keystore 签名失败，丢弃 nonce");
                                break;
                            }
                        };
                        let seal = (nonce, sr25519::Signature::from(signature)).encode();
                        if !stopping.load(Ordering::Acquire) {
                            let _submitted = futures::executor::block_on(worker.submit(seal));
                        }
                        break;
                    }

                    nonce = nonce.wrapping_add(stride);
                }
            }
        });
    }
    miners
}

#[cfg(test)]
#[path = "../tests/cpu.rs"]
mod tests;

#[cfg(test)]
#[path = "../tests/integration.rs"]
mod integration_tests;
