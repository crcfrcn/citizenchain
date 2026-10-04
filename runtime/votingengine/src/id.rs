//! 提案 ID 体系（双层 ID 设计）。
//!
//! - **主键 `proposal_id: u64`** 全局纯单调递增,实质无上限(1.84×10¹⁹)。
//!   所有 storage map(`Proposals` / `ProposalData` / `InternalVotesByTicket` 等)
//!   都以这个 u64 为主键,跨业务、跨机构、跨年全局唯一不重号。
//! - **展示号** `(year, seq_in_year)` 单独存于 `ProposalDisplayId[id]`。
//!   渲染层基于该表拼接 "2026-#000123" 类格式;展示格式想换季度制 / 字母分组
//!   只动渲染层,主键和存储不动。
//!
//! 双层 ID 路径:主键单调 + ProposalDisplayId 同事务写入。

use frame_support::traits::UnixTime;
use sp_runtime::DispatchError;

use crate::pallet::{
    self, CurrentProposalYear, Error, NextProposalId, ProposalDisplayId, YearProposalCounter,
};
use crate::ProposalDisplayMeta;

impl<T: pallet::Config> pallet::Pallet<T> {
    /// 分配提案 ID(双层设计):
    /// 1. 从 `NextProposalId` 取下一个 u64(全局单调,跨业务跨年都唯一)
    /// 2. 从 `YearProposalCounter` 取年内序号(跨年自动重置)
    /// 3. 同事务写 `ProposalDisplayId[id] = (year, seq_in_year)` 供渲染查
    pub fn allocate_proposal_id() -> Result<u64, DispatchError> {
        let now_ms = T::TimeProvider::now().as_millis();
        // 毫秒 → 秒 → 年份(UTC)
        let secs = u64::try_from(now_ms / 1000).map_err(|_| Error::<T>::ProposalIdOverflow)?;
        let year = Self::unix_seconds_to_year(secs)?;

        // 主键:全局单调累加,实质无上限
        let id = NextProposalId::<T>::mutate(|n| -> Result<u64, DispatchError> {
            let cur = *n;
            *n = n.checked_add(1).ok_or(Error::<T>::ProposalIdOverflow)?;
            Ok(cur)
        })?;

        // 年内序号:跨年自动重置;u32 上限 42.9 亿,实质无上限
        let stored_year = CurrentProposalYear::<T>::get();
        let seq_in_year = if stored_year != year {
            CurrentProposalYear::<T>::put(year);
            YearProposalCounter::<T>::put(1u32);
            0u32
        } else {
            let c = YearProposalCounter::<T>::get();
            YearProposalCounter::<T>::put(c.checked_add(1).ok_or(Error::<T>::YearCounterOverflow)?);
            c
        };

        // 同事务写展示号反查表
        ProposalDisplayId::<T>::insert(id, ProposalDisplayMeta { year, seq_in_year });

        Ok(id)
    }

    /// Unix 秒数转 UTC 公历年份。
    pub fn unix_seconds_to_year(secs: u64) -> Result<u16, DispatchError> {
        const SECS_PER_DAY: u64 = 86_400;
        const DAYS_PER_400_YEARS: u64 = 146_097;

        let mut days = secs / SECS_PER_DAY;
        let cycles = days / DAYS_PER_400_YEARS;
        let mut year = 1970u32
            .checked_add(
                u32::try_from(cycles)
                    .map_err(|_| Error::<T>::ProposalIdOverflow)?
                    .checked_mul(400)
                    .ok_or(Error::<T>::ProposalIdOverflow)?,
            )
            .ok_or(Error::<T>::ProposalIdOverflow)?;
        days %= DAYS_PER_400_YEARS;

        // 展示号年份必须按真实 UTC 公历年边界切换,
        // 不能使用平均年秒数,否则元旦附近会漂移到错误年份段。
        while days >= Self::days_in_year(year) as u64 {
            days -= Self::days_in_year(year) as u64;
            year = year.checked_add(1).ok_or(Error::<T>::ProposalIdOverflow)?;
        }

        u16::try_from(year).map_err(|_| Error::<T>::ProposalIdOverflow.into())
    }

    pub fn days_in_year(year: u32) -> u16 {
        if Self::is_leap_year(year) {
            366
        } else {
            365
        }
    }

    pub fn is_leap_year(year: u32) -> bool {
        year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400))
    }
}
