use super::*;

use crate::sync::is_sync_network_detached;
use crate::sync::prune_degraded_window;
use crate::sync::LocalSyncSample;
use crate::sync::DEGRADED_AUDIT_WINDOW;

fn suspect_sample() -> LocalSyncSample {
    LocalSyncSample {
        should_have_peers: true,
        health_peers: 0,
        is_syncing: false,
        system_peers: 0,
        raw_connected_peers: 1,
        raw_identified_peers: 1,
    }
}

#[test]
fn detects_sync_network_detached_without_block_height() {
    assert!(is_sync_network_detached(&suspect_sample()));
}

#[test]
fn does_not_degrade_when_raw_network_is_disconnected() {
    let sample = LocalSyncSample {
        raw_connected_peers: 0,
        raw_identified_peers: 0,
        ..suspect_sample()
    };
    assert!(!is_sync_network_detached(&sample));
}

#[test]
fn does_not_degrade_when_sync_peers_exist() {
    let sample = LocalSyncSample {
        health_peers: 1,
        system_peers: 1,
        ..suspect_sample()
    };
    assert!(!is_sync_network_detached(&sample));
}

#[test]
fn does_not_degrade_while_major_syncing() {
    let sample = LocalSyncSample {
        is_syncing: true,
        ..suspect_sample()
    };
    assert!(!is_sync_network_detached(&sample));
}

#[test]
fn does_not_degrade_without_identified_raw_peer() {
    let sample = LocalSyncSample {
        raw_identified_peers: 0,
        ..suspect_sample()
    };
    assert!(!is_sync_network_detached(&sample));
}

#[test]
fn degraded_window_prunes_old_entries() {
    let now = Instant::now();
    let mut events = VecDeque::from([
        now - DEGRADED_AUDIT_WINDOW - Duration::from_secs(1),
        now - Duration::from_secs(1),
    ]);
    prune_degraded_window(&mut events, now);
    assert_eq!(events.len(), 1);
}
