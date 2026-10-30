use super::*;

#[test]
fn cpu_miner_shutdown_joins_idle_and_busy_threads_and_releases_resources() {
    let mut miners = crate::cpu::CpuMinerThreads::new();
    let resource = Arc::new(());
    let weak = Arc::downgrade(&resource);
    let exits = Arc::new(AtomicU64::new(0));
    let (ready, started) = std::sync::mpsc::channel();
    for busy in [false, true] {
        let resource = resource.clone();
        let exits = exits.clone();
        let ready = ready.clone();
        miners.spawn(move |stopping| {
            let _resource = resource;
            ready.send(()).unwrap();
            while !stopping.load(Ordering::Acquire) {
                if busy {
                    std::hint::spin_loop();
                } else {
                    thread::sleep(Duration::from_millis(10));
                }
            }
            exits.fetch_add(1, Ordering::Release);
        });
    }
    drop(ready);
    for _ in 0..2 {
        started.recv_timeout(Duration::from_secs(10)).unwrap();
    }
    drop(resource);
    assert!(weak.upgrade().is_some());
    drop(miners);
    assert_eq!(exits.load(Ordering::Acquire), 2);
    assert!(
        weak.upgrade().is_none(),
        "停止返回前必须释放全部线程捕获资源"
    );
    drop(crate::cpu::CpuMinerThreads::new());
}
