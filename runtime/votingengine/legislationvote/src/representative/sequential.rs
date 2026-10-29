//! 多个代表机构顺序表决的阶段推进。

use crate::{pallet::Config, Pallet};
use frame_support::dispatch::DispatchResult;

impl<T: Config> Pallet<T> {
    /// 当前机构通过后进入下一个机构；全部机构通过后进入终局或法律专属程序。
    pub(crate) fn advance_sequential_representative_vote(proposal_id: u64) -> DispatchResult {
        Self::advance_representative_body_or_finish(proposal_id)
    }
}
