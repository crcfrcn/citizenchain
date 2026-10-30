use super::*;

use crate::guard::enforce_genesis_identity;
use crate::guard::testing::TestGenesisIdentity;
use crate::guard::verify_genesis_identity;
use crate::guard::verify_genesis_identity_against;
use runtime::opaque::Block;
use sp_blockchain::HeaderBackend;
use sp_core::H256;

#[derive(Clone)]
struct TestClient {
    recorded_hash: H256,
    indexed_hash: Option<H256>,
    header: Option<runtime::opaque::Header>,
    hash_read_error: bool,
    header_read_error: bool,
}

impl TestClient {
    fn canonical() -> Self {
        let recorded_hash = H256::from(primitives::genesis::GENESIS_HASH);
        Self {
            recorded_hash,
            indexed_hash: Some(recorded_hash),
            // 冻结块 0：空 parent/digest，以及空 extrinsics trie 的实际根。
            header: Some(runtime::opaque::Header::new(
                0,
                H256::from(hex_literal::hex!(
                    "03170a2e7597b7b7e3d84c05391d139a62b157e78786d8c082f29dcf4c111314"
                )),
                H256::from(primitives::genesis::GENESIS_STATE_ROOT),
                H256::zero(),
                Default::default(),
            )),
            hash_read_error: false,
            header_read_error: false,
        }
    }
}

impl HeaderBackend<Block> for TestClient {
    fn header(&self, hash: H256) -> sp_blockchain::Result<Option<runtime::opaque::Header>> {
        assert_eq!(Some(hash), self.indexed_hash);
        if self.header_read_error {
            return Err(sp_blockchain::Error::Backend("header read failed".into()));
        }
        Ok(self.header.clone())
    }

    fn info(&self) -> sp_blockchain::Info<Block> {
        sp_blockchain::Info {
            best_hash: self.recorded_hash,
            best_number: 0,
            genesis_hash: self.recorded_hash,
            finalized_hash: self.recorded_hash,
            finalized_number: 0,
            finalized_state: Some((self.recorded_hash, 0)),
            number_leaves: 1,
            block_gap: None,
        }
    }

    fn status(&self, hash: H256) -> sp_blockchain::Result<sp_blockchain::BlockStatus> {
        Ok(if Some(hash) == self.indexed_hash {
            sp_blockchain::BlockStatus::InChain
        } else {
            sp_blockchain::BlockStatus::Unknown
        })
    }

    fn number(&self, hash: H256) -> sp_blockchain::Result<Option<u32>> {
        Ok(self.header(hash)?.map(|header| *header.number()))
    }

    fn hash(&self, number: u32) -> sp_blockchain::Result<Option<H256>> {
        assert_eq!(number, 0);
        if self.hash_read_error {
            return Err(sp_blockchain::Error::Backend("hash read failed".into()));
        }
        Ok(self.indexed_hash)
    }
}

#[test]
fn canonical_genesis_passes_recomputed_header_and_startup_guard() {
    let client = crate::guard::genesis_identity_tests::TestClient::canonical();
    assert_eq!(verify_genesis_identity(&client), Ok(()));
    enforce_genesis_identity(&client);
}

#[test]
fn declared_test_identity_does_not_change_frozen_production_identity() {
    let mut client = crate::guard::genesis_identity_tests::TestClient::canonical();
    client
        .header
        .as_mut()
        .expect("header")
        .set_state_root(H256::repeat_byte(7));
    let identity = TestGenesisIdentity {
        hash: client.header.as_ref().expect("header").hash(),
        state_root: H256::repeat_byte(7),
    };
    client.recorded_hash = identity.hash;
    client.indexed_hash = Some(identity.hash);
    assert_eq!(
        verify_genesis_identity_against(&client, identity.hash, identity.state_root),
        Ok(())
    );
    assert!(verify_genesis_identity(&client).is_err());
    client
        .header
        .as_mut()
        .expect("header")
        .set_state_root(H256::repeat_byte(8));
    assert!(verify_genesis_identity_against(&client, identity.hash, identity.state_root).is_err());
    client = crate::guard::genesis_identity_tests::TestClient::canonical();
    assert!(verify_genesis_identity_against(&client, identity.hash, identity.state_root).is_err());
}

#[test]
fn invalid_or_unreadable_genesis_always_stops_startup() {
    let canonical = crate::guard::genesis_identity_tests::TestClient::canonical();
    let mut cases = Vec::new();
    let mut client = canonical.clone();
    client.recorded_hash = H256::repeat_byte(1);
    cases.push(("客户端创世哈希不匹配", client));
    let mut client = canonical.clone();
    client.indexed_hash = Some(H256::repeat_byte(1));
    cases.push(("块 0 索引哈希不匹配", client));
    let mut client = canonical.clone();
    client.indexed_hash = None;
    cases.push(("块 0 哈希不存在", client));
    let mut client = canonical.clone();
    client.header = None;
    cases.push(("创世区块头不存在", client));
    let mut client = canonical.clone();
    client.hash_read_error = true;
    cases.push(("读取块 0 哈希失败", client));
    let mut client = canonical.clone();
    client.header_read_error = true;
    cases.push(("读取创世区块头失败", client));
    let mut client = canonical.clone();
    client
        .header
        .as_mut()
        .expect("canonical header")
        .set_number(1);
    cases.push(("创世区块高度不匹配", client));
    let mut client = canonical.clone();
    client
        .header
        .as_mut()
        .expect("canonical header")
        .set_state_root(H256::repeat_byte(1));
    cases.push(("创世状态根不匹配", client));
    let mut client = canonical;
    client
        .header
        .as_mut()
        .expect("canonical header")
        .set_extrinsics_root(H256::repeat_byte(1));
    cases.push(("创世区块头重算哈希不匹配", client));

    for (label, client) in cases {
        let error = verify_genesis_identity(&client).expect_err(label);
        assert!(error.contains(label), "{label}: {error}");
        let mut startup_continued = false;
        let rejected = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            enforce_genesis_identity(&client);
            startup_continued = true;
        }));
        assert!(rejected.is_err(), "{label} must stop startup");
        assert!(!startup_continued, "{label} must not continue assembly");
    }
}
