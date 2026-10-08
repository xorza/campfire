use std::time::Duration;

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::query::{With, Without};
use bevy_ecs::system::{Commands, Query, Res};
use bevy_time::{Real, Time};
use campfire_protocol::secp256k1::{Secp256k1, VerifyOnly};
use campfire_protocol::{CertificateHash, ConnectChallenge, Delegation, SessionTerms};
use lightyear::prelude::server::ClientOf;
use lightyear::prelude::{
    Connected, MessageReceiver, MessageSender, Tick as NetTick, Unlink, UnlinkReason, Unlinked,
};
use tracing::debug;

use crate::join::Join;
use crate::net_protocol::MatchChannel;
use crate::offer::Offer;
use crate::session_times::SessionTimes;
use crate::sim_server::error::JoinError;
use crate::sim_server::player_link::PlayerLink;
use crate::sim_server::server_setup::ServerSetup;
use crate::superseded::Superseded;

/// Ticks between two sends of an offer that got no answer.
const RESEND_TICKS: u32 = 30;

/// What the server offers each new link, and how it checks the answer: the session's terms, its
/// grace period and restore window, and a fresh challenge, which the player's session key signs
/// with the hash of the certificate the client verified. The lobby offers it before the start,
/// and the door after it.
#[derive(Debug)]
pub(crate) struct Offering {
    pub(crate) terms: SessionTerms,
    certificate: CertificateHash,
    pub(crate) times: SessionTimes,
    clock: fn() -> u64,
    entropy: fn(&mut [u8; 32]),
    secp: Secp256k1<VerifyOnly>,
}

/// The challenge a link was offered, and the tick the offer last went out in.
#[derive(Component, Debug, Clone, Copy)]
pub(crate) struct Offered {
    pub(crate) challenge: ConnectChallenge,
    sent: NetTick,
}

/// A link whose player took a slot, before or after the start.
#[derive(Component, Debug, Clone, Copy)]
pub(crate) struct Joined;

/// Why the server refused a link's join. The link stays connected and receives nothing more.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub(crate) struct Refused(pub JoinError);

/// A link whose seat a newer login of its player took: the server tells its client, which ends
/// the link once the word arrives. The reliable channel sends the word again while it is lost,
/// so the server keeps the link, until `LINGER` after it first sent it.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Superseding {
    /// When the word first went out, in the server's real time.
    told: Option<Duration>,
}

/// How long the server keeps a superseded link whose client has not ended it: many resends of a
/// reliable message over a link that loses its packets a while. Past it, the client is as good
/// as gone, and the server ends the link itself.
const LINGER: Duration = Duration::from_secs(5);

impl Superseding {
    /// Ends `link`'s seat, as a newer login of its player took it: the link carries the slot no
    /// more, receives no offer, and ends once its client is told.
    pub(crate) fn mark(commands: &mut Commands<'_, '_>, link: Entity) {
        if let Ok(mut older) = commands.get_entity(link) {
            older
                .remove::<(Joined, PlayerLink)>()
                .insert((Refused(JoinError::Superseded), Superseding { told: None }));
        }
    }

    /// Tells each superseded link's client, once.
    pub(crate) fn tell(
        time: Res<'_, Time<Real>>,
        mut links: Query<'_, '_, (&mut Superseding, &mut MessageSender<Superseded>)>,
    ) {
        for (mut superseding, mut sender) in &mut links {
            if superseding.told.is_none() {
                sender.send::<MatchChannel>(Superseded);
                superseding.told = Some(time.elapsed());
            }
        }
    }

    /// Ends each superseded link whose client was told `LINGER` ago and has not ended it.
    pub(crate) fn end(
        time: Res<'_, Time<Real>>,
        links: Query<'_, '_, (Entity, &Superseding), Without<Unlinked>>,
        mut commands: Commands<'_, '_>,
    ) {
        let now = time.elapsed();
        for (link, superseding) in &links {
            if superseding
                .told
                .is_some_and(|told| now.saturating_sub(told) >= LINGER)
            {
                commands.entity(link).remove::<Superseding>();
                commands.trigger(Unlink {
                    entity: link,
                    reason: UnlinkReason::UserRequested(Some(
                        "a newer login of the player".to_owned(),
                    )),
                });
            }
        }
    }
}

/// The connected links still to answer, with their offer if one went out.
pub(crate) type OfferLinks<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        Option<&'static mut Offered>,
        &'static mut MessageSender<Offer>,
    ),
    (
        With<Connected>,
        With<ClientOf>,
        Without<Joined>,
        Without<Refused>,
    ),
>;

/// An offered link, and the joins it sent.
pub(crate) type JoinLink = (Entity, &'static Offered, &'static mut MessageReceiver<Join>);

/// A link still to answer: not joined, and not refused.
pub(crate) type Unanswered = (Without<Joined>, Without<Refused>);

/// The offered links still to answer.
pub(crate) type JoinLinks<'w, 's> = Query<'w, 's, JoinLink, Unanswered>;

impl Offering {
    /// The offering of a session of `terms` on the server of `setup`.
    pub(crate) fn new(terms: SessionTerms, setup: &ServerSetup) -> Offering {
        Offering {
            terms,
            certificate: setup.certificate,
            times: setup.times,
            clock: setup.clock,
            entropy: setup.entropy,
            secp: Secp256k1::verification_only(),
        }
    }

    /// Offers the terms and a fresh challenge to each connected link that has none, and the same
    /// offer again every `RESEND_TICKS` until it answers: a client may not yet receive messages
    /// when the link connects, and then the first offer is lost.
    pub(crate) fn offer(
        &self,
        now: NetTick,
        links: &mut OfferLinks<'_, '_>,
        commands: &mut Commands<'_, '_>,
    ) {
        for (link, offered, mut sender) in links {
            let challenge = match offered {
                Some(offered) if now.0.wrapping_sub(offered.sent.0) < RESEND_TICKS => continue,
                Some(mut offered) => {
                    debug!(?link, "sent the offer again");
                    offered.sent = now;
                    offered.challenge
                }
                None => {
                    let mut challenge = [0; 32];
                    (self.entropy)(&mut challenge);
                    let challenge = ConnectChallenge::new(challenge);
                    commands.entity(link).insert(Offered {
                        challenge,
                        sent: now,
                    });
                    debug!(?link, "offered the terms");
                    challenge
                }
            };
            sender.send::<MatchChannel>(Offer {
                terms: self.terms.clone(),
                times: self.times,
                challenge,
            });
        }
    }

    /// The delegation of `join`, when its session key answered `challenge` over this server's
    /// certificate, and it grants this session, not expired by the server's clock.
    pub(crate) fn check(
        &self,
        challenge: ConnectChallenge,
        join: &Join,
    ) -> Result<Delegation, JoinError> {
        let delegation = Delegation::parse(&join.delegation).map_err(JoinError::Delegation)?;
        challenge
            .check(
                &self.secp,
                &self.terms,
                &self.certificate,
                &delegation,
                &join.answer,
                (self.clock)(),
            )
            .map_err(JoinError::Connect)?;
        Ok(delegation)
    }
}

#[cfg(test)]
mod tests {
    use bevy_app::{App, Update};
    use bevy_ecs::observer::On;
    use bevy_ecs::resource::Resource;
    use bevy_ecs::system::ResMut;
    use bevy_time::{TimePlugin, TimeUpdateStrategy};

    use super::*;

    #[derive(Resource, Debug, Default)]
    struct Unlinks(Vec<Entity>);

    #[test]
    fn a_superseded_link_its_client_never_ends_ends_after_its_linger() {
        // Frames of 500 ms from real time 0; the word went out at 1 s, so the link ends in the
        // frame at 6 s, 1 + `LINGER`, and once.
        let mut app = App::new();
        app.add_plugins(TimePlugin);
        app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
            500,
        )));
        app.init_resource::<Unlinks>();
        app.add_observer(
            |unlink: On<'_, '_, Unlink>, mut unlinks: ResMut<'_, Unlinks>| {
                unlinks.0.push(unlink.entity);
            },
        );
        app.add_systems(Update, Superseding::end);
        let told = Superseding {
            told: Some(Duration::from_secs(1)),
        };
        let link = app.world_mut().spawn(told).id();
        let untold = app.world_mut().spawn(Superseding { told: None }).id();
        let mut ended_at = None;
        for _ in 0..16 {
            app.update();
            let now = app.world().resource::<Time<Real>>().elapsed();
            if ended_at.is_none() && !app.world().resource::<Unlinks>().0.is_empty() {
                ended_at = Some(now);
            }
        }
        assert_eq!(app.world().resource::<Unlinks>().0, [link]);
        assert_eq!(ended_at, Some(Duration::from_secs(6)));
        assert!(app.world().get::<Superseding>(link).is_none());
        assert!(app.world().get::<Superseding>(untold).is_some());
    }
}
