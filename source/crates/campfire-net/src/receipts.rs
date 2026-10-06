use std::time::Duration;

use bevy_ecs::resource::Resource;
use bevy_ecs::system::{Query, Res, ResMut};
use bevy_time::{Real, Time};
use campfire_common::{PlayerSlot, Tick};
use campfire_protocol::{DurableHead, Receipt, SessionId, SignedReceipt};
use campfire_runner::Session;
use lightyear::prelude::MessageSender;

use crate::journal_watch::JournalWatch;
use crate::net_protocol::MatchChannel;
use crate::seats::Seats;
use crate::server_signer::ServerSigner;

/// How often the server gives receipts.
const INTERVAL: Duration = Duration::from_secs(1);

/// The receipts the server gave: about once a second, for each seated player whose chain stands
/// further in the journal's synced records than their last receipt said, the server signs where
/// it stands and sends it.
#[derive(Resource, Debug)]
pub(crate) struct Receipts {
    /// The last durable head each slot's player got a receipt of, by slot.
    given: Vec<Option<DurableHead>>,
    /// When the next round goes out, by the server's real clock.
    next: Duration,
}

impl Receipts {
    pub(crate) fn new(slots: u32) -> Receipts {
        Receipts {
            given: vec![None; slots as usize],
            next: Duration::ZERO,
        }
    }

    /// Gives each seated player whose durable head moved a receipt of it, once a second.
    pub(crate) fn give(
        time: Res<'_, Time<Real>>,
        mut receipts: ResMut<'_, Receipts>,
        mut session: ResMut<'_, Session>,
        journal: Option<Res<'_, JournalWatch>>,
        signer: Res<'_, ServerSigner>,
        seats: Res<'_, Seats>,
        mut links: Query<'_, '_, &mut MessageSender<SignedReceipt>>,
    ) {
        let now = time.elapsed();
        if now < receipts.next {
            return;
        }
        receipts.next = now + INTERVAL;
        if let Some(journal) = journal {
            session.advance_durable(journal.0.durable());
        }
        let log = session.log();
        for (slot, seat) in seats.iter() {
            let (Some(link), Some(head)) = (seat.link, log.durable_head(slot)) else {
                continue;
            };
            let given = &mut receipts.given[slot.index()];
            if *given == Some(head) {
                continue;
            }
            let Ok(mut sender) = links.get_mut(link) else {
                continue;
            };
            let receipt = Receipts::receipt(log.session_id(), slot, log.next_tick(), head);
            sender.send::<MatchChannel>(SignedReceipt {
                receipt,
                signature: signer.sign_receipt(&receipt),
            });
            *given = Some(head);
        }
    }

    const fn receipt(
        session_id: SessionId,
        slot: PlayerSlot,
        tick: Tick,
        head: DurableHead,
    ) -> Receipt {
        Receipt {
            session_id,
            slot,
            delegation: head.delegation,
            tick,
            seq: head.seq,
            head: head.head,
        }
    }
}
