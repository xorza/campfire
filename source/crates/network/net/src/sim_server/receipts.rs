use std::time::Duration;

use bevy_ecs::resource::Resource;
use bevy_ecs::system::{Query, Res, ResMut};
use campfire_common::{Seat, Tick};
use campfire_net_messages::MessageSender;
use campfire_net_timeline::Clock;
use campfire_protocol::{DurableHead, Receipt, SessionId, SignedReceipt};
use campfire_runner::Session;

use crate::net_protocol::MatchChannel;
use crate::sim_server::journal_watch::JournalWatch;
use crate::sim_server::seat_links::SeatLinks;

/// How often the server gives receipts.
const INTERVAL: Duration = Duration::from_secs(1);

/// The receipts the server gave: about once a second, for each seated player whose chain stands
/// further in the journal's synced records than their last receipt said, the server signs where
/// it stands and sends it.
#[derive(Resource, Debug)]
pub(crate) struct Receipts {
    /// The last durable head each seat's player got a receipt of, by seat.
    given: Vec<Option<DurableHead>>,
    /// When the next round goes out, by the server's real clock.
    next: Duration,
}

impl Receipts {
    pub(crate) fn new(seats: u32) -> Self {
        Self {
            given: vec![None; usize::try_from(seats).expect("seats fit usize")],
            next: Duration::ZERO,
        }
    }

    /// Gives each seated player whose durable head moved a receipt of it, once a second.
    pub(crate) fn give(
        clock: Res<'_, Clock>,
        mut receipts: ResMut<'_, Self>,
        mut session: ResMut<'_, Session>,
        journal: Option<Res<'_, JournalWatch>>,
        seat_links: Res<'_, SeatLinks>,
        mut links: Query<'_, '_, &mut MessageSender<SignedReceipt>>,
    ) {
        let now = clock.now().elapsed();
        if now < receipts.next {
            return;
        }
        receipts.next = now + INTERVAL;
        let Some(durable) = journal.map(|journal| journal.0.durable()) else {
            return;
        };
        session
            .journal_mut()
            .expect("a session with a journal's watch keeps its journal")
            .advance_durable(durable);
        let signer = session
            .signer()
            .expect("a server's session holds its signer");
        let log = session.log();
        let journal = session
            .journal()
            .expect("a session with a journal's watch keeps its journal");
        for (seat, seat_link) in seat_links.iter() {
            let (Some(link), Some(head)) = (seat_link.link, journal.durable_head(seat)) else {
                continue;
            };
            let given = &mut receipts.given[seat.index()];
            if *given == Some(head) {
                continue;
            }
            let Ok(mut sender) = links.get_mut(link) else {
                continue;
            };
            let receipt = Self::receipt(log.session_id(), seat, log.next_tick(), head);
            sender.send::<MatchChannel>(SignedReceipt {
                receipt,
                signature: signer.sign(&receipt),
            });
            *given = Some(head);
        }
    }

    const fn receipt(session_id: SessionId, seat: Seat, tick: Tick, head: DurableHead) -> Receipt {
        Receipt {
            session_id,
            seat,
            delegation: head.delegation,
            tick,
            seq: head.seq,
            head: head.head,
        }
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use std::time::Duration;

    use crate::sim_server::receipts::Receipts;

    impl Receipts {
        /// Makes the next round go out in the server's next update, as if its second passed.
        pub(crate) const fn due_now(&mut self) {
            self.next = Duration::ZERO;
        }

        /// Holds the next round until `due_now`, so a test, not the real clock, says when it
        /// goes out.
        pub(crate) const fn hold(&mut self) {
            self.next = Duration::MAX;
        }
    }
}
