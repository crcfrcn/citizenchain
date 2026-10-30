//! 仅开发测试目标挂载的原有夹具，不进入正式构建。
use super::*;
use crate::service::new_partial_inner;
use nodeclient::types::Service;

use sc_service::error::Error as ServiceError;
use sc_service::Configuration;

// 正式入口始终使用冻结身份；此独立入口不进入任何正式节点二进制。
pub fn new_partial_for_test(config: &Configuration) -> Result<Service, ServiceError> {
    let identity =
        nodeguard::guard::testing::TestGenesisIdentity::from_chain_spec(config.chain_spec.as_ref())
            .map_err(ServiceError::Other)?;
    new_partial_inner(config, Some(identity))
}
