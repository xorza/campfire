use std::sync::Arc;
use std::time::Duration;

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::query::{With, Without};
use bevy_ecs::resource::Resource;
use bevy_ecs::system::{Commands, Query, Res};
use bevy_ecs::world::Mut;
use bevy_time::{Real, Time};
use campfire_common::ChannelBinding;
use campfire_log::LogEvent;
use campfire_net_connection::{ClientOf, Connected, Disconnect};
use campfire_net_core::NetTick;
use campfire_net_endpoint::LinkBinding;
use campfire_net_link::Unlinked;
use campfire_net_messages::{MessageReceiver, MessageSender};
use campfire_net_timeline::LocalTimeline;
use campfire_protocol::{
    ConnectAnswer, ConnectChallenge, Delegation, Entropy, SessionTerms, Verifier,
};
use tracing::debug;

use crate::close_code::CloseCode;
use crate::error::JoinError;
use crate::events::join_refused::JoinRefused;
use crate::net_protocol::MatchChannel;
use crate::net_protocol::dismissed::Dismissed;
use crate::net_protocol::join::{Join, JoinRole};
use crate::net_protocol::offer::Offer;
use crate::os::Clock;
use crate::session_times::SessionTimes;
use crate::sim_server::player_link::PlayerLink;
use crate::sim_server::server_setup::ServerSetup;

/// Ticks between two sends of an offer that got no answer.
const RESEND_TICKS: u32 = 30;

/// How long a link may stay silent after its first offer, which goes out as it connects, before
/// the server closes it (05-protocol-spec.md, Connection): an honest client answers the first offer
/// it receives, so a connection that only keeps itself alive holds no link.
const ANSWER_WINDOW: Duration = Duration::from_secs(10);

/// What the server offers each new link, and how it checks the answer: the session's terms, its
/// grace period and restore window, and a fresh challenge, which the player's session key signs
/// with the link's channel binding (11-quic-transport.md, The answer binds to the TLS session). The
/// lobby offers it before the start, and the door after it.
#[derive(Debug)]
pub(crate) struct Offering {
    pub(crate) terms: Arc<SessionTerms>,
    pub(crate) times: SessionTimes,
    clock: Clock,
    entropy: Entropy,
    verifier: Verifier,
}

/// The challenge a link was offered, the channel binding of its TLS session, which its answer
/// signs, the tick the offer last went out in, and when it first did, in the server's real time.
#[derive(Component, Debug, Clone, Copy)]
pub(crate) struct Offered {
    pub(crate) challenge: ConnectChallenge,
    pub(crate) binding: ChannelBinding,
    pub(crate) sent: NetTick,
    pub(crate) first: Duration,
}

/// A link whose player took a seat, before or after the start.
#[derive(Component, Debug, Clone, Copy)]
pub(crate) struct Joined;

/// Why the server refused a link's join, or took its seat: the link receives nothing more, and
/// ends as `Dismissing` has it.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub(crate) struct Refused(pub JoinError);

/// A link the server ends, for `dismissed`: the server tells its client, which ends the link once
/// the word arrives. The reliable channel sends the word again while it is lost, so the server
/// keeps the link, until `LINGER` after it first sent it.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Dismissing {
    dismissed: Dismissed,
    /// When the word first went out, in the server's real time.
    told: Option<Duration>,
}

/// How long the server keeps a dismissed link whose client has not ended it: many resends of a
/// reliable message over a link that loses its packets a while. Past it, the client is as good
/// as gone, or ignores the word, and the server ends the link itself.
const LINGER: Duration = Duration::from_secs(5);

impl Dismissing {
    /// Refuses `link` for `error`: its seat, when a newer login of its player took it, or its
    /// join. The link carries no seat, receives no offer, and ends once its client is told.
    pub(crate) fn refuse(commands: &mut Commands<'_, '_>, link: Entity, error: JoinError) {
        let dismissed = match error {
            JoinError::Superseded => Dismissed::Superseded,
            JoinError::Delegation(_)
            | JoinError::Connect(_)
            | JoinError::Full
            | JoinError::NoSeat
            | JoinError::Refused(_)
            | JoinError::NoSpectatorPlace => Dismissed::JoinRefused,
        };
        if let Ok(mut refused) = commands.get_entity(link) {
            refused.remove::<(Joined, PlayerLink)>().insert((
                Refused(error),
                Self {
                    dismissed,
                    told: None,
                },
            ));
        }
    }

    /// Tells each dismissed link's client, once.
    pub(crate) fn tell(
        time: Res<'_, Time<Real>>,
        mut links: Query<'_, '_, (&mut Self, &mut MessageSender<Dismissed>)>,
    ) {
        for (mut dismissing, mut sender) in &mut links {
            if dismissing.told.is_none() {
                sender.send::<MatchChannel>(dismissing.dismissed);
                dismissing.told = Some(time.elapsed());
            }
        }
    }

    /// Ends each dismissed link whose client was told `LINGER` ago and has not ended it.
    pub(crate) fn end(
        time: Res<'_, Time<Real>>,
        links: Query<'_, '_, (Entity, &Self), Without<Unlinked>>,
        mut commands: Commands<'_, '_>,
    ) {
        let now = time.elapsed();
        for (link, dismissing) in &links {
            if dismissing
                .told
                .is_some_and(|told| now.saturating_sub(told) >= LINGER)
            {
                commands.entity(link).remove::<Self>();
                commands.trigger(Disconnect {
                    entity: link,
                    reason: dismissing.dismissed.code().app(),
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
        Option<&'static LinkBinding>,
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
pub(crate) type ToAnswer = (Without<Joined>, Without<Refused>);

/// The offered links still to answer.
pub(crate) type JoinLinks<'w, 's> = Query<'w, 's, JoinLink, ToAnswer>;

impl Offered {
    /// Closes each offered link still to answer `ANSWER_WINDOW` after its first offer, with
    /// `CloseCode::Unanswered`.
    pub(crate) fn close_silent(
        time: Res<'_, Time<Real>>,
        links: Query<'_, '_, (Entity, &Self), (ToAnswer, Without<Unlinked>)>,
        mut commands: Commands<'_, '_>,
    ) {
        let now = time.elapsed();
        for (link, offered) in &links {
            if now.saturating_sub(offered.first) >= ANSWER_WINDOW {
                debug!(link = link.to_bits(), "the link left the offer unanswered");
                commands.trigger(Disconnect {
                    entity: link,
                    reason: CloseCode::Unanswered.app(),
                });
            }
        }
    }
}

/// The joins of one frame, checked: the links that answered to play, with their delegations, those
/// that answered to watch, and the links refused, with why.
#[derive(Debug, Default)]
pub(crate) struct CheckedJoins {
    /// In the order of the players' main keys, then their session keys.
    pub(crate) answered: Vec<(Entity, Delegation)>,
    pub(crate) watching: Vec<Entity>,
    pub(crate) refused: Vec<(Entity, JoinError)>,
}

/// When an offer goes out: the server's tick, and its real time.
#[derive(Debug, Clone, Copy)]
pub(crate) struct OfferTime {
    pub(crate) tick: NetTick,
    pub(crate) real: Duration,
}

impl Offering {
    /// The offering of a session of `terms` on the server of `server`.
    pub(crate) fn new(terms: SessionTerms, server: &ServerSetup) -> Self {
        Self {
            terms: Arc::new(terms),
            times: server.times,
            clock: server.os.clock.clone(),
            entropy: server.os.entropy,
            verifier: Verifier::new(),
        }
    }

    /// Offers the terms and a fresh challenge to each connected link that has none, and the same
    /// offer again every `RESEND_TICKS` until it answers, at `at`: a client may not yet receive
    /// messages when the link connects, and then the first offer is lost.
    pub(crate) fn offer(
        &self,
        at: OfferTime,
        links: &mut OfferLinks<'_, '_>,
        commands: &mut Commands<'_, '_>,
    ) {
        let OfferTime { tick: now, real } = at;
        for (link, offered, binding, mut sender) in links {
            let challenge = match offered {
                Some(offered) if now.0.wrapping_sub(offered.sent.0) < RESEND_TICKS => continue,
                Some(mut offered) => {
                    debug!(link = link.to_bits(), "sent the offer again");
                    offered.sent = now;
                    offered.challenge
                }
                None => {
                    let mut challenge = [0; 32];
                    self.entropy.fill(&mut challenge);
                    let challenge = ConnectChallenge::new(challenge);
                    let LinkBinding(binding) =
                        *binding.expect("a connected link holds its session's binding");
                    commands.entity(link).insert(Offered {
                        challenge,
                        binding,
                        sent: now,
                        first: real,
                    });
                    debug!(link = link.to_bits(), "offered the terms");
                    challenge
                }
            };
            sender.send::<MatchChannel>(Offer {
                terms: Arc::clone(&self.terms),
                times: self.times,
                challenge,
            });
        }
    }

    /// Offers the terms to each connected link that has no offer, as `offer` does, from the
    /// offering `H` holds: the lobby's before the start, the door's after it.
    pub(crate) fn offer_links<H: Resource + AsRef<Self>>(
        holder: Res<'_, H>,
        timeline: Res<'_, LocalTimeline>,
        time: Res<'_, Time<Real>>,
        mut links: OfferLinks<'_, '_>,
        mut commands: Commands<'_, '_>,
    ) {
        let at = OfferTime {
            tick: timeline.tick(),
            real: time.elapsed(),
        };
        H::as_ref(&holder).offer(at, &mut links, &mut commands);
    }

    /// The join an offered link sent, with the link and its offer; none when it sent none.
    pub(crate) fn received(
        (link, offered, mut receiver): (Entity, &Offered, Mut<'_, MessageReceiver<Join>>),
    ) -> Option<(Entity, Offered, Join)> {
        let join = receiver.receive().next()?;
        Some((link, *offered, join))
    }

    /// Refuses the join of `link` for `error`, logged; see `Dismissing::refuse`.
    pub(crate) fn refuse(commands: &mut Commands<'_, '_>, link: Entity, error: JoinError) {
        JoinRefused {
            link: link.to_bits(),
            error,
        }
        .log();
        Dismissing::refuse(commands, link, error);
    }

    /// The delegation of `join`, when its session key answered the challenge `offered` over the
    /// link's channel binding, and it grants this session, not expired by the server's clock.
    pub(crate) fn check(
        &mut self,
        offered: &Offered,
        join: &Join,
    ) -> Result<Delegation, JoinError> {
        let delegation = Delegation::parse(&join.delegation).map_err(JoinError::Delegation)?;
        let answer = ConnectAnswer {
            challenge: &offered.challenge,
            binding: &offered.binding,
        };
        answer
            .check(
                &mut self.verifier,
                &self.terms,
                &delegation,
                &join.answer,
                self.clock.unix_now(),
            )
            .map_err(JoinError::Connect)?;
        Ok(delegation)
    }

    /// Checks each of a frame's `joins`, each link's offer and join, into `checked`. A query
    /// gives the links in the order of its tables, which changes from run to run once Lightyear
    /// iterates them in parallel, so the answered ones go in an order of what they sent.
    pub(crate) fn check_all(
        &mut self,
        joins: impl Iterator<Item = (Entity, Offered, Join)>,
        checked: &mut CheckedJoins,
    ) {
        debug_assert!(
            checked.answered.is_empty()
                && checked.watching.is_empty()
                && checked.refused.is_empty()
        );
        for (link, offered, join) in joins {
            match (self.check(&offered, &join), join.role) {
                (Ok(delegation), JoinRole::Play) => checked.answered.push((link, delegation)),
                (Ok(_), JoinRole::Watch) => checked.watching.push(link),
                (Err(error), _) => checked.refused.push((link, error)),
            }
        }
        checked.answered.sort_by_key(|(_, delegation)| {
            (
                *delegation.main_key(),
                delegation.terms().session_key.serialize(),
            )
        });
    }
}

#[cfg(test)]
mod tests {
    use bevy_app::{App, Update};
    use bevy_ecs::observer::On;
    use bevy_ecs::resource::Resource;
    use bevy_ecs::system::ResMut;
    use bevy_time::{TimePlugin, TimeUpdateStrategy};
    use campfire_net_payload::AppCode;
    use campfire_net_payload::UnlinkReason;

    use super::*;

    /// Each link the systems closed, with its code and when, in real time.
    #[derive(Resource, Debug, Default)]
    struct Closes(Vec<(Entity, AppCode, Duration)>);

    #[test]
    fn a_dismissed_link_ends_after_its_linger_and_a_silent_one_after_its_window() {
        // Frames of 500 ms from real time 0. A link the server dismissed, its client told at 1 s
        // and 2 s, ends at 1 + `LINGER`, 6 s, and 7 s, with the dismissal's code; one never told
        // stays. A link offered at 0.5 s and silent ends at 0.5 + `ANSWER_WINDOW`, 10.5 s, with
        // `Unanswered`; one that joined, one refused, and one offered at 1 s that joins at 10.5 s,
        // before its window ends at 11 s, stay. Each closes once: as the stack's endpoint does, a
        // close unlinks the link.
        let mut app = App::new();
        app.add_plugins(TimePlugin);
        app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
            500,
        )));
        app.init_resource::<Closes>();
        app.add_observer(
            |close: On<'_, '_, Disconnect>,
             time: Res<'_, Time<Real>>,
             mut closes: ResMut<'_, Closes>,
             mut commands: Commands<'_, '_>| {
                closes.0.push((close.entity, close.reason, time.elapsed()));
                commands.entity(close.entity).insert(Unlinked {
                    reason: UnlinkReason::Closed(close.reason),
                });
            },
        );
        app.add_systems(Update, (Dismissing::end, Offered::close_silent));
        let dismissing = |dismissed, told: u64| Dismissing {
            dismissed,
            told: Some(Duration::from_secs(told)),
        };
        let offered = |first| Offered {
            challenge: ConnectChallenge::new([9; 32]),
            binding: ChannelBinding::new([3; 32]),
            sent: NetTick(0),
            first,
        };
        let world = app.world_mut();
        let superseded = world.spawn(dismissing(Dismissed::Superseded, 1)).id();
        let refused = world.spawn(dismissing(Dismissed::JoinRefused, 2)).id();
        let untold = world
            .spawn(Dismissing {
                dismissed: Dismissed::JoinRefused,
                told: None,
            })
            .id();
        let silent = world.spawn(offered(Duration::from_millis(500))).id();
        world.spawn((offered(Duration::ZERO), Joined));
        world.spawn((offered(Duration::ZERO), Refused(JoinError::Full)));
        let answering = world.spawn(offered(Duration::from_secs(1))).id();
        for _ in 0..23 {
            app.update();
            let now = app.world().resource::<Time<Real>>().elapsed();
            if now == Duration::from_millis(10_500) {
                app.world_mut().entity_mut(answering).insert(Joined);
            }
        }
        assert_eq!(
            app.world().resource::<Closes>().0,
            [
                (
                    superseded,
                    CloseCode::Superseded.app(),
                    Duration::from_secs(6)
                ),
                (
                    refused,
                    CloseCode::JoinRefused.app(),
                    Duration::from_secs(7)
                ),
                (
                    silent,
                    CloseCode::Unanswered.app(),
                    Duration::from_millis(10_500)
                ),
            ]
        );
        assert!(app.world().get::<Dismissing>(superseded).is_none());
        assert!(app.world().get::<Dismissing>(untold).is_some());
    }
}
