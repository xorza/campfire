//! An endpoint on a client's or a server's entity: its sessions, its IO, and the driver that runs
//! its pass, the IO thread on the OS clock or the frame on a driven one.

use std::sync::{Arc, Weak};

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::lifecycle::HookContext;
use bevy_ecs::observer::On;
use bevy_ecs::query::Without;
use bevy_ecs::system::{Commands, Query, Res};
use bevy_ecs::world::DeferredWorld;
use campfire_net_link::{Link, LinkOf, LinkStart, Linked, Linking, LocalAddr, PeerAddr, Unlink};
use campfire_net_packets::{
    CloseSession, DatagramIo, EndpointParts, EndpointRuntime, EndpointTime, News, Outbound,
    PassScratch, PeerKey,
};
use campfire_net_payload::{AppCode, UnlinkReason};
use campfire_net_timeline::Clock;
use campfire_store::Worker;

use crate::link_binding::LinkBinding;
use crate::session_link::SessionLink;

/// The datagrams, outbound datagrams and carried sessions a pass or a frame reuses.
#[derive(Debug, Default)]
struct Scratch {
    pass: PassScratch,
    outbound: Vec<Outbound>,
    carried: Vec<(PeerKey, Entity)>,
}

/// What runs the endpoint's pass.
#[derive(Debug)]
enum Driver {
    /// Before the first frame, which tells whether the clock is the OS's.
    Pending,
    /// The frame, at each of its two steps, on a driven clock.
    Inline,
    /// A thread of its own, on the OS clock, which the endpoint's drop joins.
    Thread(Worker),
}

/// An endpoint: on a client's entity, its one session, which the entity carries; on a server's,
/// a session for each peer, each carried by a `LinkOf` it spawns. A `LinkStart` opens it. Its
/// drop closes every session with `AppCode::DROPPED` and joins its thread.
#[derive(Component, Debug)]
#[component(on_discard = Self::on_discard)]
pub struct Endpoint<Io: DatagramIo> {
    runtime: Arc<EndpointRuntime<Io, Entity>>,
    driver: Driver,
    scratch: Scratch,
}

impl<Io: DatagramIo> From<EndpointParts<Io>> for Endpoint<Io> {
    fn from(parts: EndpointParts<Io>) -> Self {
        Self {
            runtime: Arc::new(EndpointRuntime::new(parts)),
            driver: Driver::Pending,
            scratch: Scratch::default(),
        }
    }
}

impl<Io: DatagramIo> Endpoint<Io> {
    /// The endpoint's IO, through which a caller reaches what its kind offers, as a simulated
    /// network's server takes each client's wire.
    pub fn io(&self) -> &Io {
        self.runtime.io()
    }

    /// The time base the clock gives: the OS's origin on a real clock, the frame's instant on a
    /// driven one.
    fn time(clock: &Clock) -> EndpointTime {
        clock
            .os_origin()
            .map_or_else(|| EndpointTime::Driven(clock.now()), EndpointTime::Os)
    }

    /// A `LinkStart` opens the endpoint: a client connects and links once its session opens, and
    /// a server accepts and is linked at once. An endpoint that is linked or linking already
    /// keeps its sessions.
    pub(crate) fn link_start(
        start: On<'_, '_, LinkStart>,
        mut endpoints: Query<'_, '_, &mut Self, (Without<Linked>, Without<Linking>)>,
        clock: Res<'_, Clock>,
        mut commands: Commands<'_, '_>,
    ) {
        let entity = start.entity;
        let Ok(mut endpoint) = endpoints.get_mut(entity) else {
            return;
        };
        let endpoint = &mut *endpoint;
        let client = {
            let mut core = endpoint.runtime.lock();
            core.set_time(Self::time(&clock));
            core.start(entity, &mut endpoint.scratch.outbound);
            core.is_client()
        };
        endpoint.runtime.send(&mut endpoint.scratch.outbound);
        if client {
            commands.entity(entity).insert(Linking);
        } else {
            commands.entity(entity).insert(Linked);
        }
    }

    /// A server's `Unlink` stops its taking of new peers; the stack unlinks each of its links,
    /// which closes their sessions.
    pub(crate) fn unlink(unlink: On<'_, '_, Unlink>, endpoints: Query<'_, '_, &Self>) {
        if let Ok(endpoint) = endpoints.get(unlink.entity) {
            endpoint.runtime.lock().stop_accepting();
        }
    }

    /// The frame's first step: on a driven clock the pass runs here; the news then link and
    /// unlink entities, and each carried session hands its link's queues what it has.
    pub(crate) fn receive(
        mut endpoints: Query<'_, '_, (Entity, &mut Self)>,
        mut links: Query<'_, '_, &mut Link>,
        clock: Res<'_, Clock>,
        mut commands: Commands<'_, '_>,
    ) {
        for (entity, mut endpoint) in &mut endpoints {
            let endpoint = &mut *endpoint;
            endpoint.drive(&clock);
            let runtime = &endpoint.runtime;
            let mut core = runtime.lock();
            while let Some(news) = core.next_news() {
                match news {
                    News::Linked {
                        key,
                        address,
                        binding,
                    } => {
                        let weak = Arc::downgrade(runtime);
                        let closer: Weak<dyn CloseSession> = weak;
                        let link = SessionLink::new(closer, key);
                        let mtu = runtime.io().mtu(address);
                        let carrier = if core.is_client() {
                            commands.entity(entity).insert((
                                PeerAddr(address),
                                LocalAddr(runtime.io().local_address()),
                                link,
                            ));
                            core.carry(key, entity, mtu);
                            entity
                        } else {
                            // The new link holds its session's budget and largest payload before
                            // it is linked, as the transport sizes its fragments at `Linked`.
                            let carrier = commands.spawn_empty().id();
                            core.carry(key, carrier, mtu);
                            let mut queues = Link::default();
                            core.hand_over(key, &mut queues.session);
                            commands.entity(carrier).insert((
                                LinkOf { server: entity },
                                queues,
                                PeerAddr(address),
                                link,
                            ));
                            carrier
                        };
                        // `Linked` goes in a command of its own, after what the link's spawn
                        // makes an app add to it.
                        if let Some(binding) = binding {
                            commands.entity(carrier).insert(LinkBinding(binding));
                        }
                        commands.entity(carrier).insert(Linked);
                    }
                    News::Ended { carrier, reason } => {
                        commands.trigger(Unlink {
                            entity: carrier,
                            reason,
                        });
                    }
                }
            }
            let carried = &mut endpoint.scratch.carried;
            carried.clear();
            carried.extend(core.carried());
            for &(key, carrier) in carried.iter() {
                let Ok(mut link) = links.get_mut(carrier) else {
                    continue;
                };
                core.hand_over(key, &mut link.session);
            }
        }
    }

    /// The frame's second step: each carried session takes its link's queues, its datagrams go
    /// out at once, and the pass runs here on a driven clock or wakes the thread on a real one.
    pub(crate) fn send(
        mut endpoints: Query<'_, '_, &mut Self>,
        mut links: Query<'_, '_, &mut Link>,
        clock: Res<'_, Clock>,
    ) {
        for mut endpoint in &mut endpoints {
            let endpoint = &mut *endpoint;
            {
                let mut core = endpoint.runtime.lock();
                if matches!(endpoint.driver, Driver::Inline) {
                    core.set_time(EndpointTime::Driven(clock.now()));
                }
                let Scratch {
                    outbound, carried, ..
                } = &mut endpoint.scratch;
                carried.clear();
                carried.extend(core.carried());
                for &(key, carrier) in carried.iter() {
                    if let Ok(mut link) = links.get_mut(carrier) {
                        core.take_over(key, &mut link.session, outbound);
                    }
                }
            }
            endpoint.runtime.send(&mut endpoint.scratch.outbound);
            match endpoint.driver {
                Driver::Inline => endpoint.runtime.pass(&mut endpoint.scratch.pass),
                Driver::Thread(_) | Driver::Pending => endpoint.runtime.io().wake(),
            }
        }
    }

    /// Resolves the driver at the first frame, and runs the frame's pass on a driven clock.
    fn drive(&mut self, clock: &Clock) {
        if matches!(self.driver, Driver::Pending) {
            self.driver = match clock.os_origin() {
                Some(origin) => {
                    self.runtime.lock().set_time(EndpointTime::Os(origin));
                    let runtime = Arc::clone(&self.runtime);
                    Driver::Thread(Worker::start("campfire-session", move || runtime.run()))
                }
                None => Driver::Inline,
            };
        }
        match &mut self.driver {
            Driver::Inline => {
                self.runtime
                    .lock()
                    .set_time(EndpointTime::Driven(clock.now()));
                self.runtime.pass(&mut self.scratch.pass);
            }
            Driver::Thread(worker) => worker.check(),
            Driver::Pending => unreachable!("the driver resolves above"),
        }
    }

    /// Ends the link of a started endpoint's entity as the endpoint leaves it, with the code its
    /// drop closes each session with.
    fn on_discard(mut world: DeferredWorld<'_>, context: HookContext) {
        let entity = context.entity;
        if world.get::<Linked>(entity).is_none() && world.get::<Linking>(entity).is_none() {
            return;
        }
        world.commands().trigger(Unlink {
            entity,
            reason: UnlinkReason::Closed(AppCode::DROPPED),
        });
    }
}

impl<Io: DatagramIo> Drop for Endpoint<Io> {
    fn drop(&mut self) {
        {
            let mut core = self.runtime.lock();
            core.close_all(&mut self.scratch.outbound);
        }
        self.runtime.send(&mut self.scratch.outbound);
        self.runtime.io().wake();
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use bevy_ecs::entity::Entity;
    use campfire_net_packets::DatagramIo;
    use campfire_net_packets::internals::session_counts::SessionCounts;

    use crate::endpoint::Endpoint;

    impl<Io: DatagramIo> Endpoint<Io> {
        /// The counts of the session that `carrier`, the endpoint's entity or one of its links,
        /// carries.
        pub fn session_counts(&self, carrier: Entity) -> Option<SessionCounts> {
            self.runtime.lock().session_counts(carrier)
        }
    }
}

#[cfg(test)]
mod tests;
