use super::*;
use crate::{CatchUp, CompactCommit};
use finality_grandpa::Precommit;
use futures::future::Either;
use futures_timer::Delay;
use sc_client_api::BlockImportNotification;
use sc_utils::mpsc::{tracing_unbounded, TracingUnboundedSender};
use sp_consensus::BlockOrigin;
use sp_core::crypto::UncheckedFrom;
use substrate_test_runtime_client::runtime::{Block, Hash, Header};

#[derive(Clone)]
struct TestChainState {
    sender: TracingUnboundedSender<BlockImportNotification<Block>>,
    known_blocks: Arc<Mutex<HashMap<Hash, u64>>>,
}

impl TestChainState {
    fn new() -> (Self, ImportNotifications<Block>) {
        let (tx, rx) = tracing_unbounded("test", 100_000);
        let state = TestChainState {
            sender: tx,
            known_blocks: Arc::new(Mutex::new(HashMap::new())),
        };

        (state, rx)
    }

    fn block_status(&self) -> TestBlockStatus {
        TestBlockStatus {
            inner: self.known_blocks.clone(),
        }
    }

    fn import_header(&self, header: Header) {
        let hash = header.hash();
        let number = *header.number();
        let (tx, _rx) = tracing_unbounded("unpin-worker-channel", 10_000);
        self.known_blocks.lock().insert(hash, number);
        self.sender
            .unbounded_send(BlockImportNotification::<Block>::new(
                hash,
                BlockOrigin::File,
                header,
                false,
                None,
                tx,
            ))
            .unwrap();
    }
}

struct TestBlockStatus {
    inner: Arc<Mutex<HashMap<Hash, u64>>>,
}

impl BlockStatusT<Block> for TestBlockStatus {
    fn block_number(&self, hash: Hash) -> Result<Option<u64>, Error> {
        Ok(self.inner.lock().get(&hash).map(|x| *x))
    }
}

#[derive(Clone)]
struct TestBlockSyncRequester {
    requests: Arc<Mutex<Vec<(Hash, NumberFor<Block>)>>>,
}

impl Default for TestBlockSyncRequester {
    fn default() -> Self {
        TestBlockSyncRequester {
            requests: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

impl BlockSyncRequesterT<Block> for TestBlockSyncRequester {
    fn set_sync_fork_request(
        &self,
        _peers: Vec<sc_network_types::PeerId>,
        hash: Hash,
        number: NumberFor<Block>,
    ) {
        self.requests.lock().push((hash, number));
    }
}

fn make_header(number: u64) -> Header {
    Header::new(
        number,
        Default::default(),
        Default::default(),
        Default::default(),
        Default::default(),
    )
}

// unwrap the commit from `CommunicationIn` returning its fields in a tuple,
// panics if the given message isn't a commit
fn unapply_commit(msg: CommunicationIn<Block>) -> (u64, CompactCommit<Header>) {
    match msg {
        voter::CommunicationIn::Commit(round, commit, ..) => (round, commit),
        _ => panic!("expected commit"),
    }
}

// unwrap the catch up from `CommunicationIn` returning its inner representation,
// panics if the given message isn't a catch up
fn unapply_catch_up(msg: CommunicationIn<Block>) -> CatchUp<Header> {
    match msg {
        voter::CommunicationIn::CatchUp(catch_up, ..) => catch_up,
        _ => panic!("expected catch up"),
    }
}

fn message_all_dependencies_satisfied<F>(
    msg: CommunicationIn<Block>,
    enact_dependencies: F,
) -> CommunicationIn<Block>
where
    F: FnOnce(&TestChainState),
{
    let (chain_state, import_notifications) = TestChainState::new();
    let block_status = chain_state.block_status();

    // enact all dependencies before importing the message
    enact_dependencies(&chain_state);

    let (global_tx, global_rx) = tracing_unbounded("test", 100_000);

    let until_imported = UntilGlobalMessageBlocksImported::new(
        import_notifications,
        TestBlockSyncRequester::default(),
        block_status,
        global_rx,
        "global",
        None,
    );

    global_tx.unbounded_send(msg).unwrap();

    let work = until_imported.into_future();

    futures::executor::block_on(work).0.unwrap().unwrap()
}

fn blocking_message_on_dependencies<F>(
    msg: CommunicationIn<Block>,
    enact_dependencies: F,
) -> CommunicationIn<Block>
where
    F: FnOnce(&TestChainState),
{
    let (chain_state, import_notifications) = TestChainState::new();
    let block_status = chain_state.block_status();

    let (global_tx, global_rx) = tracing_unbounded("test", 100_000);

    let until_imported = UntilGlobalMessageBlocksImported::new(
        import_notifications,
        TestBlockSyncRequester::default(),
        block_status,
        global_rx,
        "global",
        None,
    );

    global_tx.unbounded_send(msg).unwrap();

    // NOTE: needs to be cloned otherwise it is moved to the stream and
    // dropped too early.
    let inner_chain_state = chain_state.clone();
    let work = future::select(
        until_imported.into_future(),
        Delay::new(Duration::from_millis(100)),
    )
    .then(move |res| match res {
        Either::Left(_) => panic!("timeout should have fired first"),
        Either::Right((_, until_imported)) => {
            // timeout fired. push in the headers.
            enact_dependencies(&inner_chain_state);

            until_imported
        }
    });

    futures::executor::block_on(work).0.unwrap().unwrap()
}

#[test]
fn blocking_commit_message() {
    let h1 = make_header(5);
    let h2 = make_header(6);
    let h3 = make_header(7);

    let unknown_commit = CompactCommit::<Header> {
        target_hash: h1.hash(),
        target_number: 5,
        precommits: vec![
            Precommit {
                target_hash: h2.hash(),
                target_number: 6,
            },
            Precommit {
                target_hash: h3.hash(),
                target_number: 7,
            },
        ],
        auth_data: Vec::new(), // not used
    };

    let unknown_commit =
        || voter::CommunicationIn::Commit(0, unknown_commit.clone(), voter::Callback::Blank);

    let res = blocking_message_on_dependencies(unknown_commit(), |chain_state| {
        chain_state.import_header(h1);
        chain_state.import_header(h2);
        chain_state.import_header(h3);
    });

    assert_eq!(unapply_commit(res), unapply_commit(unknown_commit()));
}

#[test]
fn commit_message_all_known() {
    let h1 = make_header(5);
    let h2 = make_header(6);
    let h3 = make_header(7);

    let known_commit = CompactCommit::<Header> {
        target_hash: h1.hash(),
        target_number: 5,
        precommits: vec![
            Precommit {
                target_hash: h2.hash(),
                target_number: 6,
            },
            Precommit {
                target_hash: h3.hash(),
                target_number: 7,
            },
        ],
        auth_data: Vec::new(), // not used
    };

    let known_commit =
        || voter::CommunicationIn::Commit(0, known_commit.clone(), voter::Callback::Blank);

    let res = message_all_dependencies_satisfied(known_commit(), |chain_state| {
        chain_state.import_header(h1);
        chain_state.import_header(h2);
        chain_state.import_header(h3);
    });

    assert_eq!(unapply_commit(res), unapply_commit(known_commit()));
}

#[test]
fn blocking_catch_up_message() {
    let h1 = make_header(5);
    let h2 = make_header(6);
    let h3 = make_header(7);

    let signed_prevote = |header: &Header| finality_grandpa::SignedPrevote {
        id: UncheckedFrom::unchecked_from([1; 32]),
        signature: UncheckedFrom::unchecked_from([1; 64]),
        prevote: finality_grandpa::Prevote {
            target_hash: header.hash(),
            target_number: *header.number(),
        },
    };

    let signed_precommit = |header: &Header| finality_grandpa::SignedPrecommit {
        id: UncheckedFrom::unchecked_from([1; 32]),
        signature: UncheckedFrom::unchecked_from([1; 64]),
        precommit: finality_grandpa::Precommit {
            target_hash: header.hash(),
            target_number: *header.number(),
        },
    };

    let prevotes = vec![signed_prevote(&h1), signed_prevote(&h3)];

    let precommits = vec![signed_precommit(&h1), signed_precommit(&h2)];

    let unknown_catch_up = finality_grandpa::CatchUp {
        round_number: 1,
        prevotes,
        precommits,
        base_hash: h1.hash(),
        base_number: *h1.number(),
    };

    let unknown_catch_up =
        || voter::CommunicationIn::CatchUp(unknown_catch_up.clone(), voter::Callback::Blank);

    let res = blocking_message_on_dependencies(unknown_catch_up(), |chain_state| {
        chain_state.import_header(h1);
        chain_state.import_header(h2);
        chain_state.import_header(h3);
    });

    assert_eq!(unapply_catch_up(res), unapply_catch_up(unknown_catch_up()));
}

#[test]
fn catch_up_message_all_known() {
    let h1 = make_header(5);
    let h2 = make_header(6);
    let h3 = make_header(7);

    let signed_prevote = |header: &Header| finality_grandpa::SignedPrevote {
        id: UncheckedFrom::unchecked_from([1; 32]),
        signature: UncheckedFrom::unchecked_from([1; 64]),
        prevote: finality_grandpa::Prevote {
            target_hash: header.hash(),
            target_number: *header.number(),
        },
    };

    let signed_precommit = |header: &Header| finality_grandpa::SignedPrecommit {
        id: UncheckedFrom::unchecked_from([1; 32]),
        signature: UncheckedFrom::unchecked_from([1; 64]),
        precommit: finality_grandpa::Precommit {
            target_hash: header.hash(),
            target_number: *header.number(),
        },
    };

    let prevotes = vec![signed_prevote(&h1), signed_prevote(&h3)];

    let precommits = vec![signed_precommit(&h1), signed_precommit(&h2)];

    let unknown_catch_up = finality_grandpa::CatchUp {
        round_number: 1,
        prevotes,
        precommits,
        base_hash: h1.hash(),
        base_number: *h1.number(),
    };

    let unknown_catch_up =
        || voter::CommunicationIn::CatchUp(unknown_catch_up.clone(), voter::Callback::Blank);

    let res = message_all_dependencies_satisfied(unknown_catch_up(), |chain_state| {
        chain_state.import_header(h1);
        chain_state.import_header(h2);
        chain_state.import_header(h3);
    });

    assert_eq!(unapply_catch_up(res), unapply_catch_up(unknown_catch_up()));
}

#[test]
fn request_block_sync_for_needed_blocks() {
    let (chain_state, import_notifications) = TestChainState::new();
    let block_status = chain_state.block_status();

    let (global_tx, global_rx) = tracing_unbounded("test", 100_000);

    let block_sync_requester = TestBlockSyncRequester::default();

    let until_imported = UntilGlobalMessageBlocksImported::new(
        import_notifications,
        block_sync_requester.clone(),
        block_status,
        global_rx,
        "global",
        None,
    );

    let h1 = make_header(5);
    let h2 = make_header(6);
    let h3 = make_header(7);

    // we create a commit message, with precommits for blocks 6 and 7 which
    // we haven't imported.
    let unknown_commit = CompactCommit::<Header> {
        target_hash: h1.hash(),
        target_number: 5,
        precommits: vec![
            Precommit {
                target_hash: h2.hash(),
                target_number: 6,
            },
            Precommit {
                target_hash: h3.hash(),
                target_number: 7,
            },
        ],
        auth_data: Vec::new(), // not used
    };

    let unknown_commit =
        || voter::CommunicationIn::Commit(0, unknown_commit.clone(), voter::Callback::Blank);

    // we send the commit message and spawn the until_imported stream
    global_tx.unbounded_send(unknown_commit()).unwrap();

    let threads_pool = futures::executor::ThreadPool::new().unwrap();
    threads_pool.spawn_ok(until_imported.into_future().map(|_| ()));

    // assert that we will make sync requests
    let assert = futures::future::poll_fn(|ctx| {
        let block_sync_requests = block_sync_requester.requests.lock();

        // we request blocks targeted by the precommits that aren't imported
        if block_sync_requests.contains(&(h2.hash(), *h2.number()))
            && block_sync_requests.contains(&(h3.hash(), *h3.number()))
        {
            return Poll::Ready(());
        }

        // NOTE: nothing in this function is future-aware (i.e nothing gets registered to wake
        // up this future), we manually wake up this task to avoid having to wait until the
        // timeout below triggers.
        ctx.waker().wake_by_ref();

        Poll::Pending
    });

    // the `until_imported` stream doesn't request the blocks immediately,
    // but it should request them after a small timeout
    let timeout = Delay::new(Duration::from_secs(60));
    let test = future::select(assert, timeout)
        .map(|res| match res {
            Either::Left(_) => {}
            Either::Right(_) => panic!("timed out waiting for block sync request"),
        })
        .map(drop);

    futures::executor::block_on(test);
}

fn test_catch_up() -> Arc<Mutex<Option<CommunicationIn<Block>>>> {
    let header = make_header(5);

    let unknown_catch_up = finality_grandpa::CatchUp {
        round_number: 1,
        precommits: vec![],
        prevotes: vec![],
        base_hash: header.hash(),
        base_number: *header.number(),
    };

    let catch_up =
        voter::CommunicationIn::CatchUp(unknown_catch_up.clone(), voter::Callback::Blank);

    Arc::new(Mutex::new(Some(catch_up)))
}

#[test]
fn block_global_message_wait_completed_return_when_all_awaited() {
    let msg_inner = test_catch_up();

    let waiting_block_1 = BlockGlobalMessage::<Block> {
        inner: msg_inner.clone(),
        target_number: 1,
    };

    let waiting_block_2 = BlockGlobalMessage::<Block> {
        inner: msg_inner,
        target_number: 2,
    };

    // waiting_block_2 is still waiting for block 2, thus this should return `None`.
    assert!(waiting_block_1.wait_completed(1).is_none());

    // Message only depended on block 1 and 2. Both have been imported, thus this should yield
    // the message.
    assert!(waiting_block_2.wait_completed(2).is_some());
}

#[test]
fn block_global_message_wait_completed_return_none_on_block_number_mismatch() {
    let msg_inner = test_catch_up();

    let waiting_block_1 = BlockGlobalMessage::<Block> {
        inner: msg_inner.clone(),
        target_number: 1,
    };

    let waiting_block_2 = BlockGlobalMessage::<Block> {
        inner: msg_inner,
        target_number: 2,
    };

    // Calling wait_completed with wrong block number should yield None.
    assert!(waiting_block_1.wait_completed(1234).is_none());

    // All blocks, that the message depended on, have been imported. Still, given the above
    // block number mismatch this should return None.
    assert!(waiting_block_2.wait_completed(2).is_none());
}

#[test]
fn metrics_cleans_up_after_itself() {
    let r = Registry::new();

    let mut m1 = Metrics::register(&r).unwrap();
    let m2 = m1.clone();

    // Add a new message to the 'queue' of m1.
    m1.waiting_messages_inc();

    // m1 and m2 are synced through the shared atomic.
    assert_eq!(1, m2.global_waiting_messages.get());

    // Drop 'queue' m1.
    drop(m1);

    // Make sure m1 cleaned up after itself, removing all messages that were left in its queue
    // when dropped from the global metric.
    assert_eq!(0, m2.global_waiting_messages.get());
}
