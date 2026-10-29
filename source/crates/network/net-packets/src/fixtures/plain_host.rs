use std::collections::BTreeMap;
use std::sync::Arc;

use bevy_platform::time::Instant as OsInstant;
use campfire_common::ChannelBinding;
use campfire_net_core::Instant;
use campfire_net_payload::{
    OutgoingPayload, PacketSessionQueues, PayloadFate, PayloadKind, PayloadToken, UnlinkReason,
};
use campfire_threads::{ThreadName, Worker};

use crate::datagram_io::DatagramIo;
use crate::datagram_io::Outbound;
use crate::endpoint_core::{EndpointTime, News, PeerKey};
use crate::endpoint_pieces::EndpointPieces;
use crate::endpoint_runtime::{EndpointRuntime, HoldTiming, PassScratch};
use crate::fixtures::hold_clock::HoldClock;

/// The carrier of a client's one packet session: the host's own.
pub const CLIENT: u32 = 0;

/// A host with no engine, for the tests of an endpoint's protocol: it runs an endpoint's frame
/// steps as `campfire_net_endpoint` does, with a number for each packet session where the ECS has
/// an entity and a [`HostLink`] where it has a link. Each payload a packet session opens is
/// accepted (11-net-stack/testing.md, A protocol's test runs on the plain host of
/// `net-packets::internals`).
#[derive(Debug)]
pub struct PlainHost<Io: DatagramIo> {
    runtime: Arc<EndpointRuntime<Io, u32>>,
    driver: Driver,
    links: BTreeMap<u32, HostLink>,
    next_carrier: u32,
    outbound: Vec<Outbound>,
    scratch: PassScratch,
    carried: Vec<(PeerKey, u32)>,
}

/// What runs the host's pass: its frame on a driven clock, or a thread on the OS clock.
#[derive(Debug)]
enum Driver {
    Inline,
    Thread(Worker),
}

/// A packet session as the host holds it: its queues, its binding, whether it linked or ended, and
/// what it opened and learned.
#[derive(Debug, Default)]
pub struct HostLink {
    pub queues: PacketSessionQueues,
    pub binding: Option<ChannelBinding>,
    pub linked: bool,
    pub ended: Option<UnlinkReason>,
    /// The bytes of each payload the packet session opened, in order.
    pub opened: Vec<Vec<u8>>,
    pub fates: Vec<PayloadFate>,
}

impl<Io: DatagramIo> PlainHost<Io> {
    /// A host on a driven clock, whose frames run the pass at the instant each names.
    pub fn driven(parts: EndpointPieces<Io>) -> Self {
        Self::new(parts, Driver::Inline)
    }

    /// A host on the OS clock since `origin`, whose IO thread runs the pass.
    pub fn os(parts: EndpointPieces<Io>, origin: OsInstant) -> Self {
        let runtime = Arc::new(EndpointRuntime::new(parts));
        runtime.lock().set_time(EndpointTime::Os(origin));
        let thread = Arc::clone(&runtime);
        let driver = Driver::Thread(Worker::start(
            ThreadName::fixture("plain-host"),
            move || {
                thread.run();
            },
        ));
        Self::with_runtime(runtime, driver)
    }

    fn new(parts: EndpointPieces<Io>, driver: Driver) -> Self {
        Self::with_runtime(Arc::new(EndpointRuntime::new(parts)), driver)
    }

    fn with_runtime(runtime: Arc<EndpointRuntime<Io, u32>>, driver: Driver) -> Self {
        Self {
            runtime,
            driver,
            links: BTreeMap::new(),
            next_carrier: CLIENT + 1,
            outbound: Vec::new(),
            scratch: PassScratch::default(),
            carried: Vec::new(),
        }
    }

    pub fn io(&self) -> &Io {
        self.runtime.io()
    }

    /// The endpoint's time: the instant a driven host's last frame named, the OS's since its
    /// origin on a thread.
    pub fn now(&self) -> Instant {
        self.runtime.lock().now()
    }

    /// Starts the endpoint at `now`: a client connects, its packet session carried by [`CLIENT`],
    /// and a server accepts.
    pub fn start(&mut self, now: Instant) {
        {
            let mut core = self.runtime.lock();
            if matches!(self.driver, Driver::Inline) {
                core.set_time(EndpointTime::Driven(now));
            }
            core.start(CLIENT, &mut self.outbound);
            if core.is_client() {
                self.links.insert(CLIENT, HostLink::default());
            }
        }
        self.runtime.send(&mut self.outbound);
    }

    /// One frame at `now`: the receive step, which accepts each opened payload and takes each
    /// fate, then the send step, as `campfire_net_endpoint`'s systems run them.
    pub fn frame(&mut self, now: Instant) {
        self.frame_holding(now, &mut ());
    }

    /// The frame of a driven host at `now`, the time of each of its holds of the endpoint's lock
    /// added to `clock`.
    pub fn timed_frame(&mut self, now: Instant, clock: &mut HoldClock) {
        assert!(
            matches!(self.driver, Driver::Inline),
            "a driven host's frame is timed"
        );
        self.frame_holding(now, clock);
    }

    /// The pass of a driven host at `now` alone, as its frame starts with, the time of each of
    /// its holds of the endpoint's lock added to `clock`.
    pub fn timed_pass(&mut self, now: Instant, clock: &mut HoldClock) {
        assert!(
            matches!(self.driver, Driver::Inline),
            "a driven host's pass is timed"
        );
        self.runtime.lock().set_time(EndpointTime::Driven(now));
        self.runtime.pass_holding(&mut self.scratch, clock);
    }

    fn frame_holding(&mut self, now: Instant, timing: &mut impl HoldTiming) {
        match &mut self.driver {
            Driver::Inline => {
                self.runtime.lock().set_time(EndpointTime::Driven(now));
                self.runtime.pass_holding(&mut self.scratch, timing);
            }
            Driver::Thread(worker) => worker.check(),
        }
        self.receive(timing);
        for link in self.links.values_mut() {
            while let Some(opened) = link.queues.pop_opened() {
                link.queues.accept(opened.number);
                link.opened.push(opened.payload.freeze().to_vec());
            }
            link.fates.extend(link.queues.fates());
        }
        self.send(timing);
    }

    fn receive(&mut self, timing: &mut impl HoldTiming) {
        timing.hold(|| {
            let mut core = self.runtime.lock();
            while let Some(news) = core.next_news() {
                match news {
                    News::Linked {
                        key,
                        address,
                        binding,
                    } => {
                        let carrier = if core.is_client() {
                            CLIENT
                        } else {
                            let carrier = self.next_carrier;
                            self.next_carrier += 1;
                            carrier
                        };
                        let link = self.links.entry(carrier).or_default();
                        link.binding = binding;
                        link.linked = true;
                        core.carry(key, carrier, self.runtime.io().mtu(address));
                    }
                    News::Ended { carrier, reason } => {
                        if let Some(link) = self.links.get_mut(&carrier) {
                            link.linked = false;
                            link.ended = Some(reason);
                        }
                    }
                }
            }
            self.carried.clear();
            self.carried.extend(core.carried());
            for &(key, carrier) in &self.carried {
                let Some(link) = self.links.get_mut(&carrier) else {
                    continue;
                };
                core.hand_over(key, &mut link.queues);
            }
        });
    }

    fn send(&mut self, timing: &mut impl HoldTiming) {
        timing.hold(|| {
            let mut core = self.runtime.lock();
            for &(key, carrier) in &self.carried {
                if let Some(link) = self.links.get_mut(&carrier) {
                    core.take_over(key, &mut link.queues, &mut self.outbound);
                }
            }
        });
        match self.driver {
            Driver::Inline => {
                self.runtime.send(&mut self.outbound);
                self.runtime.pass_holding(&mut self.scratch, timing);
            }
            Driver::Thread(_) => self.runtime.send_from_frame(&mut self.outbound),
        }
    }

    /// The packet session `carrier` carries.
    pub fn link(&self, carrier: u32) -> Option<&HostLink> {
        self.links.get(&carrier)
    }

    pub fn link_mut(&mut self, carrier: u32) -> Option<&mut HostLink> {
        self.links.get_mut(&carrier)
    }

    /// Queues `data` as a payload of kind 1 on the packet session `carrier` carries, its fate
    /// reported as `token`.
    pub fn queue(&mut self, carrier: u32, token: u64, data: &[u8]) {
        let payload = OutgoingPayload::buffer(data);
        self.link_mut(carrier)
            .expect("the carrier holds a packet session")
            .queues
            .send(PayloadToken(token), PayloadKind::new(1), payload);
    }

    /// The carriers of the packet sessions a server took, which linked, in the order they linked.
    pub fn linked(&self) -> impl Iterator<Item = u32> + '_ {
        self.links
            .iter()
            .filter(|(carrier, link)| **carrier != CLIENT && link.linked)
            .map(|(carrier, _)| *carrier)
    }
}

impl<Io: DatagramIo> Drop for PlainHost<Io> {
    fn drop(&mut self) {
        {
            self.runtime.lock().close_all(&mut self.outbound);
        }
        self.runtime.send(&mut self.outbound);
        self.runtime.io().wake();
    }
}
