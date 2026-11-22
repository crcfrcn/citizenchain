//! 为节点服务测试提供部分服务装配和等待停止辅助。

use crate::service::new_partial_inner;
use nodeclient::types::Service;

use sc_service::error::Error as ServiceError;
use sc_service::Configuration;

// 正式入口始终使用冻结身份；此独立入口不进入任何正式节点二进制。
pub fn new_partial_for_test(config: &Configuration) -> Result<Service, ServiceError> {
    let identity =
        nodeguard::guard::testing::TestGenesisIdentity::from_chain_spec(config.chain_spec.as_ref())
            .map_err(ServiceError::Other)?;
    // 两类守卫共享由此声明构建的内存基准，测试入口不打开生产数据。
    new_partial_inner(config, Some(identity))
}

/// 所有隔离服务先释放后台任务和数据库，再允许同库重开或清场。
pub fn stop_partial(partial: Service, runtime: &tokio::runtime::Runtime) {
    let released = std::sync::Arc::downgrade(&partial.backend);
    let sc_service::PartialComponents {
        client,
        backend,
        task_manager,
        keystore_container,
        select_chain,
        import_queue,
        transaction_pool,
        other,
    } = partial;
    let tasks = task_manager.into_task_registry();
    drop((
        client,
        backend,
        keystore_container,
        select_chain,
        import_queue,
        transaction_pool,
        other,
    ));
    runtime.block_on(async {
        tokio::time::timeout(std::time::Duration::from_secs(30), async {
            while released.upgrade().is_some() || !tasks.running_tasks().is_empty() {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("隔离服务退出必须释放全部任务及数据库");
    });
}
