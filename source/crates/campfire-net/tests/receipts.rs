//! Receipts: each player of a match whose server keeps a journal gets a receipt of the inputs the
//! journal synced, keeps it, and writes it to the client's data directory; the journal the server
//! leaves when it stops holds every input a receipt names.

use std::time::Duration;
use std::{fs, thread};

use campfire_net::internals::{LinkModel, LocalMatch, MatchSetup};
use campfire_net::{JoinState, PlayerLink, SessionDir};
use campfire_protocol::secp256k1::Secp256k1;
use campfire_protocol::{Controller, SignedReceipt};
use campfire_runner::Session;

use crate::Scratch;

#[test]
fn each_player_keeps_a_receipt_of_inputs_the_journal_holds() {
    let data = Scratch::new("receipts");
    let mut local = LocalMatch::new(MatchSetup::duo(LinkModel::PERFECT, LocalMatch::SEED_CHAIN));
    local.keep_data(data.0.clone());
    local.start_match();
    local.play_by_team(LocalMatch::SCENARIO_SCRIPTS);
    // 150 steps of 1/60 s: two and a half seconds of the server's clock, which gives receipts
    // once a second.
    for _ in 0..150 {
        local.step();
    }
    let world = local.server().world();
    let id = world.resource::<Session>().log().session_id();
    let server = LocalMatch::server_keypair().x_only_public_key().0;
    let secp = Secp256k1::verification_only();
    let mut kept = Vec::new();
    for client in 0..2 {
        let slot = world.get::<PlayerLink>(local.link(client)).unwrap().slot();
        let state = local.client(client).world().resource::<JoinState>();
        let receipt = *state.receipt().expect("a receipt within 150 steps");
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
            .0
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
    let restored = SessionDir::find(&data.0)
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
