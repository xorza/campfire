use std::num::NonZeroU32;
use std::time::Duration;

use bevy_ecs::entity::Entity;
use bevy_ecs::query::With;
use bytes::Bytes;
use campfire_common::internals::AllocAudit;
use campfire_quic::{AddressLimit, LinkTimes, QuicEndpoint};
use lightyear::link::server::LinkOf;
use lightyear::link::{Link, LinkStart, Linked};

use crate::loopback::{self, ClientApp, PACKET};

/// A frame of each end that moves one packet each way: the client's send system queues it, the
/// server's receive system moves it to its link, and back. Each endpoint's thread does the IO on
/// its own thread, which no window counts.
#[test]
fn a_frame_that_moves_a_packet_each_way_allocates_nothing() {
    let quic = QuicEndpoint::server(
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
