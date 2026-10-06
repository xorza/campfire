//! Receipts: each player of a match whose server keeps a journal gets a receipt of the inputs the
//! journal synced, keeps it, and writes it to the client's data directory; the journal the server
//! leaves when it stops holds every input a receipt names. A receipt not written is logged, and
//! the client plays on.

use std::time::Duration;
use std::{fs, thread};

use campfire_net::internals::{InProcessMatch, LinkModel, MatchSetup};
use campfire_net::{JoinState, JournalWatch, PlayerLink, ReceiptUnsaved, ServerDir, SessionDir};
use campfire_protocol::internals::TestKey;
use campfire_protocol::secp256k1::Secp256k1;
use campfire_protocol::{Controller, SignedReceipt};
use campfire_runner::Session;
use tempfile::TempDir;

#[test]
fn each_player_keeps_a_receipt_of_inputs_the_journal_holds() {
    let data = TempDir::new().unwrap();
    let mut local = InProcessMatch::new(MatchSetup::duo(
        LinkModel::PERFECT,
        InProcessMatch::SEED_CHAIN,
    ));
    local.keep_data(data.path().to_owned());
    local.start_match();
    local.play_by_team(InProcessMatch::SCENARIO_SCRIPTS);
    // 150 steps of 1/60 s: two and a half seconds of the server's clock, which gives receipts
    // once a second. A receipt names only what the journal's writer synced, by the disk's real
    // time, so the match waits for the sync, then plays one more second.
    for _ in 0..150 {
        local.step();
    }
    let watch = local.server().world().resource::<JournalWatch>().0.clone();
    while !watch.settled() {
        thread::sleep(Duration::from_millis(1));
    }
    for _ in 0..60 {
        local.step();
    }
    let world = local.server().world();
    let id = world.resource::<Session>().log().session_id();
    let server = TestKey::server().x_only_public_key().0;
    let secp = Secp256k1::verification_only();
    let mut kept = Vec::new();
    for client in 0..2 {
        let slot = world.get::<PlayerLink>(local.link(client)).unwrap().slot();
        let state = local.client(client).world().resource::<JoinState>();
        let receipt = *state.receipt().expect("a receipt of the synced inputs");
        assert_eq!(
            (receipt.receipt.session_id, receipt.receipt.slot),
            (id, slot)
        );
        assert!(
            receipt
                .receipt
                .signed_by(&secp, &server, &receipt.signature)
        );
        let file = data
            .path()
            .join(format!("client-{client}"))
            .join("receipts")
            .join(format!("{id}.receipt"));
        // The writer thread writes it in its own time: once the file holds it, it stays.
        let written = || {
            fs::read(&file)
                .ok()
                .map(|bytes| SignedReceipt::decode(&bytes))
        };
        for _ in 0..1000 {
            if written() == Some(Ok(receipt)) {
                break;
            }
            thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(written(), Some(Ok(receipt)));
        kept.push(receipt.receipt);
    }

    // A crash after the journal's last sync: the log the journal rebuilds holds each slot's
    // chain past the seq its receipt names, under the delegation it names.
    local.stop_server();
    let stopped = ServerDir::open(data.path()).unwrap();
    let restored = SessionDir::find(&stopped)
        .unwrap()
        .unwrap()
        .restore()
        .unwrap()
        .unwrap();
    for receipt in kept {
        let Some(Controller::Player { chain, .. }) = restored.log.controller(receipt.slot) else {
            panic!("slot {} has no player", receipt.slot.get());
        };
        assert!(chain.next_seq() > receipt.seq, "{receipt:?}");
        let (_, delegation) = restored
            .log
            .header()
            .players()
            .find(|&(slot, _)| slot == receipt.slot)
            .unwrap();
        assert_eq!(*delegation.id(), receipt.delegation);
    }
}

#[test]
fn a_receipt_not_written_is_logged_and_the_client_plays_on() {
    // A file holds the place of client 0's receipts' directory, so its receipts are not written,
    // on every OS; client 1's are.
    let data = TempDir::new().unwrap();
    let mut local = InProcessMatch::new(MatchSetup::duo(
        LinkModel::PERFECT,
        InProcessMatch::SEED_CHAIN,
    ));
    local.keep_data(data.path().to_owned());
    let place = data.path().join("client-0").join("receipts");
    fs::write(&place, b"").unwrap();
    local.start_match();
    local.play_by_team(InProcessMatch::SCENARIO_SCRIPTS);
    // The writer fails in its own time, after a receipt came; the client's `Faults` logs it.
    let mut unsaved = Vec::new();
    for _ in 0..2000 {
        local.step();
        unsaved.extend(local.log().take::<ReceiptUnsaved>());
        if !unsaved.is_empty() {
            break;
        }
        thread::sleep(Duration::from_millis(1));
    }
    assert!(!unsaved.is_empty());
    // The client keeps its receipt and plays on.
    let state = local.client(0).world().resource::<JoinState>();
    assert!(state.receipt().is_some());
    assert!(state.clock().is_some());
    assert!(place.is_file());
}
