use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::query::{With, Without};
use bevy_ecs::system::{Commands, Query};
use campfire_protocol::secp256k1::{Secp256k1, VerifyOnly};
use campfire_protocol::{CertificateHash, ConnectChallenge, Delegation, SessionTerms};
use lightyear::prelude::server::ClientOf;
use lightyear::prelude::{
    Connected, MessageReceiver, MessageSender, Tick as NetTick, Unlink, UnlinkReason,
};
use tracing::debug;

use crate::error::JoinError;
use crate::join::Join;
use crate::net_protocol::MatchChannel;
use crate::offer::Offer;
use crate::server_setup::ServerSetup;
use crate::session_times::SessionTimes;
use crate::sim_server::PlayerLink;
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
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Refused(pub JoinError);

/// A link whose seat a newer login of its player took: the server tells its client, then ends
/// it once the word went out.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Superseding {
    told: bool,
}

impl Superseding {
    /// Ends `link`'s seat, as a newer login of its player took it: the link carries the slot no
    /// more, receives no offer, and ends once its client is told.
    pub(crate) fn mark(commands: &mut Commands<'_, '_>, link: Entity) {
        if let Ok(mut older) = commands.get_entity(link) {
            older
                .remove::<(Joined, PlayerLink)>()
                .insert((Refused(JoinError::Superseded), Superseding { told: false }));
        }
    }

    /// Tells each superseded link's client.
    pub(crate) fn tell(
        mut links: Query<'_, '_, (&mut Superseding, &mut MessageSender<Superseded>)>,
    ) {
        for (mut superseding, mut sender) in &mut links {
            if !superseding.told {
                sender.send::<MatchChannel>(Superseded);
                superseding.told = true;
            }
        }
    }

    /// Ends each superseded link whose client was told, once its messages went out.
    pub(crate) fn end(
        links: Query<'_, '_, (Entity, &Superseding)>,
        mut commands: Commands<'_, '_>,
    ) {
        for (link, superseding) in &links {
            if superseding.told {
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
