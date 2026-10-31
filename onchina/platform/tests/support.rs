//! 公共真实Router测试构造，调用原生产分发器。
use super::*;
pub fn router_for(module: Arc<dyn modules::model::ModuleBusiness>) -> Router {
    build_routes(AppState {
        db: Db::empty_for_route_test(),
        rate_limiter: Arc::new(LocalRateLimiter::new()),
        modules: Arc::new(modules::ModuleCatalog::new(Some(module))),
    })
}

/// 平台自身回归选择公共宿主，不编入正式软件。
#[cfg(test)]
pub(super) fn local_module() -> Option<Arc<dyn modules::model::ModuleBusiness>> {
    None
}
