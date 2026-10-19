use std::num::NonZeroU32;
use std::time::Duration;

use bevy_ecs::entity::Entity;
use bevy_ecs::query::With;
use bytes::Bytes;
use campfire_measure::AllocAudit;
use campfire_quic::{AddressLimit, LinkTimes, ServerSocket};
use lightyear::link::server::LinkOf;
use lightyear::link::{Link, LinkStart, Linked};

use crate::loopback::{self, ClientApp, FIRST_KEY_UPDATE, PACKET};

/// Packets each end sends in one round before the audit, so a few rounds pass each end's first
/// key update.
const ROUND: usize = 50;

/// A frame of each end that moves one packet each way: the client's send system queues it, the
/// server's receive system moves it to its link, and back, and each endpoint's thread does the
/// IO, which the window counts too. Before it, rounds of `ROUND` packets each way pass each end's
/// first key update, which allocates the next keys.
#[test]
fn a_frame_that_moves_a_packet_each_way_allocates_nothing() {
    let quic = ServerSocket::bind(
        ([127, 0, 0, 1], 0).into(),
        &loopback::secret(),
        LinkTimes::CAMPFIRE,
    )
    .unwrap();
    let (address, key) = (quic.local_address(), quic.tls_key());
    let mut server = loopback::app();
    let limit = AddressLimit::new(NonZeroU32::MIN, Duration::from_secs(60));
    let endpoint = server.world_mut().spawn(quic.serve(limit)).id();
    server.world_mut().trigger(LinkStart { entity: endpoint });
    let mut client = ClientApp::connect(address, &key);
    let mut link = None;
    loopback::until(|| {
        server.update();
        client.app.update();
        link = server
            .world_mut()
            .query_filtered::<Entity, (With<LinkOf>, With<Linked>)>()
            .iter(server.world())
            .next();
        client.is_linked() && link.is_some()
    });
    let link = link.unwrap();
    for _ in 0..FIRST_KEY_UPDATE.div_ceil(ROUND) {
        for _ in 0..ROUND {
            client.link().send(Bytes::from_static(&PACKET));
        }
        client.app.update();
        let mut taken = 0;
        loopback::until(|| {
            server.update();
            let recv = &mut server.world_mut().get_mut::<Link>(link).unwrap().recv;
            while recv.pop().is_some() {
                taken += 1;
            }
            taken == ROUND
        });
        let mut server_link = server.world_mut().get_mut::<Link>(link).unwrap();
        for _ in 0..ROUND {
            server_link.send(Bytes::from_static(&PACKET));
        }
        server.update();
        let mut taken = 0;
        loopback::until(|| {
            client.app.update();
            while client.link().recv.pop().is_some() {
                taken += 1;
            }
            taken == ROUND
        });
    }

    AllocAudit::new().run(|| {
        client.link().send(Bytes::from_static(&PACKET));
        client.app.update();
        loopback::until(|| {
            server.update();
            server
                .world_mut()
                .get_mut::<Link>(link)
                .unwrap()
                .recv
                .pop()
                .is_some()
        });
        server
            .world_mut()
            .get_mut::<Link>(link)
            .unwrap()
            .send(Bytes::from_static(&PACKET));
        server.update();
        loopback::until(|| {
            client.app.update();
            client.link().recv.pop().is_some()
        });
    });
}
