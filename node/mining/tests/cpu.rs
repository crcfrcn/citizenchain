//! 验证空CPU矿工空闲及释放时撤销停止标记。

use super::*;

#[test]
fn empty_cpu_worker_owner_starts_idle_and_drop_revokes_its_stop_flag() {
    let workers = CpuMinerThreads::new();
    assert!(workers.threads.is_empty());
    let stopping = workers.stopping.clone();
    assert!(!stopping.load(Ordering::Acquire));
    drop(workers);
    assert!(stopping.load(Ordering::Acquire));
}
