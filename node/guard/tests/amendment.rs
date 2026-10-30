use super::*;

use runtime::opaque::Block;
use sc_consensus::BlockCheckParams;
use sc_consensus::BlockImport;
use sc_consensus::BlockImportParams;
use sc_consensus::ImportResult;
use sp_consensus::Error as ConsensusError;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;

#[derive(Default)]
struct CountingImport {
    imports: AtomicUsize,
}

#[async_trait::async_trait]
impl BlockImport<Block> for CountingImport {
    type Error = ConsensusError;

    async fn check_block(
        &self,
        _block: BlockCheckParams<Block>,
    ) -> Result<ImportResult, Self::Error> {
        Ok(ImportResult::AlreadyInChain)
    }

    async fn import_block(
        &self,
        _block: BlockImportParams<Block>,
    ) -> Result<ImportResult, Self::Error> {
        self.imports.fetch_add(1, Ordering::SeqCst);
        Ok(ImportResult::AlreadyInChain)
    }
}

fn import_params(number: u32) -> BlockImportParams<Block> {
    use sp_consensus::BlockOrigin;
    use sp_core::H256;
    use sp_runtime::Digest;

    let header = runtime::opaque::Header::new(
        number,
        H256::repeat_byte(1),
        H256::repeat_byte(2),
        H256::repeat_byte(3),
        Digest::default(),
    );
    BlockImportParams::new(BlockOrigin::NetworkInitialSync, header)
}

#[test]
fn four_constitution_tamper_classes_return_known_bad_without_stopping_next_import() {
    let inner = crate::constitution::guard::tests::CountingImport::default();
    for (number, reason) in [
        (1, "不可修改条款被篡改"),
        (2, "manifest 被篡改"),
        (3, "历史版本被篡改"),
        (4, "修宪凭据被篡改"),
    ] {
        let result = futures::executor::block_on(crate::guard::import_if_verified(
            &inner,
            import_params(number),
            Err(reason.into()),
        ))
        .expect("非法宪法区块应返回导入结果而不是终止节点");
        assert_eq!(result, ImportResult::KnownBad);
        assert_eq!(inner.imports.load(Ordering::SeqCst), 0);
    }

    // 闸门无跨块污染：连续拒绝后，下一合法区块仍可正常委派给旧 runtime 的内层导入器。
    let accepted = futures::executor::block_on(crate::guard::import_if_verified(
        &inner,
        import_params(5),
        Ok(()),
    ))
    .expect("合法区块应在拒绝后继续导入");
    assert_eq!(accepted, ImportResult::AlreadyInChain);
    assert_eq!(inner.imports.load(Ordering::SeqCst), 1);
}
