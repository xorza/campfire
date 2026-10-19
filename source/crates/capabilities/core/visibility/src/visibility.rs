use campfire_common::NamedEnum;
use campfire_state_types::{DataKind, Recipients};

use crate::error::VisibilityDecodeError;
use crate::out_view::OutView;
use crate::setting::SettingValue;
use crate::setting::error::VisibilityError;
use crate::spectator_fog::SpectatorFog;

/// A session's visibility (10-replication-audiences.md, Each kind has a default audience, and a
/// mode may set it; 10-replication-audiences.md, Three layers; 10-replication-audiences.md, The
/// host admits spectators, and their fog is a setting; 10-replication-audiences.md, A player out of
/// play sees what its team sees, or what a spectator sees): the audience of each kind, the
/// spectators' fog, what a player out of play sees, and the most spectators the session admits. Its
/// terms carry it, so every player signs it, as its bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Visibility {
    audiences: [Recipients; DataKind::ALL.len()],
    spectators: SpectatorFog,
    out: OutView,
    max_spectators: u32,
}

impl Visibility {
    /// What no mode or host changes: each kind's default audience, every unit to spectators
    /// through any fog, a player out of play seeing what its team sees, and no spectator.
    pub const DEFAULT: Self = Self {
        audiences: Self::default_audiences(),
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
    pub(crate) const fn with_max_spectators(mut self, count: u32) -> Self {
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

    /// Its bytes, which the session id hashes (05-protocol-spec.md, Keys): a byte for each kind a
    /// mode may set, in the order `DataKind` declares them, its recipients' bits; the spectators'
    /// fog; what a player out of play sees; and the most spectators, a `u32` little-endian.
    pub fn encode(&self) -> Vec<u8> {
        let kinds = Self::settable_kinds();
        let mut bytes = Vec::with_capacity(kinds.clone().count() + 2 + 4);
        bytes.extend(kinds.map(|kind| self.audience(kind).bits()));
        bytes.push(Self::code(self.spectators));
        bytes.push(Self::code(self.out));
        bytes.extend(self.max_spectators.to_le_bytes());
        bytes
    }

    /// The visibility of `bytes`, which `encode` wrote; an error for bytes no visibility encodes
    /// as, a recipient no mode sets for its kind among them, so each visibility has one encoding.
    pub fn decode(bytes: &[u8]) -> Result<Self, VisibilityDecodeError> {
        let kinds = Self::settable_kinds();
        let length = kinds.clone().count() + 2 + 4;
        if bytes.len() != length {
            return Err(VisibilityDecodeError::Length(bytes.len()));
        }
        let mut visibility = Self::DEFAULT;
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
            SpectatorFog::at(usize::from(*fog)).ok_or(VisibilityDecodeError::Spectators)?;
        visibility.out = OutView::at(usize::from(*out)).ok_or(VisibilityDecodeError::Out)?;
        let count: [u8; 4] = count.try_into().expect("four bytes");
        visibility.max_spectators = u32::from_le_bytes(count);
        Ok(visibility)
    }

    /// The byte of `member` in the encoding: its place among its enum's members.
    fn code(member: impl NamedEnum) -> u8 {
        u8::try_from(member.index()).expect("an enum of a visibility has few members")
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
