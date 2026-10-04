//! GPU PoW miner using OpenCL.
//!
//! This module is only compiled when the `gpu-mining` feature is enabled.
//! It provides a GPU-accelerated blake2b-256 hash search that runs alongside
//! the CPU miner, using the upper half of the nonce space (bit 63 = 1).

use crate::core::service::SimplePow;
use citizenchain::opaque::Block;
use codec::Encode;
use ocl::{Buffer, MemFlags, ProQue};
use sc_consensus_pow::MiningHandle;
use sp_core::U256;
use std::{
    sync::atomic::{AtomicU64, Ordering},
    thread,
    time::{Duration, Instant},
};

/// GPU 哈希率（hashes/sec），以 f64 的 bits 存储在 AtomicU64 中。
/// 全局变量，供 RPC 接口读取。
static GPU_HASHRATE: AtomicU64 = AtomicU64::new(0);

/// 获取当前 GPU 哈希率（hashes/sec）。
pub fn gpu_hashrate() -> f64 {
    f64::from_bits(GPU_HASHRATE.load(Ordering::Relaxed))
}

/// OpenCL kernel source embedded at compile time.
const KERNEL_SRC: &str = include_str!("blake2b_pow.cl");

/// Number of nonces to test per GPU batch dispatch.
/// 2^24 = ~16 million — good balance between GPU utilization and responsiveness.
const DEFAULT_BATCH_SIZE: u32 = 1 << 24;

/// GPU miner state holding OpenCL resources.
struct GpuMiner {
    pro_que: ProQue,
    // Persistent GPU buffers (reused across batches).
    buf_pre_hash: Buffer<u8>,
    buf_target: Buffer<u64>,
    buf_result_nonce: Buffer<u64>,
    buf_found: Buffer<u32>,
    batch_size: u32,
}

impl GpuMiner {
    /// Try to initialize the GPU miner on the given device.
    /// Returns Err if no GPU is available or OpenCL initialization fails.
    fn try_init(device_index: usize) -> Result<Self, String> {
        let platform = ocl::Platform::default();
        let devices = ocl::Device::list(platform, Some(ocl::flags::DeviceType::GPU))
            .map_err(|e| format!("failed to list GPU devices: {e}"))?;

        if devices.is_empty() {
            return Err("no GPU devices found".into());
        }

        let device = devices
            .get(device_index)
            .ok_or_else(|| {
                format!(
                    "GPU device index {} out of range (found {} devices)",
                    device_index,
                    devices.len()
                )
            })?
            .clone();

        let device_name = device.name().unwrap_or_else(|_| "unknown".into());
        log::info!(
            "Initializing GPU miner on device {}: {}",
            device_index,
            device_name
        );

        let batch_size = DEFAULT_BATCH_SIZE;

        let pro_que = ProQue::builder()
            .platform(platform)
            .device(device)
            .src(KERNEL_SRC)
            .dims(batch_size as usize)
            .build()
            .map_err(|e| format!("failed to build OpenCL program: {e}"))?;

        let buf_pre_hash = Buffer::<u8>::builder()
            .queue(pro_que.queue().clone())
            .flags(MemFlags::new().read_only())
            .len(32)
            .build()
            .map_err(|e| format!("failed to create pre_hash buffer: {e}"))?;

        let buf_target = Buffer::<u64>::builder()
            .queue(pro_que.queue().clone())
            .flags(MemFlags::new().read_only())
            .len(4)
            .build()
            .map_err(|e| format!("failed to create target buffer: {e}"))?;

        let buf_result_nonce = Buffer::<u64>::builder()
            .queue(pro_que.queue().clone())
            .flags(MemFlags::new().write_only())
            .len(1)
            .build()
            .map_err(|e| format!("failed to create result_nonce buffer: {e}"))?;

        let buf_found = Buffer::<u32>::builder()
            .queue(pro_que.queue().clone())
            .flags(MemFlags::new().read_write())
            .len(1)
            .build()
            .map_err(|e| format!("failed to create found buffer: {e}"))?;

        Ok(GpuMiner {
            pro_que,
            buf_pre_hash,
            buf_target,
            buf_result_nonce,
            buf_found,
            batch_size,
        })
    }

    /// Run a single batch of nonce searches on the GPU.
    /// Returns Some(nonce) if a valid nonce is found, None otherwise.
    fn search_batch(
        &self,
        pre_hash: &[u8],
        target_be: &[u64; 4],
        nonce_base: u64,
    ) -> Result<Option<u64>, String> {
        // Upload pre_hash and target to GPU.
        self.buf_pre_hash
            .write(pre_hash)
            .enq()
            .map_err(|e| format!("write pre_hash: {e}"))?;
        self.buf_target
            .write(target_be.as_slice())
            .enq()
            .map_err(|e| format!("write target: {e}"))?;

        // Reset found flag to 0.
        self.buf_found
            .write(&[0u32] as &[u32])
            .enq()
            .map_err(|e| format!("reset found: {e}"))?;

        // Build and enqueue the kernel.
        let kernel = self
            .pro_que
            .kernel_builder("blake2b_pow_mine")
            .arg(&self.buf_pre_hash)
            .arg(nonce_base)
            .arg(&self.buf_target)
            .arg(&self.buf_result_nonce)
            .arg(&self.buf_found)
            .build()
            .map_err(|e| format!("build kernel: {e}"))?;

        unsafe {
            kernel.enq().map_err(|e| format!("enqueue kernel: {e}"))?;
        }

        // Read back results.
        let mut found = [0u32; 1];
        self.buf_found
            .read(&mut found as &mut [u32])
            .enq()
            .map_err(|e| format!("read found: {e}"))?;

        if found[0] != 0 {
            let mut result_nonce = [0u64; 1];
            self.buf_result_nonce
                .read(&mut result_nonce as &mut [u64])
                .enq()
                .map_err(|e| format!("read result_nonce: {e}"))?;
            Ok(Some(result_nonce[0]))
        } else {
            Ok(None)
        }
    }
}

/// Convert U256 difficulty to big-endian target bytes for the GPU kernel.
/// target = U256::MAX / difficulty, stored as 4 x u64 in big-endian word order.
fn difficulty_to_target_be(difficulty: U256) -> [u64; 4] {
    if difficulty.is_zero() {
        return [0u64; 4];
    }
    let target = U256::MAX / difficulty;
    let bytes: [u8; 32] = target.to_big_endian();
    // Convert 32 big-endian bytes to 4 big-endian u64 words.
    let mut words = [0u64; 4];
    for i in 0..4 {
        let offset = i * 8;
        words[i] = u64::from_be_bytes([
            bytes[offset],
            bytes[offset + 1],
            bytes[offset + 2],
            bytes[offset + 3],
            bytes[offset + 4],
            bytes[offset + 5],
            bytes[offset + 6],
            bytes[offset + 7],
        ]);
    }
    words
}

/// Try to start the GPU miner. Spawns a background thread.
/// Returns Ok(()) if GPU initialization succeeded, Err otherwise.
pub fn try_start(
    worker: MiningHandle<Block, SimplePow, ()>,
    device_index: usize,
    pool_ready: std::sync::Arc<dyn Fn() -> usize + Send + Sync>,
    keystore: sp_keystore::KeystorePtr,
    author_public: sp_core::sr25519::Public,
) -> Result<(), String> {
    let miner = GpuMiner::try_init(device_index)?;

    thread::spawn(move || {
        let batch_size = miner.batch_size;

        loop {
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

            // GPU uses upper nonce space (bit 63 = 1).
            let random_base = {
                let seed_bytes = metadata.pre_hash.as_ref();
                let seed = u64::from_le_bytes(seed_bytes[..8].try_into().unwrap_or([0u8; 8]));
                seed | 0x8000000000000000
            };
            let mut nonce_base = random_base;

            let target_be = difficulty_to_target_be(metadata.difficulty);

            loop {
                if worker.version() != build_version {
                    break;
                }

                let batch_start = Instant::now();
                match miner.search_batch(metadata.pre_hash.as_ref(), &target_be, nonce_base) {
                    Ok(Some(nonce)) => {
                        let elapsed = batch_start.elapsed();
                        if elapsed.as_nanos() > 0 {
                            let hr = batch_size as f64 / elapsed.as_secs_f64();
                            GPU_HASHRATE.store(hr.to_bits(), Ordering::Relaxed);
                        }
                        // 有效工作量证明找到后立即提交；只防止提交已经过期的工作。
                        if worker.version() != build_version {
                            break;
                        }

                        // 签名 pre_hash 证明矿工身份，签名失败则丢弃该 nonce。
                        let signature = match keystore.sr25519_sign(
                            sp_core::crypto::KeyTypeId(*b"powr"),
                            &author_public,
                            metadata.pre_hash.as_ref(),
                        ) {
                            Ok(Some(sig)) => sig,
                            _ => {
                                log::warn!("GPU PoW: keystore 签名失败，丢弃 nonce");
                                break;
                            }
                        };
                        let seal = (nonce, sp_core::sr25519::Signature::from(signature)).encode();
                        let _submitted = futures::executor::block_on(worker.submit(seal));
                        break;
                    }
                    Ok(None) => {
                        // 更新哈希率统计。
                        let elapsed = batch_start.elapsed();
                        if elapsed.as_nanos() > 0 {
                            let hr = batch_size as f64 / elapsed.as_secs_f64();
                            GPU_HASHRATE.store(hr.to_bits(), Ordering::Relaxed);
                        }
                        // No solution in this batch, advance nonce_base.
                        nonce_base = nonce_base.wrapping_add(batch_size as u64);
                    }
                    Err(e) => {
                        log::error!("GPU mining error: {e}");
                        // Back off before retrying to avoid spamming logs.
                        thread::sleep(Duration::from_secs(5));
                        break;
                    }
                }
            }
        }
    });

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn difficulty_to_target_be_zero_returns_zeros() {
        assert_eq!(difficulty_to_target_be(U256::zero()), [0u64; 4]);
    }

    #[test]
    fn difficulty_to_target_be_one_returns_max() {
        let result = difficulty_to_target_be(U256::one());
        // U256::MAX / 1 = U256::MAX → 全 0xFF
        assert_eq!(result, [u64::MAX; 4]);
    }

    #[test]
    fn difficulty_to_target_be_two() {
        let result = difficulty_to_target_be(U256::from(2));
        let target = U256::MAX / U256::from(2);
        let bytes: [u8; 32] = target.to_big_endian();
        let expected = bytes_to_words(&bytes);
        assert_eq!(result, expected);
    }

    #[test]
    fn difficulty_to_target_be_roundtrip() {
        let difficulty = U256::from(1000);
        let words = difficulty_to_target_be(difficulty);
        let target = U256::MAX / difficulty;
        let mut reconstructed = [0u8; 32];
        for (i, word) in words.iter().enumerate() {
            reconstructed[i * 8..(i + 1) * 8].copy_from_slice(&word.to_be_bytes());
        }
        assert_eq!(U256::from_big_endian(&reconstructed), target);
    }

    fn bytes_to_words(bytes: &[u8; 32]) -> [u64; 4] {
        let mut words = [0u64; 4];
        for i in 0..4 {
            let o = i * 8;
            words[i] = u64::from_be_bytes([
                bytes[o],
                bytes[o + 1],
                bytes[o + 2],
                bytes[o + 3],
                bytes[o + 4],
                bytes[o + 5],
                bytes[o + 6],
                bytes[o + 7],
            ]);
        }
        words
    }
}
