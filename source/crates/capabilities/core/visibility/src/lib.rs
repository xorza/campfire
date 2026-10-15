#![deny(clippy::float_arithmetic, clippy::iter_over_hash_type)]

use bevy_ecs::resource::Resource;
use campfire_state_types::{DataKind, Recipients};

mod error;
mod mode_visibility;
mod setting;

pub use crate::error::VisibilityDecodeError;
pub use crate::mode_visibility::{HostVisibility, ModeVisibility};
pub use crate::setting::error::VisibilityError;
pub use crate::setting::{Setting, SettingValue};

/// What a session's spectators see (design 14, D15).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SpectatorFog {
    /// Every unit, through any fog.
    All,
    /// One team's view, or all teams' together, as the spectator chooses.
    Teams,
}

/// What a player out of play sees of the other teams (design 14, D16). A match holds its
/// session's as a resource: a player that sees what a spectator sees gives no order.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OutView {
    /// What its team sees.
    Team,
    /// What a spectator sees.
    Spectators,
}

/// A session's visibility (design 14, D4, D14 to D16): the audience of each kind, the
/// spectators' fog, what a player out of play sees, and the most spectators the session
/// admits. Its terms carry it, so every player signs it, as its bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Visibility {
    audiences: [Recipients; DataKind::ALL.len()],
    spectators: SpectatorFog,
    out: OutView,
    max_spectators: u32,
}

impl SpectatorFog {
    const NAMED: [(&'static str, SpectatorFog); 2] =
        [("all", SpectatorFog::All), ("teams", SpectatorFog::Teams)];

    pub fn named(name: &str) -> Option<SpectatorFog> {
        SpectatorFog::NAMED
            .into_iter()
            .find_map(|(named, fog)| (named == name).then_some(fog))
    }

    const fn code(self) -> u8 {
        match self {
            SpectatorFog::All => 0,
            SpectatorFog::Teams => 1,
        }
    }

    const fn of_code(code: u8) -> Option<SpectatorFog> {
        match code {
            0 => Some(SpectatorFog::All),
            1 => Some(SpectatorFog::Teams),
            _ => None,
        }
    }
}

impl OutView {
    const NAMED: [(&'static str, OutView); 2] =
        [("team", OutView::Team), ("spectators", OutView::Spectators)];

    pub fn named(name: &str) -> Option<OutView> {
        OutView::NAMED
            .into_iter()
            .find_map(|(named, out)| (named == name).then_some(out))
    }

    const fn code(self) -> u8 {
        match self {
            OutView::Team => 0,
            OutView::Spectators => 1,
        }
    }

    const fn of_code(code: u8) -> Option<OutView> {
        match code {
            0 => Some(OutView::Team),
            1 => Some(OutView::Spectators),
            _ => None,
        }
    }
}

impl Visibility {
    /// What no mode or host changes: each kind's default audience, every unit to spectators
    /// through any fog, a player out of play seeing what its team sees, and no spectator.
    pub const DEFAULT: Visibility = Visibility {
        audiences: Visibility::default_audiences(),
        spectators: SpectatorFog::All,
        out: OutView::Team,
        max_spectators: 0,
    };

    /// Who beside the owner receives `kind`; none for the server's, which no client receives.
    pub const fn audience(&self, kind: DataKind) -> Recipients {
        self.audiences[kind.index()]
    }

    pub const fn spectators(&self) -> SpectatorFog {
        self.spectators
    }

    pub const fn out(&self) -> OutView {
        self.out
    }

    pub const fn max_spectators(&self) -> u32 {
        self.max_spectators
    }

    /// Admits at most `count` spectators.
    pub(crate) const fn with_max_spectators(mut self, count: u32) -> Visibility {
        self.max_spectators = count;
        self
    }

    /// Sets the setting `value` names to it; an error for a recipient the kind's audience holds or
    /// lacks whatever a mode says.
    pub fn set(&mut self, value: SettingValue) -> Result<(), VisibilityError> {
        match value {
            SettingValue::Audience(kind, recipients) => {
                let settable = kind.settable();
                let refused = recipients.without(settable);
                if refused != Recipients::NONE {
                    return Err(VisibilityError::Recipient {
                        kind,
                        recipient: refused,
                    });
                }
                self.audiences[kind.index()] = kind.audience().without(settable) | recipients;
            }
            SettingValue::Spectators(fog) => self.spectators = fog,
            SettingValue::Out(out) => self.out = out,
        }
        Ok(())
    }

    /// Its bytes, which the session id hashes (design 05, Keys): a byte for each kind a mode
    /// may set, in the order `DataKind` declares them, its recipients' bits; the spectators' fog;
    /// what a player out of play sees; and the most spectators, a `u32` little-endian.
    pub fn encode(&self) -> Vec<u8> {
        let kinds = Visibility::settable_kinds();
        let mut bytes = Vec::with_capacity(kinds.clone().count() + 2 + 4);
        bytes.extend(kinds.map(|kind| self.audience(kind).bits()));
        bytes.push(self.spectators.code());
        bytes.push(self.out.code());
        bytes.extend(self.max_spectators.to_le_bytes());
        bytes
    }

    /// The visibility of `bytes`, which `encode` wrote; an error for bytes no visibility encodes
    /// as, a recipient no mode sets for its kind among them, so each visibility has one encoding.
    pub fn decode(bytes: &[u8]) -> Result<Visibility, VisibilityDecodeError> {
        let kinds = Visibility::settable_kinds();
        let length = kinds.clone().count() + 2 + 4;
        if bytes.len() != length {
            return Err(VisibilityDecodeError::Length(bytes.len()));
        }
        let mut visibility = Visibility::DEFAULT;
        for (kind, &bits) in kinds.zip(bytes) {
            let audience = Recipients::from_bits(bits)
                .filter(|audience| {
                    let fixed = kind.audience().without(kind.settable());
                    audience.without(kind.settable()) == fixed
                })
                .ok_or(VisibilityDecodeError::Audience(kind))?;
            visibility.audiences[kind.index()] = audience;
        }
        let [fog, out, count @ ..] = &bytes[length - 6..] else {
            unreachable!("the length was checked");
        };
        visibility.spectators =
            SpectatorFog::of_code(*fog).ok_or(VisibilityDecodeError::Spectators)?;
        visibility.out = OutView::of_code(*out).ok_or(VisibilityDecodeError::Out)?;
        let count: [u8; 4] = count.try_into().expect("four bytes");
        visibility.max_spectators = u32::from_le_bytes(count);
        Ok(visibility)
    }

    /// The kinds whose audience a mode may set, in the order `DataKind` declares them.
    pub(crate) fn settable_kinds() -> impl Iterator<Item = DataKind> + Clone {
        DataKind::ALL
            .into_iter()
            .filter(|kind| kind.settable() != Recipients::NONE)
    }

    const fn default_audiences() -> [Recipients; DataKind::ALL.len()] {
        let mut audiences = [Recipients::NONE; DataKind::ALL.len()];
        let mut at = 0;
        while at < DataKind::ALL.len() {
            audiences[at] = DataKind::ALL[at].audience();
            at += 1;
        }
        audiences
    }
}

#[cfg(test)]
mod tests;
