//! PoW 动态难度调整模块=pow-difficulty
//!
//! # 设计原理
//! 参考比特币 Nakamoto 难度调整算法：每隔 DIFFICULTY_ADJUSTMENT_INTERVAL 块，
//! 根据窗口期实际出块总时长与目标时长的比值，按比例调整 PoW 挖矿难度。
//!
//! # 调整公式
//! ```text
//! new_difficulty = old_difficulty × (target_window_ms / actual_window_ms)
//! ```
//! 并限制单次调整幅度在 [old/4, old×4] 范围内，防止难度暴涨或暴跌。
//!
//! # 时序说明
//! - 窗口起始时间戳在调整周期首块的 on_finalize 中记录（此时 pallet_timestamp 已完成时间戳注入）。
//! - 窗口终止时间戳在调整周期末块的 on_finalize 中读取并触发调整。
//! - 节点层直接读取 CurrentDifficulty RAW storage，避免 Runtime API 成为守卫绕路点。
//! - 当前算法只取窗口首尾两个时间点，不对窗口内每一块做采样；因此制度安全仍依赖时间戳 inherent 的有效性。
//! - runtime 在难度状态变更前拒绝只有 timestamp inherent 的空块；NodeGuard 的提前拒绝不能替代该共识规则。

#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(feature = "runtime-benchmarks")]
mod benchmarks;
pub mod weights;

pub use pallet::*;

use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use sp_runtime::Debug;

/// 可随合法 runtime 升级变更的 PoW 难度参数。
#[derive(
    Encode,
    Decode,
    DecodeWithMemTracking,
    Clone,
    Copy,
    Debug,
    TypeInfo,
    MaxEncodedLen,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
)]
#[serde(rename_all = "camelCase")]
pub struct PowDifficultyParams {
    pub params_version: u32,
    pub algorithm_version: u16,
    pub target_block_time_ms: u64,
    pub adjustment_interval: u32,
    pub max_adjust_up_factor: u64,
    pub max_adjust_down_divisor: u64,
}

impl PowDifficultyParams {
    /// 当前链创世默认值；这些数值不是节点永久常量，运行后以 ActiveParams 为唯一真源。
    pub const fn genesis_default() -> Self {
        Self {
            params_version: primitives::pow_const::POW_PARAMS_VERSION,
            algorithm_version: primitives::pow_const::POW_ALGORITHM_VERSION,
            target_block_time_ms: primitives::pow_const::POW_TARGET_BLOCK_TIME_MS,
            adjustment_interval: primitives::pow_const::DIFFICULTY_ADJUSTMENT_INTERVAL,
            max_adjust_up_factor: primitives::pow_const::DIFFICULTY_MAX_ADJUST_FACTOR,
            max_adjust_down_divisor: primitives::pow_const::DIFFICULTY_MIN_ADJUST_FACTOR,
        }
    }

    /// 只固定代数安全边界，不在节点和 runtime 中重复写死可治理的数值范围。
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.params_version == 0 {
            return Err("params_version 不得为 0");
        }
        if self.algorithm_version == 0 {
            return Err("algorithm_version 不得为 0");
        }
        if self.target_block_time_ms == 0 {
            return Err("target_block_time_ms 不得为 0");
        }
        if self.adjustment_interval == 0 {
            return Err("adjustment_interval 不得为 0");
        }
        if self.max_adjust_up_factor == 0 || self.max_adjust_down_divisor == 0 {
            return Err("难度调整倍率不得为 0");
        }
        self.target_block_time_ms
            .checked_mul(self.adjustment_interval as u64)
            .ok_or("目标窗口溢出")?;
        Ok(())
    }

    pub fn target_window_ms(&self) -> Option<u64> {
        self.target_block_time_ms
            .checked_mul(self.adjustment_interval as u64)
    }

    fn same_values_except_version(&self, other: &Self) -> bool {
        self.algorithm_version == other.algorithm_version
            && self.target_block_time_ms == other.target_block_time_ms
            && self.adjustment_interval == other.adjustment_interval
            && self.max_adjust_up_factor == other.max_adjust_up_factor
            && self.max_adjust_down_divisor == other.max_adjust_down_divisor
    }
}

impl Default for PowDifficultyParams {
    fn default() -> Self {
        Self::genesis_default()
    }
}

#[derive(
    Encode,
    Decode,
    DecodeWithMemTracking,
    Clone,
    Copy,
    Debug,
    TypeInfo,
    MaxEncodedLen,
    PartialEq,
    Eq,
)]
pub struct PendingPowDifficultyParams {
    pub params: PowDifficultyParams,
    pub activate_at: u32,
}

#[derive(
    Encode,
    Decode,
    DecodeWithMemTracking,
    Clone,
    Copy,
    Debug,
    TypeInfo,
    MaxEncodedLen,
    PartialEq,
    Eq,
)]
pub struct DifficultyAdjustmentAudit {
    pub block: u32,
    pub params_version: u32,
    pub old_difficulty: u64,
    pub new_difficulty: u64,
    pub window_start_block: u32,
    pub actual_window_ms: u64,
}

#[frame_support::pallet]
pub mod pallet {
    use super::{DifficultyAdjustmentAudit, PendingPowDifficultyParams, PowDifficultyParams};
    use frame_support::{pallet_prelude::*, traits::Time};
    use frame_system::pallet_prelude::*;
    use primitives::pow_const::POW_INITIAL_DIFFICULTY;
    use sp_runtime::traits::SaturatedConversion;

    use crate::weights::WeightInfo as PowDifficultyWeightInfo;

    const STORAGE_VERSION: StorageVersion = StorageVersion::new(0);

    #[pallet::pallet]
    #[pallet::storage_version(STORAGE_VERSION)]
    pub struct Pallet<T>(_);

    /// Pallet 配置：需要 frame_system、pallet_timestamp 作为超特征。
    /// pallet_timestamp 用于读取当前块时间戳；目标窗口固定取核心常量，禁止由链上状态修改。
    #[pallet::config]
    pub trait Config:
        frame_system::Config<RuntimeEvent: From<Event<Self>>> + pallet_timestamp::Config
    {
        type WeightInfo: crate::weights::WeightInfo;
    }

    // ─── Storage ──────────────────────────────────────────────────────────────

    /// 当前 PoW 挖矿难度值。创世时为 POW_INITIAL_DIFFICULTY，此后由调整算法自动维护。
    /// 正常路径下该值必须始终大于 0；若迁移/脏状态把它写成 0，on_finalize 会兜底修复到至少 1。
    #[pallet::storage]
    #[pallet::getter(fn current_difficulty)]
    pub type CurrentDifficulty<T> = StorageValue<_, u64, ValueQuery, DefaultInitialDifficulty>;

    /// 难度初始默认值（ValueQuery 的 OnEmpty 实现）。
    pub struct DefaultInitialDifficulty;
    impl Get<u64> for DefaultInitialDifficulty {
        fn get() -> u64 {
            POW_INITIAL_DIFFICULTY
        }
    }

    /// 当前调整窗口的起始时间戳（毫秒 Unix 时间）。
    /// None 表示本窗口起始时间尚未记录，将在当前周期首块的 on_finalize 中写入。
    #[pallet::storage]
    pub type WindowStartMs<T> = StorageValue<_, u64, OptionQuery>;

    /// 当前窗口起始高度；参数升级激活时与时间窗口一起重置。
    #[pallet::storage]
    pub type WindowStartBlock<T> = StorageValue<_, u32, OptionQuery>;

    /// 当前生效的唯一 PoW 参数真源。
    #[pallet::storage]
    #[pallet::getter(fn active_params)]
    pub type ActiveParams<T> = StorageValue<_, PowDifficultyParams, ValueQuery>;

    /// runtime 升级块写入、下一块原子激活的参数。
    #[pallet::storage]
    pub type PendingParams<T> = StorageValue<_, PendingPowDifficultyParams, OptionQuery>;

    /// 最近一次难度调整审计，供节点守卫与运维复核。
    #[pallet::storage]
    pub type LastAdjustment<T> = StorageValue<_, DifficultyAdjustmentAudit, OptionQuery>;

    #[pallet::genesis_config]
    pub struct GenesisConfig<T: Config> {
        pub params: PowDifficultyParams,
        pub initial_difficulty: u64,
        pub _marker: PhantomData<T>,
    }

    impl<T: Config> Default for GenesisConfig<T> {
        fn default() -> Self {
            Self {
                params: PowDifficultyParams::genesis_default(),
                initial_difficulty: POW_INITIAL_DIFFICULTY,
                _marker: PhantomData,
            }
        }
    }

    #[pallet::genesis_build]
    impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
        fn build(&self) {
            assert!(self.params.validate().is_ok(), "PoW 创世参数无效");
            assert!(self.initial_difficulty > 0, "PoW 创世难度不得为 0");
            ActiveParams::<T>::put(self.params);
            CurrentDifficulty::<T>::put(self.initial_difficulty);
        }
    }

    // ─── Events ───────────────────────────────────────────────────────────────

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        /// 难度调整完成。
        /// [触发区块高度, 旧难度, 新难度, 窗口实际耗时ms, 目标窗口时间ms]
        DifficultyAdjusted {
            block: BlockNumberFor<T>,
            old_difficulty: u64,
            new_difficulty: u64,
            actual_window_ms: u64,
            target_window_ms: u64,
        },
        /// 脏状态防御:on_finalize 检测到链上 PoW 参数非法/算法版本不符/目标窗口缺失,
        /// 本块跳过难度调整(难度维持现值),等治理升级修复。绝不 panic 拖垮全链。
        PowParamsUnhealthy { block: BlockNumberFor<T> },
        /// 脏状态防御:难度调整窗口被跳过(elapsed 超过 adjustment_interval),
        /// 以当前块重置窗口起点自愈,本块不调整难度。绝不 panic 拖垮全链。
        PowWindowReset { block: BlockNumberFor<T> },
    }

    // ─── Hooks ────────────────────────────────────────────────────────────────

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
        /// try-runtime 状态校验：确保 CurrentDifficulty 始终为正数。
        #[cfg(feature = "try-runtime")]
        fn try_state(_n: BlockNumberFor<T>) -> Result<(), sp_runtime::TryRuntimeError> {
            let diff = CurrentDifficulty::<T>::get();
            frame_support::ensure!(diff > 0, "CurrentDifficulty 不得为 0");
            let params = ActiveParams::<T>::get();
            frame_support::ensure!(params.validate().is_ok(), "ActiveParams 无效");
            frame_support::ensure!(
                params.algorithm_version == primitives::pow_const::POW_ALGORITHM_VERSION,
                "ActiveParams 算法版本不受当前 runtime 支持"
            );
            if let Some(pending) = PendingParams::<T>::get() {
                frame_support::ensure!(pending.params.validate().is_ok(), "PendingParams 无效");
                frame_support::ensure!(
                    pending.params.algorithm_version
                        == primitives::pow_const::POW_ALGORITHM_VERSION,
                    "PendingParams 算法版本不受当前 runtime 支持"
                );
                frame_support::ensure!(
                    pending.params.params_version == params.params_version.saturating_add(1),
                    "PendingParams 版本必须精确加一"
                );
            }
            Ok(())
        }

        fn on_initialize(n: BlockNumberFor<T>) -> Weight {
            let block_num: u32 = n.saturated_into();
            if block_num == 0 {
                return Weight::zero();
            }

            if let Some(pending) = PendingParams::<T>::get() {
                // 脏状态防御:正常情况 activate_at 是未来块,on_initialize 逐块推进到相等时激活。
                // 若因异常已错过(activate_at < block_num),不再 panic 拖垮全链,改用 `<=`
                // 在首个到达/越过的块补激活——治理批准的参数不丢,也不永久卡死。
                if pending.activate_at <= block_num {
                    ActiveParams::<T>::put(pending.params);
                    PendingParams::<T>::kill();
                    WindowStartBlock::<T>::kill();
                    WindowStartMs::<T>::kill();
                    // 参数激活块会在 on_finalize 以当前块重新建立窗口；单独返回
                    // 激活路径权重，完整覆盖参数切换和窗口重置产生的读写。
                    return <T as Config>::WeightInfo::on_initialize_activate_params();
                }
            }

            let params = ActiveParams::<T>::get();
            let is_adjustment_block = WindowStartBlock::<T>::get()
                .map(|start| block_num.saturating_sub(start) == params.adjustment_interval)
                .unwrap_or(false);

            if is_adjustment_block {
                // 实际重点在 on_finalize，但 FRAME 只能在 on_initialize 预申报预算。
                <T as Config>::WeightInfo::on_initialize_adjustment()
            } else if WindowStartMs::<T>::get().is_none() {
                <T as Config>::WeightInfo::on_initialize_start_window()
            } else {
                <T as Config>::WeightInfo::on_initialize_idle()
            }
        }

        /// on_finalize 在 pallet_timestamp 的时间戳注入（inherent）之后执行，
        /// 因此可安全调用 pallet_timestamp::Pallet::<T>::now() 获取当前块时间戳。
        fn on_finalize(n: BlockNumberFor<T>) {
            let block_num: u32 = n.saturated_into();
            let now_ms: u64 = pallet_timestamp::Pallet::<T>::now().saturated_into();

            // 跳过创世块（block 0 无时间戳注入）
            if now_ms == 0 {
                return;
            }

            // runtime 是空块规则的最终共识闸门：即使出块节点删除或绕过 NodeGuard，
            // 诚实节点重新执行正式 runtime WASM 时仍会拒绝只有 timestamp inherent 的区块。
            // 此检查必须先于窗口起点或当前难度写入，确保无效空块不能推进难度状态。
            let extrinsic_count = frame_system::Pallet::<T>::extrinsic_count();
            assert!(
                extrinsic_count > 1,
                "空块不允许上链：区块必须包含 timestamp 之外的交易"
            );

            // 脏状态防御:链上参数非法 / 算法版本不符 / 目标窗口缺失时,难度调整无法安全进行。
            // 与空块闸门不同,这类脏状态不是共识裁决,绝不 panic(否则每块必崩=全链永久停摆)。
            // 改为发告警事件 + 跳过本块调整,难度维持现值,等治理升级修复。
            let params = ActiveParams::<T>::get();
            let params_healthy = params.validate().is_ok()
                && params.algorithm_version == primitives::pow_const::POW_ALGORITHM_VERSION;
            let Some(target_window_ms) = params.target_window_ms().filter(|_| params_healthy)
            else {
                Self::deposit_event(Event::PowParamsUnhealthy { block: n });
                return;
            };
            let window_start_block = WindowStartBlock::<T>::get();
            let elapsed_blocks = window_start_block.map(|start| block_num.saturating_sub(start));
            // 脏状态防御:窗口被跳过(elapsed 超过 adjustment_interval)时不再 panic,
            // 以当前块重置窗口起点自愈,本块不调整难度。
            if elapsed_blocks
                .map(|elapsed| elapsed > params.adjustment_interval)
                .unwrap_or(false)
            {
                Self::deposit_event(Event::PowWindowReset { block: n });
                WindowStartMs::<T>::put(now_ms);
                WindowStartBlock::<T>::put(block_num);
                return;
            }
            let is_adjustment_block = elapsed_blocks == Some(params.adjustment_interval);

            if is_adjustment_block {
                // ── 调整块：计算新难度 ────────────────────────────────────────
                if let Some(start_ms) = WindowStartMs::<T>::get() {
                    let actual_window_ms = now_ms.saturating_sub(start_ms).max(1);
                    // 六分钟仅是全链固定的平均目标；PoW 找到即出块，不设置最短或最晚期限。
                    let old_difficulty = CurrentDifficulty::<T>::get();
                    // 正常情况下 old_difficulty 不会为 0；这里做兜底是为了防止
                    // 迁移错误或脏状态把 clamp 的上下界反转，进而在调整块上触发 panic。
                    let calc_difficulty = old_difficulty.max(1);

                    // 新难度 = 旧难度 × (目标时间 / 实际时间)
                    // 出块过快 → actual < target → 新难度升高（更难挖）
                    // 出块过慢 → actual > target → 新难度降低（更易挖）
                    let new_diff_u128 = (calc_difficulty as u128)
                        .saturating_mul(target_window_ms as u128)
                        / actual_window_ms as u128;

                    // 单次调整幅度限制按“参与计算的安全难度”夹紧；
                    // 即便存储里出现 0，也只会被修复为 >= 1，而不会把链直接打崩。
                    let max_diff = calc_difficulty.saturating_mul(params.max_adjust_up_factor);
                    let min_diff = (calc_difficulty / params.max_adjust_down_divisor).max(1);
                    let new_diff = new_diff_u128.saturated_into::<u64>();
                    let new_difficulty = new_diff.clamp(min_diff, max_diff);

                    CurrentDifficulty::<T>::put(new_difficulty);
                    LastAdjustment::<T>::put(DifficultyAdjustmentAudit {
                        block: block_num,
                        params_version: params.params_version,
                        old_difficulty,
                        new_difficulty,
                        window_start_block: window_start_block.unwrap_or(block_num),
                        actual_window_ms,
                    });

                    Self::deposit_event(Event::DifficultyAdjusted {
                        block: n,
                        old_difficulty,
                        new_difficulty,
                        actual_window_ms,
                        target_window_ms,
                    });
                }
                // 以当前调整块时间戳作为下一窗口起点，避免少算 1 个区块间隔。
                WindowStartMs::<T>::put(now_ms);
                WindowStartBlock::<T>::put(block_num);
            } else {
                // ── 非调整块：若窗口起始未记录，以当前块时间戳为起点 ──────────
                if WindowStartMs::<T>::get().is_none() || WindowStartBlock::<T>::get().is_none() {
                    WindowStartMs::<T>::put(now_ms);
                    WindowStartBlock::<T>::put(block_num);
                }
            }
        }
    }

    impl<T: Config> Pallet<T> {
        /// 仅供 runtime-upgrade 的原子升级执行器调用；本 pallet 不暴露普通 extrinsic。
        pub fn stage_params(new_params: PowDifficultyParams, activate_at: u32) -> DispatchResult {
            new_params
                .validate()
                .map_err(|_| DispatchError::Other("invalid pow difficulty params"))?;
            ensure!(
                new_params.algorithm_version == primitives::pow_const::POW_ALGORITHM_VERSION,
                "当前 runtime 不支持该 PoW 算法版本"
            );
            ensure!(PendingParams::<T>::get().is_none(), "已有待生效 PoW 参数");

            let active = ActiveParams::<T>::get();
            if active.same_values_except_version(&new_params) {
                ensure!(
                    new_params.params_version == active.params_version,
                    "参数未变时版本不得变化"
                );
                return Ok(());
            }

            ensure!(
                new_params.params_version == active.params_version.saturating_add(1),
                "PoW 参数版本必须精确加一"
            );
            PendingParams::<T>::put(PendingPowDifficultyParams {
                params: new_params,
                activate_at,
            });
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests;
