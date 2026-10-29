//! 单个代表机构表决完成后的状态推进。

use crate::{pallet::Config, Pallet};
use frame_support::dispatch::DispatchResult;

impl<T: Config> Pallet<T> {
    /// 单机构路线完成后进入提案配置的终局或法律专属程序。
    pub(crate) fn finish_single_representative_vote(proposal_id: u64) -> DispatchResult {
        Self::finish_representative_route(proposal_id)
    }
}
